//! One network: its description, the built runtime, and the run thread.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;

use spikes_dna::NetworkDescription;
use spikes_model::Phase;
use spikes_network::{
    BuildOptions, Event, ModulatorMsg, ModulatorRef, Network, NeuronActor, NeuronMsg, NeuronRef, PeriodicInput,
    PoissonInput, PulseInput, Runtime, SignalInput, SimMsg,
};
use spikes_record::{RecordOptions, Recorder, RunInfo, SensorLog, Summary};
use tempo_runtime::{ClockHandle, Control, Jitter, RuntimeConfig, SimDuration, SimTime};

use crate::cluster::{Coordinator, Forwarder, NewSynapse, NodeSetup, RouteChange};
use crate::network::NetworkInfo;
use crate::sensors::{Moment, Sensor, SensorSpec};
use crate::viewer::{Hub, ViewSettings, Viewer};
use crate::{SessionError, SessionId};

/// How long a query waits for a running simulation to answer.
const QUERY_TIMEOUT: Duration = Duration::from_secs(5);

/// Where a session is in its life.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum State {
    /// The description is parsed; nothing is built.
    Loaded,
    /// Built, and not running: ready to start (or continue) a run.
    Ready,
    /// A run is in progress.
    Running,
    /// A run is in progress with the clock paused.
    Paused,
    /// Something failed; rebuild to continue.
    Failed,
}

/// How fast simulation time passes.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Speed {
    /// As fast as the machine can go.
    AsFastAsPossible,
    /// Simulated seconds per wall-clock second: 1 is real time, 0.25 is four times slower, 4 is four times faster.
    Factor(f64),
}

impl Speed {
    /// Sets the clock running at this speed (a clock left paused by an earlier run is resumed).
    fn apply(self, clock: &ClockHandle) {
        match self {
            Self::AsFastAsPossible => clock.set_as_fast_as_possible(),
            Self::Factor(f) => {
                clock.set_time_factor(1.0 / f);
                clock.resume();
            }
        }
    }

    fn check(self) -> Result<Self, SessionError> {
        match self {
            Self::Factor(f) if !(f.is_finite() && f > 0.0) => Err(SessionError::Invalid(format!(
                "speed must be positive and finite, not {f}"
            ))),
            _ => Ok(self),
        }
    }
}

/// The default shortest connection delay: 100 µs. Chemical synapses take ~0.3 ms or more to transmit, and shorter
/// delays (from neurons a few µm apart) would shrink the shards' synchronization windows to µs, making multi-shard
/// runs slower than one shard. Set `BuildSettings::min_delay` to zero for the raw `distance / speed` delays.
pub const DEFAULT_MIN_DELAY: SimDuration = SimDuration::from_micros(100);

/// How to build the network.
#[derive(Copy, Clone, Debug)]
pub struct BuildSettings {
    /// Seed for the runtime and for random initial weights.
    pub seed: u64,
    /// Worker threads.
    pub shards: usize,
    /// The shortest connection delay (see [`DEFAULT_MIN_DELAY`]).
    pub min_delay: SimDuration,
    /// Timing noise on every connection.
    pub jitter: Jitter,
    /// Report every learning weight change.
    pub record_weights: bool,
}

impl Default for BuildSettings {
    fn default() -> Self {
        let options = BuildOptions::default();
        Self {
            seed: 1,
            shards: 1,
            min_delay: DEFAULT_MIN_DELAY,
            jitter: options.jitter,
            record_weights: true,
        }
    }
}

/// How to run.
#[derive(Clone, Debug)]
pub struct RunSettings {
    /// Simulation speed.
    pub speed: Speed,
    /// Run for this long, then stop; `None` runs until stopped (or, as fast as possible, until nothing is left to
    /// do).
    pub duration: Option<SimDuration>,
    /// Record the run into this directory.
    pub record: Option<PathBuf>,
}

impl Default for RunSettings {
    fn default() -> Self {
        Self {
            speed: Speed::Factor(1.0),
            duration: None,
            record: None,
        }
    }
}

/// A synthetic input running inside the simulation, from the current time on.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum InputSpec {
    /// Each event goes to one target chosen at random, at Poisson-distributed times.
    Poisson {
        /// Neurons (by index).
        targets: Vec<u32>,
        /// Mean rate in Hz (over all targets).
        rate_hz: f64,
        /// Input per event in mV.
        mv: f32,
        /// How long; `None` for ever.
        duration: Option<SimDuration>,
    },
    /// Every target, every `period`.
    Periodic {
        /// Neurons (by index).
        targets: Vec<u32>,
        /// Interval.
        period: SimDuration,
        /// Input per pulse in mV.
        mv: f32,
        /// How long; `None` for ever.
        duration: Option<SimDuration>,
    },
    /// Every target at each of `times` (from the input's start).
    Pulses {
        /// Neurons (by index).
        targets: Vec<u32>,
        /// When, from the input's start.
        times: Vec<SimDuration>,
        /// Input per pulse in mV.
        mv: f32,
    },
    /// A formula sampled in simulated time (`t` in ms from the input's start), encoded, into each channel's neurons.
    Signal {
        /// Each channel's neurons (by index).
        channels: Vec<Vec<u32>>,
        /// The formula (see `spikes_signal::formula`).
        formula: String,
        /// Time between samples.
        sample: SimDuration,
        /// How samples become input.
        encoding: spikes_signal::SampleEncoding,
        /// The noise key (set by [`Session::set_inputs`] from the seed and the source's name).
        key: u64,
        /// How long; `None` for ever.
        duration: Option<SimDuration>,
        /// The data files the formula's `data(…)` looks up (those it uses). Part of the spec, so a reset and a
        /// saved run keep them.
        data: Vec<spikes_signal::DataFile>,
    },
}

impl InputSpec {
    /// Every neuron it reaches.
    fn targets(&self) -> Vec<u32> {
        match self {
            Self::Poisson { targets, .. } | Self::Periodic { targets, .. } | Self::Pulses { targets, .. } => {
                targets.clone()
            }
            Self::Signal { channels, .. } => channels.iter().flatten().copied().collect(),
        }
    }

    /// Checks it can run, without starting it.
    fn validate(&self, network: &NetworkInfo) -> Result<(), SessionError> {
        if let Some(&i) = self.targets().iter().find(|&&i| network.id(i).is_none()) {
            return Err(SessionError::NotFound(format!("neuron {i}")));
        }
        match self {
            Self::Poisson { rate_hz, .. } if !(rate_hz.is_finite() && *rate_hz > 0.0) => {
                Err(SessionError::Invalid(format!("rate must be positive, not {rate_hz}")))
            }
            Self::Periodic { period, .. } if period.is_zero() => {
                Err(SessionError::Invalid("the period must be positive".into()))
            }
            Self::Signal {
                formula,
                sample,
                channels,
                data,
                ..
            } => {
                if sample.is_zero() {
                    return Err(SessionError::Invalid("the sample interval must be positive".into()));
                }
                let compiled = spikes_signal::Formula::compile(formula)
                    .map_err(|e| SessionError::Invalid(format!("formula: {e}")))?;
                check_data(&compiled, data, channels.len())
            }
            _ => Ok(()),
        }
    }
}

/// Checks that every data file a formula looks up is given, can be read, and has the channels the formula reads
/// (`data("x", t, c)` needs one per channel of the source; `data("x", t)` one; `data("x", t, 3)` four).
fn check_data(
    formula: &spikes_signal::Formula,
    files: &[spikes_signal::DataFile],
    channels: usize,
) -> Result<(), SessionError> {
    use spikes_signal::DataChannel;
    let invalid = |m: String| SessionError::Invalid(m);
    let sets = spikes_signal::DataSets::load(files).map_err(|e| invalid(e.to_string()))?;
    for u in formula.data_uses() {
        let set = sets
            .get(&u.name)
            .ok_or_else(|| invalid(format!("no data file named '{}'", u.name)))?;
        let needed = match u.channel {
            DataChannel::Zero => 1,
            DataChannel::C => channels,
            DataChannel::Fixed(k) => k as usize + 1,
            DataChannel::Computed => 0,
        };
        if set.channel_count() < needed {
            return Err(invalid(format!(
                "data file '{}' has {} channel{}, the source needs {needed}",
                u.name,
                set.channel_count(),
                if set.channel_count() == 1 { "" } else { "s" }
            )));
        }
    }
    Ok(())
}

/// A trial of a run's input protocol (app plan 05 §2.1): the UI expands an input file's trials into its sources'
/// on-periods, and gives the engine the trials too, to record them and show them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trial {
    /// Its number, from 1.
    pub number: u32,
    /// Its condition (empty without conditions).
    pub condition: String,
    /// When it starts and ends, from the run's start.
    pub start: SimDuration,
    pub end: SimDuration,
}

/// A named input of an input file, as the engine runs it (app plan 05): [`Session::set_inputs`].
#[derive(Clone, Debug, PartialEq)]
pub struct InputSource {
    /// Its name (unique in the set; it keys the signal's noise, and [`Session::fire`] finds it by it).
    pub name: String,
    /// When it starts, from the run's start.
    pub start: SimDuration,
    /// What it does.
    pub input: InputSpec,
}

/// Something that happened to a session.
#[derive(Clone, Debug, PartialEq)]
pub enum SessionNotice {
    /// The state changed.
    StateChanged(State),
    /// Connections were added or pruned: fetch the network again.
    NetworkChanged,
    /// A sensor connected or disconnected: list them again.
    SensorsChanged,
    /// A run ended (reached its duration, was stopped, or ran out of events).
    RunEnded {
        /// Simulation time at the end.
        at: SimTime,
        /// The recording, if the run was recorded.
        recording: Option<Summary>,
    },
    /// Recording failed; the run itself was not affected.
    RecordingFailed(String),
    /// The run thread panicked.
    Failed(String),
}

/// A neuron's state, as of its last update.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NeuronSnapshot {
    /// Membrane potential in mV at `last_event`.
    pub membrane_mv: f64,
    /// Intrinsic-plasticity offset in mV.
    pub intrinsic_mv: f64,
    /// Firing-rate estimate in Hz.
    pub rate_hz: f64,
    /// When it last updated.
    pub last_event: SimTime,
    /// When it last spiked.
    pub last_spike: Option<SimTime>,
    /// Whether it is firing tonically.
    pub tonic: bool,
    /// Spikes so far.
    pub spikes: u64,
    /// Incoming synapses: (pre index, weight, eligibility).
    pub synapses: Vec<(u32, f32, f32)>,
}

struct Built {
    rt: Runtime,
    /// The built network (`None` for a replay, whose actors are ghosts of a recorded run).
    net: Option<Network>,
}

enum Stage {
    Loaded,
    Ready(Box<Built>),
    Running {
        control: Control<SimMsg, Event>,
        clock: ClockHandle,
    },
    Failed(String),
}

struct Inner {
    text: String,
    description: NetworkDescription,
    settings: BuildSettings,
    speed: Speed,
    phase: Stage,
    network: Arc<NetworkInfo>,
    /// For a replay: the recording.
    replay: Option<Arc<crate::replay::ReplayData>>,
    /// The built network's wiring (routes, delays, synapse parameter sets), for connections made later.
    wiring: Option<Arc<Network>>,
    release_site: Option<ModulatorRef>,
    /// Node agents to build on (see [`crate::cluster`]); empty for this process alone.
    nodes: Vec<String>,
    /// The cluster this session coordinates, once built on nodes.
    cluster: Option<Arc<Coordinator>>,
    /// This session is a node of another session's cluster.
    member: Option<Arc<Forwarder>>,
    /// A cluster's paced runs wait for every node at every window (exact), rather than each node following its copy
    /// of the clock (best effort).
    lockstep: bool,
    /// Inputs added since the build, and when: a saved run adds them again before restoring.
    inputs: Vec<(InputSpec, SimTime)>,
    /// The input set given since the build ([`Session::set_inputs`]), and its trials, which a reset applies again.
    input_set: Option<(Vec<InputSource>, Vec<Trial>)>,
    /// The node agents the network was last built on, and the synapses each node made (this one first), for
    /// [`Session::deployment`].
    built_on: (Vec<String>, Vec<u64>),
    /// Each neuron's node and shard, once asked for or a run starts (a run takes the runtime, but not the neurons'
    /// homes with it).
    homes: Option<Arc<Vec<(u32, u16)>>>,
}

struct Shared {
    id: SessionId,
    name: String,
    inner: Mutex<Inner>,
    /// Signalled when a run ends.
    changed: Condvar,
    listeners: Mutex<Vec<Sender<SessionNotice>>>,
    hub: Arc<Hub>,
    /// Sensor data of the current recorded run.
    sensor_log: Mutex<Option<SensorLog>>,
    /// Connected sensors (from any connection), and the next key for one.
    sensors: Mutex<Vec<crate::sensors::SensorEntry>>,
    next_sensor_key: std::sync::atomic::AtomicU64,
    next_sensor: std::sync::atomic::AtomicU32,
}

/// A handle to a session. Cheap to clone; every method can be called from any thread.
#[derive(Clone)]
pub struct Session {
    shared: Arc<Shared>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("id", &self.shared.id)
            .field("name", &self.shared.name)
            .field("state", &self.state())
            .finish()
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl Session {
    pub(crate) fn new(id: SessionId, name: String, text: String) -> Result<Self, SessionError> {
        let description = spikes_dna::parse(&text).map_err(SessionError::Parse)?;
        Ok(Self::described(id, name, text, description))
    }

    pub(crate) fn described(id: SessionId, name: String, text: String, description: NetworkDescription) -> Self {
        Self {
            shared: Arc::new(Shared {
                id,
                name,
                inner: Mutex::new(Inner {
                    text,
                    description,
                    settings: BuildSettings::default(),
                    speed: Speed::Factor(1.0),
                    phase: Stage::Loaded,
                    network: Arc::default(),
                    wiring: None,
                    replay: None,
                    release_site: None,
                    nodes: Vec::new(),
                    cluster: None,
                    member: None,
                    lockstep: true,
                    inputs: Vec::new(),
                    input_set: None,
                    built_on: (Vec::new(), Vec::new()),
                    homes: None,
                }),
                changed: Condvar::new(),
                listeners: Mutex::new(Vec::new()),
                hub: Hub::start(id.0),
                sensor_log: Mutex::new(None),
                next_sensor: std::sync::atomic::AtomicU32::new(0),
                sensors: Mutex::new(Vec::new()),
                next_sensor_key: std::sync::atomic::AtomicU64::new(1),
            }),
        }
    }

    /// The session's id.
    pub fn id(&self) -> SessionId {
        self.shared.id
    }

    /// The session's name.
    pub fn name(&self) -> &str {
        &self.shared.name
    }

    /// Notices from now on. Dropping the receiver unsubscribes.
    pub fn notices(&self) -> Receiver<SessionNotice> {
        let (tx, rx) = channel();
        lock(&self.shared.listeners).push(tx);
        rx
    }

    fn notify(&self, notice: SessionNotice) {
        lock(&self.shared.listeners).retain(|tx| tx.send(notice.clone()).is_ok());
    }

    fn inner(&self) -> MutexGuard<'_, Inner> {
        lock(&self.shared.inner)
    }

    /// The current state.
    pub fn state(&self) -> State {
        state_of(&self.inner())
    }

    /// Why the session failed, if it did.
    pub fn failure(&self) -> Option<String> {
        match &self.inner().phase {
            Stage::Failed(message) => Some(message.clone()),
            _ => None,
        }
    }

    /// The network description's source text.
    pub fn description_text(&self) -> String {
        self.inner().text.clone()
    }

    /// The parsed description.
    pub fn description(&self) -> NetworkDescription {
        self.inner().description.clone()
    }

    /// The built network (empty until built).
    pub fn network(&self) -> Arc<NetworkInfo> {
        Arc::clone(&self.inner().network)
    }

    /// The current simulation time: while running, how far the shards have got (to within one synchronization
    /// window); while paused, the exact pause point.
    pub fn sim_time(&self) -> SimTime {
        match &self.inner().phase {
            Stage::Ready(built) => built.rt.now(),
            // Paused: exactly where the clock stopped (the shards run up to it). Running: how far the shards have
            // got, published once per synchronization window.
            Stage::Running { clock, .. } if clock.is_paused() => clock.horizon().unwrap_or_else(|| clock.sim_time()),
            Stage::Running { clock, .. } => clock.sim_time(),
            Stage::Loaded | Stage::Failed(_) => SimTime::ZERO,
        }
    }

    /// Replaces the description. Allowed unless running; discards the built network.
    pub fn update_description(&self, text: String) -> Result<(), SessionError> {
        let mut inner = self.inner();
        expect_not_running(&inner)?;
        inner.description = spikes_dna::parse(&text).map_err(SessionError::Parse)?;
        inner.text = text;
        inner.phase = Stage::Loaded;
        inner.network = Arc::default();
        inner.wiring = None;
        inner.release_site = None;
        drop(inner);
        self.notify(SessionNotice::StateChanged(State::Loaded));
        Ok(())
    }

    /// Builds (or rebuilds) the network. Allowed unless running.
    pub fn build(&self, settings: BuildSettings) -> Result<Arc<NetworkInfo>, SessionError> {
        if settings.shards == 0 {
            return Err(SessionError::Invalid("at least one shard is needed".into()));
        }
        let mut inner = self.inner();
        expect_not_running(&inner)?;
        if let Some(data) = inner.replay.clone() {
            return Ok(self.build_replay(&mut inner, &data, settings, SimTime::ZERO));
        }
        if let Some(old) = inner.cluster.take() {
            old.close();
        }
        let cluster = if inner.nodes.is_empty() {
            None
        } else {
            Some(Coordinator::start(
                &inner.nodes,
                &inner.text,
                &inner.description,
                &settings,
            )?)
        };
        let (rt, net, network) = match self.build_network(&mut inner, settings, cluster.as_ref().map(|(_, s)| s)) {
            Ok(built) => built,
            Err(e) => {
                if let Some((coordinator, _)) = cluster {
                    coordinator.close();
                }
                inner.phase = Stage::Loaded;
                return Err(e);
            }
        };
        let cluster = match cluster {
            Some((coordinator, setup)) => {
                // Events from the other nodes may arrive as soon as the mesh is up.
                coordinator.listen(rt.event_publisher(), rt.system_publisher());
                if let Err(e) = (setup.connect)(&setup.link) {
                    coordinator.close();
                    inner.phase = Stage::Loaded;
                    return Err(e);
                }
                Some((coordinator, ()))
            }
            None => None,
        };
        let (mut rt, mut net) = (rt, net);
        let mut network = network;
        if let Some((coordinator, _)) = &cluster {
            // Each node made the connections onto its own neurons: the senders' nodes learn where their routes
            // reach, and the counts add up.
            match coordinator.exchange_reach(&mut rt) {
                Ok(others) => {
                    let mut per_node = vec![net.synapse_count];
                    per_node.extend(&others);
                    net.synapse_count += others.iter().sum::<u64>();
                    inner.built_on = (inner.nodes.clone(), per_node);
                }
                Err(e) => {
                    coordinator.close();
                    inner.phase = Stage::Loaded;
                    return Err(e);
                }
            }
            // The other nodes' neurons have their synapses there (listed only for networks of listable size).
            let rows = if net.synapse_count <= crate::network::SYNAPSE_LIST_LIMIT {
                coordinator
                    .synapses()
                    .map(|remote| [spikes_record::synapse_rows(&rt, &net), remote].concat())
            } else {
                Ok(Vec::new())
            };
            match rows {
                Ok(rows) => {
                    network = Arc::new(NetworkInfo::new(&net, &inner.description, &rows));
                    inner.network = Arc::clone(&network);
                }
                Err(e) => {
                    coordinator.close();
                    inner.phase = Stage::Loaded;
                    return Err(e);
                }
            }
            let owners = (0..network.neurons.len() as u32)
                .map(|i| network.id(i).and_then(|id| rt.node_of(id)).unwrap_or(0))
                .collect();
            coordinator.set_owners(owners);
        }
        inner.cluster = cluster.map(|(c, _)| c);
        self.shared.hub.connect(&rt, Arc::clone(&network));
        inner.phase = Stage::Ready(Box::new(Built { rt, net: Some(net) }));
        drop(inner);
        let _ = self.observe(&self.shared.hub.membrane_neurons(), true);
        self.notify(SessionNotice::StateChanged(State::Ready));
        Ok(network)
    }

    /// Puts the network back as it was built: time 0, its initial weights and state. It's built again with the
    /// last build's settings (and nodes), so its random draws are the same, and the inputs added before it first ran
    /// (such as background input) are added again; connections made or pruned since, and later inputs, are gone. A
    /// replay goes back to its start. Allowed unless running.
    pub fn reset(&self) -> Result<Arc<NetworkInfo>, SessionError> {
        let (settings, set, inputs) = {
            let inner = self.inner();
            expect_not_running(&inner)?;
            if matches!(inner.phase, Stage::Loaded) && inner.network.neurons.is_empty() {
                return Err(SessionError::Invalid("build the network first".into()));
            }
            let set = inner.input_set.clone();
            // Other inputs added before the run, besides the input set's.
            let in_set = |input: &InputSpec| set.iter().flat_map(|(s, _)| s).any(|s| &s.input == input);
            let at_start: Vec<InputSpec> = inner
                .inputs
                .iter()
                .filter(|(input, at)| *at == SimTime::ZERO && !in_set(input))
                .map(|(input, _)| input.clone())
                .collect();
            (inner.settings, set, at_start)
        };
        let network = self.build(settings)?;
        if let Some((sources, trials)) = set {
            self.set_inputs(sources, trials)?;
        }
        for input in inputs {
            self.add_input_at(input, Some(SimTime::ZERO))?;
        }
        Ok(network)
    }

    /// Gives the run its inputs (an input file's sources, app plan 05), and its trials (none without a trial
    /// schedule): once per build, before it runs (a reset applies them again; to change them, build again). Each
    /// source starts at its `start`; signals' noise is keyed by the build's seed and the source's name, so a run
    /// repeats exactly. Everything is checked before anything starts. Trials are recorded with each recorded run
    /// (`trials.arrow`) and listed by [`Self::trials`].
    ///
    /// # Errors
    /// Unless built and at time 0 without inputs given yet; for a replay; for an invalid source (with its name) or
    /// trials out of order.
    pub fn set_inputs(&self, sources: Vec<InputSource>, trials: Vec<Trial>) -> Result<(), SessionError> {
        for (i, t) in trials.iter().enumerate() {
            if t.end < t.start {
                return Err(SessionError::Invalid(format!(
                    "trial {} ends before it starts",
                    t.number
                )));
            }
            if i > 0 && t.start < trials[i - 1].start {
                return Err(SessionError::Invalid("trials must be in order of their start".into()));
            }
        }
        self.not_replay("take input")?;
        let (seed, network) = {
            let inner = self.inner();
            let state = state_of(&inner);
            let Stage::Ready(built) = &inner.phase else {
                return Err(SessionError::State {
                    state,
                    action: "give inputs to",
                });
            };
            if built.rt.now() != SimTime::ZERO || inner.input_set.is_some() {
                return Err(SessionError::Invalid(
                    "the run already has its inputs: reset or build again to change them".into(),
                ));
            }
            (inner.settings.seed, Arc::clone(&inner.network))
        };
        let mut names = std::collections::HashSet::new();
        let mut sources = sources;
        for source in &mut sources {
            if !names.insert(source.name.clone()) {
                return Err(SessionError::Invalid(format!(
                    "two sources are named '{}'",
                    source.name
                )));
            }
            if let InputSpec::Signal { key, .. } = &mut source.input {
                *key = spikes_signal::noise::key(seed, &source.name);
            }
            source
                .input
                .validate(&network)
                .map_err(|e| SessionError::Invalid(format!("input '{}': {e}", source.name)))?;
        }
        for source in &sources {
            self.add_input_at(source.input.clone(), Some(SimTime::ZERO.saturating_add(source.start)))?;
        }
        self.inner().input_set = Some((sources, trials));
        Ok(())
    }

    /// The run's trials: as given with its inputs, or, for a replay, as recorded (none without a trial schedule).
    pub fn trials(&self) -> Vec<Trial> {
        let inner = self.inner();
        if let Some(replay) = &inner.replay {
            return replay.trials.clone();
        }
        inner.input_set.as_ref().map(|(_, t)| t.clone()).unwrap_or_default()
    }

    /// Fires a pulse source of the input set now (a manual trigger: what Stimulate did), whatever its times.
    ///
    /// # Errors
    /// If there's no such pulse source; unless built.
    pub fn fire(&self, source: &str) -> Result<(), SessionError> {
        let (targets, mv) = {
            let inner = self.inner();
            let found = inner.input_set.iter().flat_map(|(s, _)| s).find(|s| s.name == source);
            match found.map(|s| &s.input) {
                Some(InputSpec::Pulses { targets, mv, .. }) => (targets.clone(), *mv),
                Some(_) => return Err(SessionError::Invalid(format!("input '{source}' isn't a pulse"))),
                None => return Err(SessionError::NotFound(format!("input '{source}'"))),
            }
        };
        self.stimulate(&targets, mv)
    }

    /// Builds the runtime and network (as one node of a cluster with `node`).
    fn build_network(
        &self,
        inner: &mut MutexGuard<'_, Inner>,
        settings: BuildSettings,
        node: Option<&NodeSetup>,
    ) -> Result<(Runtime, Network, Arc<NetworkInfo>), SessionError> {
        let mut rt = Runtime::new(RuntimeConfig {
            seed: settings.seed,
            shards: settings.shards,
            // Windows are as long as the shortest delay between shards: of connections (known when built), and of
            // direct messages (inputs, modulator releases), declared here.
            min_remote_delay: Some(spikes_network::direct_delay(&inner.description)),
            cluster: node.map(|n| n.placement.clone()),
            ..RuntimeConfig::default()
        });
        let options = BuildOptions {
            seed: settings.seed,
            min_delay: settings.min_delay,
            jitter: settings.jitter,
            record_weights: settings.record_weights,
            nodes: node.map(|n| n.node_names.clone()).unwrap_or_default(),
        };
        let net = spikes_network::build(&mut rt, &inner.description, &options);
        // Sensors belong to a built network: numbering them afresh makes a rebuilt network with the same inputs
        // reproduce (a sensor's random numbers depend on the seed and its id).
        self.shared.next_sensor.store(0, std::sync::atomic::Ordering::Relaxed);
        let rows = if net.synapse_count <= crate::network::SYNAPSE_LIST_LIMIT {
            spikes_record::synapse_rows(&rt, &net)
        } else {
            Vec::new()
        };
        let network = Arc::new(NetworkInfo::new(&net, &inner.description, &rows));
        inner.wiring = Some(Arc::new(net.clone()));
        inner.release_site =
            (!network.modulators.is_empty()).then(|| net.add_release_site(&mut rt, network.centroid()));
        if let Some(node) = node {
            rt.set_link(Arc::new(node.link.clone()));
        }
        inner.network = Arc::clone(&network);
        inner.settings = settings;
        inner.inputs.clear();
        inner.input_set = None;
        inner.built_on = (Vec::new(), Vec::new());
        inner.homes = None;
        Ok((rt, net, network))
    }

    /// Which node each neuron runs on (0: this process; on a cluster, a node agent's number in the order given to
    /// [`Self::set_nodes`], from 1), once built.
    pub fn neuron_nodes(&self) -> Option<Vec<u32>> {
        let inner = self.inner();
        let Stage::Ready(built) = &inner.phase else { return None };
        let network = &inner.network;
        Some(
            (0..network.neurons.len() as u32)
                .map(|i| network.id(i).and_then(|id| built.rt.node_of(id)).unwrap_or(0))
                .collect(),
        )
    }

    /// Where the built network runs: its nodes and their shards, each neuron's node and shard, and how its
    /// populations and sets are spread over the nodes (with their pins), once built.
    pub fn deployment(&self) -> Option<crate::deployment::Deployment> {
        let mut inner = self.inner();
        let homes = homes(&mut inner)?;
        let network = &inner.network;
        let (nodes, synapses) = &inner.built_on;
        let synapses = if synapses.is_empty() {
            vec![network.synapse_count]
        } else {
            synapses.clone()
        };
        Some(crate::deployment::Deployment::new(
            &inner.description,
            nodes,
            inner.settings.shards,
            &homes,
            &synapses,
            inner.replay.is_some(),
        ))
    }

    /// Builds node agents on these addresses from the next build on, as one cluster that this session coordinates;
    /// empty for this process alone. Each is `host:port`, or `name=host:port` to be the node a description's
    /// `node=name` pins neurons to (see [`crate::cluster`]).
    pub fn set_nodes(&self, nodes: Vec<String>) -> Result<(), SessionError> {
        let mut inner = self.inner();
        expect_not_running(&inner)?;
        if !nodes.is_empty() && inner.replay.is_some() {
            return Err(SessionError::Invalid("a replay runs in this process".into()));
        }
        inner.nodes = nodes;
        Ok(())
    }

    /// For a session built on nodes: whether paced runs keep every node in lockstep (exact; the default) or let each
    /// follow its copy of the clock (best effort: faster across a network, but spikes may arrive late, and are
    /// counted). Runs as fast as possible are always in lockstep. Applies from the next start.
    pub fn set_lockstep(&self, lockstep: bool) {
        self.inner().lockstep = lockstep;
    }

    /// The node agents this session builds on.
    pub fn nodes(&self) -> Vec<String> {
        self.inner().nodes.clone()
    }

    /// How the cluster is doing, if this session coordinates one.
    pub fn cluster_status(&self) -> Option<crate::cluster::ClusterStatus> {
        let cluster = self.coordinator()?;
        Some(crate::cluster::ClusterStatus {
            nodes: cluster.addresses(),
            link: cluster.link.stats(),
            links: cluster.links(),
            lockstep: cluster.lockstep(),
        })
    }

    /// Builds as node `setup.placement.node` of another session's cluster (for the node agent).
    pub(crate) fn build_member(&self, settings: BuildSettings, setup: NodeSetup) -> Result<(), SessionError> {
        let mut inner = self.inner();
        let (rt, net, _) = self.build_network(&mut inner, settings, Some(&setup))?;
        let forwarder = Forwarder::start(rt.subscribe_lossless(), rt.system_events(), setup.link.clone());
        crate::cluster::follow_coordinator(&setup.link, rt.clock());
        (setup.connect)(&setup.link)?;
        inner.member = Some(forwarder);
        inner.phase = Stage::Ready(Box::new(Built { rt, net: Some(net) }));
        Ok(())
    }

    /// As a node of a cluster: runs until `end`, in step with the coordinator.
    pub(crate) fn run_member(
        &self,
        end: SimTime,
        lockstep: bool,
        schedule: Option<tempo_runtime::ClockSchedule>,
    ) -> Result<(), SessionError> {
        let mut inner = self.inner();
        let built = match std::mem::replace(&mut inner.phase, Stage::Loaded) {
            Stage::Ready(built) => built,
            other => {
                let state = state_of_phase(&other);
                inner.phase = other;
                return Err(SessionError::State { state, action: "start" });
            }
        };
        let Built { rt, net } = *built;
        let clock = rt.clock();
        let stop_token = clock.stop_token();
        if let Some(forwarder) = &inner.member {
            forwarder.link().set_lockstep(lockstep);
        }
        match schedule {
            // Best effort: follow the coordinator's clock (its changes arrive over the mesh).
            Some(schedule) if !lockstep => clock.follow(schedule),
            // Lockstep: the coordinator's clock paces the cluster; this node just follows its windows.
            _ => {
                clock.unfollow();
                clock.set_as_fast_as_possible();
            }
        }
        inner.phase = Stage::Running {
            control: rt.control(),
            clock,
        };
        drop(inner);
        let session = self.clone();
        std::thread::Builder::new()
            .name(format!("node-run-{}", self.shared.id.0))
            .spawn(move || session.run_thread(rt, net, None, end, stop_token))
            .map_err(|e| SessionError::Invalid(format!("couldn't start the run thread: {e}")))?;
        Ok(())
    }

    /// Leaves the cluster: a node drops its network; a coordinator releases its nodes.
    pub fn leave_cluster(&self) {
        let (member, cluster) = {
            let mut inner = self.inner();
            (inner.member.take(), inner.cluster.take())
        };
        if let Some(cluster) = cluster {
            cluster.close();
        }
        if let Some(forwarder) = member {
            // Closing the link releases a run waiting for the coordinator; then stop it.
            forwarder.link().close();
            forwarder.stop();
            let _ = self.stop();
            self.inner().phase = Stage::Loaded;
        }
    }

    fn coordinator(&self) -> Option<Arc<Coordinator>> {
        self.inner().cluster.clone()
    }

    /// Starts (or continues) a run on a thread of its own.
    pub fn start(&self, run: RunSettings) -> Result<(), SessionError> {
        let speed = run.speed.check()?;
        let mut inner = self.inner();
        homes(&mut inner);
        let built = match std::mem::replace(&mut inner.phase, Stage::Loaded) {
            Stage::Ready(built) => built,
            other => {
                let state = state_of_phase(&other);
                inner.phase = other;
                return Err(SessionError::State { state, action: "start" });
            }
        };
        let Built { rt, net } = *built;
        let recorder = match (&run.record, &net) {
            (Some(_), None) => {
                inner.phase = Stage::Ready(Box::new(Built { rt, net }));
                return Err(SessionError::Invalid("a replay can't be recorded again".into()));
            }
            (Some(dir), Some(network)) => {
                let info = RunInfo {
                    seed: inner.settings.seed,
                    description: &inner.text,
                };
                let synapses: Result<Vec<spikes_record::SynapseRow>, _> = match &inner.cluster {
                    // Every node's synapses, as learning has left them.
                    Some(cluster) => cluster
                        .synapses()
                        .map(|remote| [spikes_record::synapse_rows(&rt, network), remote].concat()),
                    None => Ok(spikes_record::synapse_rows(&rt, network)),
                };
                let started = synapses
                    .map_err(|e| spikes_record::RecordError::Io(std::io::Error::other(e.to_string())))
                    .and_then(|synapses| {
                        Recorder::start_with(
                            dir,
                            &rt,
                            network,
                            &inner.description,
                            info,
                            RecordOptions::default(),
                            &synapses,
                        )
                    })
                    .and_then(|recorder| {
                        if let Some((_, trials)) = inner.input_set.as_ref().filter(|(_, t)| !t.is_empty()) {
                            let rows: Vec<spikes_record::TrialRow> = trials
                                .iter()
                                .map(|t| spikes_record::TrialRow {
                                    trial: t.number,
                                    condition: t.condition.clone(),
                                    start_ns: t.start.as_nanos(),
                                    end_ns: t.end.as_nanos(),
                                })
                                .collect();
                            spikes_record::write_trials(dir, &rows)?;
                        }
                        Ok((recorder, SensorLog::create(dir, RecordOptions::default().batch_rows)?))
                    });
                match started {
                    Ok((recorder, sensor_log)) => {
                        *lock(&self.shared.sensor_log) = Some(sensor_log);
                        Some(recorder)
                    }
                    Err(e) => {
                        inner.phase = Stage::Ready(Box::new(Built { rt, net }));
                        return Err(SessionError::Record(e.to_string()));
                    }
                }
            }
            (None, _) => None,
        };
        let clock = rt.clock();
        // Taken before the run thread exists, so a stop() right after start() returns can't be missed.
        let stop_token = clock.stop_token();
        speed.apply(&clock);
        inner.speed = speed;
        let end = run.duration.map_or(SimTime::MAX, |d| rt.now().saturating_add(d));
        // Best effort: every node paces from the same instant, a little in the future.
        let lockstep = inner.lockstep || matches!(speed, Speed::AsFastAsPossible);
        let schedule = match speed {
            Speed::Factor(f) if !lockstep && inner.cluster.is_some() => {
                let schedule = tempo_runtime::ClockSchedule::Running {
                    anchor_wall: std::time::SystemTime::now() + Duration::from_millis(100),
                    anchor_sim: rt.now(),
                    time_factor: 1.0 / f,
                };
                clock.follow(schedule);
                Some(schedule)
            }
            _ => {
                clock.unfollow();
                None
            }
        };
        if let Some(cluster) = &inner.cluster
            && let Err(e) = cluster.run(end, lockstep, schedule)
        {
            cluster.close();
            inner.cluster = None;
            inner.phase = Stage::Failed(e.to_string());
            drop(inner);
            self.notify(SessionNotice::StateChanged(State::Failed));
            return Err(e);
        }
        inner.phase = Stage::Running {
            control: rt.control(),
            clock,
        };
        drop(inner);

        let session = self.clone();
        std::thread::Builder::new()
            .name(format!("session-{}", self.shared.id.0))
            .spawn(move || session.run_thread(rt, net, recorder, end, stop_token))
            .map_err(|e| SessionError::Invalid(format!("couldn't start the run thread: {e}")))?;
        self.notify(SessionNotice::StateChanged(State::Running));
        Ok(())
    }

    fn run_thread(
        self,
        mut rt: Runtime,
        net: Option<Network>,
        recorder: Option<Recorder>,
        end: SimTime,
        stop_token: tempo_runtime::StopToken,
    ) {
        let mut outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            rt.run_until_unless_stopped(end, stop_token)
        }));
        // A cluster's run ends everywhere at once. The coordinator waits until every node has sent the events it
        // emitted (they precede its end report), so views and recordings are complete; a node sends them.
        let (member, cluster) = {
            let inner = self.inner();
            (inner.member.clone(), inner.cluster.clone())
        };
        if outcome.is_ok() {
            if let Some(forwarder) = member {
                forwarder.run_ended(rt.now());
            }
            if let Some(cluster) = cluster
                && let Err(e) = cluster.wait_for_run_end(rt.now())
            {
                outcome = Err(Box::new(e.to_string()));
            }
        }
        let recording = recorder.map(Recorder::finish);
        let sensor_log = lock(&self.shared.sensor_log).take().map(SensorLog::finish);
        let mut inner = self.inner();
        let notices = match outcome {
            Ok(_) => {
                // Commands that arrived as the run ended are answered now, not at the next run.
                rt.apply_control();
                let at = rt.now();
                self.shared.hub.advance(at);
                inner.phase = Stage::Ready(Box::new(Built { rt, net }));
                let mut notices = Vec::new();
                if let Some(Err(e)) = sensor_log {
                    notices.push(SessionNotice::RecordingFailed(format!("sensor data: {e}")));
                }
                let recording = match recording {
                    Some(Ok(summary)) => Some(summary),
                    Some(Err(e)) => {
                        notices.push(SessionNotice::RecordingFailed(e.to_string()));
                        None
                    }
                    None => None,
                };
                notices.push(SessionNotice::RunEnded { at, recording });
                notices.push(SessionNotice::StateChanged(State::Ready));
                notices
            }
            Err(panic) => {
                let message = panic
                    .downcast_ref::<&str>()
                    .map(|s| (*s).to_string())
                    .or_else(|| panic.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "the run panicked".into());
                inner.phase = Stage::Failed(message.clone());
                vec![
                    SessionNotice::Failed(message),
                    SessionNotice::StateChanged(State::Failed),
                ]
            }
        };
        // Notices go out before anyone can see the session as no longer running, so a caller woken by
        // `wait_for_run` (or reading the state) always finds the run's notices already sent.
        for notice in notices {
            self.notify(notice);
        }
        drop(inner);
        self.shared.changed.notify_all();
    }

    fn clock(&self) -> Result<ClockHandle, SessionError> {
        let inner = self.inner();
        match &inner.phase {
            Stage::Running { clock, .. } => Ok(clock.clone()),
            other => Err(SessionError::State {
                state: state_of_phase(other),
                action: "control the clock of",
            }),
        }
    }

    /// The runtime's counters, its lookahead (the synchronization window length) and the wall time its shards have
    /// spent waiting for each other (summed), when ready (not running).
    pub fn runtime_stats(&self) -> Option<(tempo_runtime::Stats, SimDuration, Duration)> {
        match &self.inner().phase {
            Stage::Ready(built) => Some((built.rt.stats(), built.rt.lookahead(), built.rt.sync_wait())),
            _ => None,
        }
    }

    /// Pauses a run.
    pub fn pause(&self) -> Result<(), SessionError> {
        let clock = self.clock()?;
        clock.pause();
        if let Some(cluster) = self.coordinator() {
            cluster.clock_changed(&clock);
        }
        self.notify(SessionNotice::StateChanged(State::Paused));
        Ok(())
    }

    /// Resumes a paused run at the current speed.
    pub fn resume(&self) -> Result<(), SessionError> {
        let clock = self.clock()?;
        match self.inner().speed {
            Speed::AsFastAsPossible => clock.set_as_fast_as_possible(),
            Speed::Factor(_) => clock.resume(),
        }
        if let Some(cluster) = self.coordinator() {
            cluster.clock_changed(&clock);
        }
        self.notify(SessionNotice::StateChanged(State::Running));
        Ok(())
    }

    /// Advances by `by`: while paused, lets the run go on for that long and pause again; when ready, runs that
    /// long as fast as possible before returning.
    pub fn step(&self, by: SimDuration) -> Result<(), SessionError> {
        let mut inner = self.inner();
        let cluster = inner.cluster.clone();
        match &mut inner.phase {
            Stage::Running { clock, .. } if clock.is_paused() => {
                clock.step(by);
                if let Some(cluster) = &cluster {
                    cluster.clock_changed(clock);
                }
                Ok(())
            }
            Stage::Ready(built) => {
                // As fast as possible, whatever state the last run left the clock in (a paused clock would never
                // get there). The next start sets the speed again.
                built.rt.clock().set_as_fast_as_possible();
                let end = built.rt.now().saturating_add(by);
                let cluster = inner.cluster.clone();
                let Stage::Ready(built) = &mut inner.phase else {
                    unreachable!()
                };
                if let Some(cluster) = &cluster {
                    cluster.run(end, true, None)?;
                }
                built.rt.run_until(end);
                if let Some(cluster) = &cluster {
                    cluster.wait_for_run_end(built.rt.now())?;
                }
                self.shared.hub.advance(built.rt.now());
                Ok(())
            }
            other => Err(SessionError::State {
                state: state_of_phase(other),
                action: "step",
            }),
        }
    }

    /// Changes the speed; applies to the next run when not running. A paused run stays paused.
    pub fn set_speed(&self, speed: Speed) -> Result<(), SessionError> {
        let speed = speed.check()?;
        let mut inner = self.inner();
        inner.speed = speed;
        if let Stage::Running { clock, .. } = &inner.phase {
            match speed {
                Speed::Factor(f) => clock.set_time_factor(1.0 / f),
                Speed::AsFastAsPossible if !clock.is_paused() => clock.set_as_fast_as_possible(),
                Speed::AsFastAsPossible => {} // applied on resume
            }
            if let Some(cluster) = &inner.cluster {
                cluster.clock_changed(clock);
            }
        }
        Ok(())
    }

    /// The current speed setting.
    pub fn speed(&self) -> Speed {
        self.inner().speed
    }

    /// Stops a run and waits for it to end (recording included). Does nothing unless running.
    pub fn stop(&self) -> Result<(), SessionError> {
        let mut inner = self.inner();
        if let Stage::Running { clock, .. } = &inner.phase {
            clock.stop();
            if let Some(cluster) = &inner.cluster {
                cluster.stop_nodes();
            }
        }
        while matches!(inner.phase, Stage::Running { .. }) {
            inner = self.shared.changed.wait(inner).unwrap_or_else(|p| p.into_inner());
        }
        Ok(())
    }

    /// Waits until the current run ends (by itself), or `timeout` passes. Returns whether it ended.
    pub fn wait_for_run(&self, timeout: Duration) -> bool {
        let inner = self.inner();
        let (inner, _) = self
            .shared
            .changed
            .wait_timeout_while(inner, timeout, |inner| matches!(inner.phase, Stage::Running { .. }))
            .unwrap_or_else(|p| p.into_inner());
        !matches!(inner.phase, Stage::Running { .. })
    }

    /// Starts a live view. Frames keep coming across runs and rebuilds until the viewer is dropped.
    pub fn attach(&self, settings: ViewSettings) -> Viewer {
        let membrane = settings.membrane.clone();
        let viewer = self.shared.hub.attach(settings, self.network().neurons.len());
        let _ = self.observe(&membrane, true); // fails harmlessly before the network is built
        viewer
    }

    /// What live frames have cost so far.
    pub fn frame_stats(&self) -> crate::FrameStats {
        self.shared.hub.frame_stats()
    }

    /// Changes what a viewer gets.
    pub fn update_view(&self, viewer: &Viewer, settings: ViewSettings) {
        self.update_view_id(viewer.id(), settings);
    }

    /// Like [`Self::update_view`], for the viewer with this id (whichever thread holds it).
    pub(crate) fn update_view_id(&self, viewer: u64, settings: ViewSettings) {
        let membrane = settings.membrane.clone();
        self.shared.hub.update(viewer, settings, self.network().neurons.len());
        let _ = self.observe(&membrane, true);
    }

    /// Adds a synthetic input that runs inside the simulation. Only while ready (actors can't be added during a
    /// run); a rebuild removes it.
    pub fn add_input(&self, input: InputSpec) -> Result<(), SessionError> {
        self.add_input_at(input, None)
    }

    /// Adds a synthetic input starting at `start` (default: now). On a cluster's coordinator, every node adds it.
    pub(crate) fn add_input_at(&self, input: InputSpec, start: Option<SimTime>) -> Result<(), SessionError> {
        self.not_replay("take input")?;
        let network = self.network();
        let input_copy = input.clone();
        let mut inner = self.inner();
        let state = state_of(&inner);
        let Stage::Ready(built) = &mut inner.phase else {
            return Err(SessionError::State {
                state,
                action: "add an input to",
            });
        };
        let start = start.unwrap_or_else(|| built.rt.now());
        spawn_input(&mut built.rt, &network, input, start)?;
        inner.inputs.push((input_copy.clone(), start));
        // Every node spawns it too, so the directories stay identical (it lives on one of them).
        if let Some(cluster) = inner.cluster.clone() {
            drop(inner);
            cluster.add_input(&input_copy, start)?;
        }
        Ok(())
    }

    /// The part of a snapshot of the run held here (the whole run, unless this is a node of a cluster).
    pub(crate) fn snapshot_here(&self) -> Result<tempo_runtime::Snapshot, SessionError> {
        let mut inner = self.inner();
        let state = state_of(&inner);
        let Stage::Ready(built) = &mut inner.phase else {
            return Err(SessionError::State { state, action: "save" });
        };
        built
            .rt
            .snapshot(&spikes_network::snapshot_codec())
            .map_err(|e| SessionError::Invalid(format!("couldn't save: {e}")))
    }

    /// Restores a run's snapshot into the runtime here (on a node of a cluster: the actors and events it holds).
    pub(crate) fn restore_here(&self, snapshot: &tempo_runtime::Snapshot) -> Result<(), SessionError> {
        let mut inner = self.inner();
        let state = state_of(&inner);
        let Stage::Ready(built) = &mut inner.phase else {
            return Err(SessionError::State {
                state,
                action: "load a saved run into",
            });
        };
        built
            .rt
            .restore(&spikes_network::snapshot_codec(), snapshot)
            .map_err(|e| SessionError::Invalid(format!("couldn't restore the saved run: {e}")))
    }

    /// Saves the run so far: the description, how it was built, the inputs added and every neuron's, synapse's and
    /// pending event's state (on a cluster, every node's). Allowed when ready (built, or stopped), or paused (in one
    /// process: the run stays paused); [`Self::load_saved`] continues from it, also on another number of shards or
    /// nodes.
    ///
    /// # Errors
    /// While running (pause first), or paused on a cluster (stop first); before a build; for a replay.
    pub fn save(&self) -> Result<Vec<u8>, SessionError> {
        self.save_timed().map(|(bytes, _)| bytes)
    }

    /// Like [`Self::save`], with the simulated time it was saved at.
    pub fn save_timed(&self) -> Result<(Vec<u8>, SimTime), SessionError> {
        self.not_replay("save")?;
        // A paused run (in one process) is saved where it waits; otherwise, between runs.
        let paused = {
            let inner = self.inner();
            match &inner.phase {
                Stage::Running { control, clock } => {
                    if !clock.is_paused() {
                        return Err(SessionError::State {
                            state: State::Running,
                            action: "save (pause it first)",
                        });
                    }
                    if inner.cluster.is_some() {
                        return Err(SessionError::Invalid(
                            "a run on a cluster can't be saved while paused: stop it first".into(),
                        ));
                    }
                    Some(control.clone())
                }
                _ => None,
            }
        };
        if let Some(control) = paused {
            let snapshot = self
                .await_reply(control.snapshot(Arc::new(spikes_network::snapshot_codec())))?
                .map_err(|e| SessionError::Invalid(format!("couldn't save: {e}")))?;
            return self.saved_run(snapshot);
        }
        let here = self.snapshot_here()?;
        let cluster = self.coordinator();
        let snapshot = match &cluster {
            Some(cluster) => {
                let mut parts = vec![here];
                parts.extend(cluster.save_states()?);
                tempo_runtime::Snapshot::merge(parts)
            }
            None => here,
        };
        self.saved_run(snapshot)
    }

    /// A saved run's bytes, with this session's description, settings and inputs.
    fn saved_run(&self, snapshot: tempo_runtime::Snapshot) -> Result<(Vec<u8>, SimTime), SessionError> {
        let at = SimTime::from_nanos(snapshot.now);
        let inner = self.inner();
        let saved = SavedRun {
            description: inner.text.clone(),
            seed: inner.settings.seed,
            min_delay: inner.settings.min_delay,
            jitter: inner.settings.jitter,
            record_weights: inner.settings.record_weights,
            inputs: inner.inputs.clone(),
            snapshot,
        };
        drop(inner);
        let mut bytes = SAVED_MAGIC.to_vec();
        bytes.extend(postcard::to_stdvec(&saved).map_err(|e| SessionError::Invalid(e.to_string()))?);
        Ok((bytes, at))
    }

    /// Continues a saved run (see [`Self::save`]): takes its description, builds it on `shards` shards (on the
    /// session's nodes, if it has any: see [`Self::set_nodes`]), and restores its state. The session is then ready,
    /// at the saved time. Allowed unless running.
    ///
    /// # Errors
    /// If the file isn't a saved run, or is damaged; for a replay.
    pub fn load_saved(&self, bytes: &[u8], shards: usize) -> Result<Arc<NetworkInfo>, SessionError> {
        self.not_replay("load a saved run into")?;
        let body = bytes
            .strip_prefix(SAVED_MAGIC.as_slice())
            .ok_or_else(|| SessionError::Invalid("not a saved run".into()))?;
        let saved: SavedRun =
            postcard::from_bytes(body).map_err(|e| SessionError::Invalid(format!("the saved run is damaged: {e}")))?;
        self.update_description(saved.description.clone())?;
        self.build(BuildSettings {
            seed: saved.seed,
            shards,
            min_delay: saved.min_delay,
            jitter: saved.jitter,
            record_weights: saved.record_weights,
        })?;
        for (spec, start) in &saved.inputs {
            self.add_input_at(spec.clone(), Some(*start))?;
        }
        self.restore_here(&saved.snapshot)?;
        let cluster = self.coordinator();
        if let Some(cluster) = &cluster {
            cluster.restore_state(&saved.snapshot)?;
        }
        // Connections made or pruned before saving are in the neurons now.
        let mut guard = self.inner();
        let inner = &mut *guard;
        let Stage::Ready(built) = &mut inner.phase else {
            unreachable!("restored just now")
        };
        let net = built.net.as_ref().expect("a built network");
        let here = spikes_record::synapse_rows(&built.rt, net);
        let rows = match &cluster {
            Some(cluster) => [here, cluster.synapses()?].concat(),
            None => here,
        };
        let count = rows
            .iter()
            .filter(|r| r.pre != tempo_runtime::ActorId::WORLD.raw())
            .count() as u64;
        let listed = if count <= crate::network::SYNAPSE_LIST_LIMIT {
            rows
        } else {
            Vec::new()
        };
        let mut info = NetworkInfo::new(net, &inner.description, &listed);
        info.synapse_count = count;
        let network = Arc::new(info);
        inner.network = Arc::clone(&network);
        self.shared.hub.connect(&built.rt, Arc::clone(&network));
        let now = built.rt.now();
        drop(guard);
        self.shared.hub.advance(now);
        self.notify(SessionNotice::NetworkChanged);
        Ok(network)
    }

    pub(crate) fn replay_of(id: SessionId, data: crate::replay::ReplayData) -> Self {
        let description = spikes_dna::parse(&data.description).unwrap_or_default();
        let session = Self::described(
            id,
            format!("replay: {}", data.name),
            data.description.clone(),
            description,
        );
        session.inner().replay = Some(Arc::new(data));
        session
    }

    /// Whether this session replays a recording.
    pub fn is_replay(&self) -> bool {
        self.inner().replay.is_some()
    }

    /// For a replay: the recording's last event time.
    pub fn replay_end(&self) -> Option<SimTime> {
        self.inner().replay.as_ref().map(|r| r.end)
    }

    fn build_replay(
        &self,
        inner: &mut MutexGuard<'_, Inner>,
        data: &Arc<crate::replay::ReplayData>,
        settings: BuildSettings,
        from: SimTime,
    ) -> Arc<NetworkInfo> {
        let (mut rt, info) = data.runtime(settings.seed, from);
        let network = Arc::new(info);
        self.shared.hub.connect(&rt, Arc::clone(&network));
        self.shared.hub.advance(rt.now());
        for i in self.shared.hub.membrane_neurons() {
            if let Some(id) = network.id(i) {
                rt.observe(id, true);
            }
        }
        inner.network = Arc::clone(&network);
        inner.settings = settings;
        inner.built_on = (Vec::new(), Vec::new());
        inner.homes = None;
        inner.wiring = None;
        inner.release_site = None;
        inner.phase = Stage::Ready(Box::new(Built { rt, net: None }));
        self.notify(SessionNotice::StateChanged(State::Ready));
        network
    }

    /// For a replay: jumps to simulated time `to` (not while running).
    pub fn seek(&self, to: SimTime) -> Result<(), SessionError> {
        let mut inner = self.inner();
        expect_not_running(&inner)?;
        let Some(data) = inner.replay.clone() else {
            return Err(SessionError::Invalid("only a replay can seek".into()));
        };
        let settings = inner.settings;
        self.build_replay(&mut inner, &data, settings, to);
        Ok(())
    }

    fn not_replay(&self, action: &str) -> Result<(), SessionError> {
        if self.is_replay() {
            Err(SessionError::Invalid(format!(
                "a replay plays back what was recorded: it can't {action}"
            )))
        } else {
            Ok(())
        }
    }

    /// Adds a connection from neuron `pre` to neuron `post` (indices), now. Its delay is the sending neuron's
    /// synaptic delay plus conduction time; its sign defaults to the sending neuron's; `learning` names one of the
    /// description's learning rules (`None`: no learning). Works while running.
    pub fn connect(
        &self,
        pre: u32,
        post: u32,
        weight: f32,
        inhibitory: Option<bool>,
        learning: Option<&str>,
    ) -> Result<(), SessionError> {
        self.not_replay("change its connections")?;
        let (network, wiring) = self.wiring()?;
        self.neuron_id(&network, pre)?;
        self.neuron_id(&network, post)?;
        let new = wiring
            .connection(pre as usize, post as usize, learning)
            .ok_or_else(|| SessionError::NotFound(format!("learning rule '{}'", learning.unwrap_or("flat"))))?;
        let sign = sign_of(inhibitory, new.sign);
        let last = self.sim_time();
        // The synapse lives with the receiving neuron, the route with the sending one: each on its own node.
        let cluster = self.coordinator();
        let index = match cluster.as_ref().and_then(|c| c.owner(post)) {
            Some(node) => cluster.as_ref().expect("owner implies cluster").add_synapse(
                node,
                NewSynapse {
                    pre,
                    post,
                    weight,
                    inhibitory,
                    learning,
                    last,
                },
            )?,
            None => self.add_synapse_here(pre, post, weight, inhibitory, learning, last)?,
        };
        let change = RouteChange::Connect {
            post,
            delay_ns: new.delay.as_nanos(),
            edge: index,
        };
        match cluster.as_ref().and_then(|c| c.owner(pre)) {
            Some(node) => {
                let cluster = cluster.as_ref().expect("owner implies cluster");
                cluster.change_route(node, pre, change)?;
                cluster.set_out(node, pre)?;
            }
            None => {
                self.change_route_here(pre, change)?;
                self.set_out_here(pre)?;
            }
        }
        let mut updated = (*network).clone();
        let listed = updated.lists_synapses();
        updated.synapse_count += 1;
        if listed {
            updated.synapses.push(crate::SynapseInfo {
                post,
                synapse: index,
                pre,
                weight,
                inhibitory: sign == spikes_model::Sign::Inhibitory,
            });
        }
        self.inner().network = Arc::new(updated);
        self.notify(SessionNotice::NetworkChanged);
        Ok(())
    }

    /// Removes the connection from `pre` to `post` (indices). Works while running; spikes already on their way
    /// arrive at a synapse of weight zero.
    pub fn prune(&self, pre: u32, post: u32) -> Result<(), SessionError> {
        let (network, _) = self.wiring()?;
        self.neuron_id(&network, pre)?;
        self.neuron_id(&network, post)?;
        let cluster = self.coordinator();
        let index = match cluster.as_ref().and_then(|c| c.owner(post)) {
            Some(node) => cluster
                .as_ref()
                .expect("owner implies cluster")
                .prune_synapse(node, pre, post)?,
            None => self.prune_synapse_here(pre, post)?,
        }
        .ok_or_else(|| SessionError::NotFound(format!("a connection from neuron {pre} to neuron {post}")))?;
        let change = RouteChange::Prune { post, edge: index };
        match cluster.as_ref().and_then(|c| c.owner(pre)) {
            Some(node) => cluster
                .as_ref()
                .expect("owner implies cluster")
                .change_route(node, pre, change)?,
            None => self.change_route_here(pre, change)?,
        }
        let mut updated = (*network).clone();
        updated.synapses.retain(|s| !(s.post == post && s.synapse == index));
        updated.synapse_count = updated.synapse_count.saturating_sub(1);
        self.inner().network = Arc::new(updated);
        self.notify(SessionNotice::NetworkChanged);
        Ok(())
    }

    /// Adds a synapse from `pre` on neuron `post`, which lives here; returns its index there.
    pub(crate) fn add_synapse_here(
        &self,
        pre: u32,
        post: u32,
        weight: f32,
        inhibitory: Option<bool>,
        learning: Option<&str>,
        last: SimTime,
    ) -> Result<u32, SessionError> {
        let (network, wiring) = self.wiring()?;
        let (pre_id, post_id) = (self.neuron_id(&network, pre)?, self.neuron_id(&network, post)?);
        let new = wiring
            .connection(pre as usize, post as usize, learning)
            .ok_or_else(|| SessionError::NotFound(format!("learning rule '{}'", learning.unwrap_or("flat"))))?;
        let sign = sign_of(inhibitory, new.sign);
        let mut synapse = spikes_model::Synapse::new(f64::from(weight), f64::from(weight), sign, new.params);
        synapse.last = last;
        self.on_neuron(post_id, move |actor| actor.add_synapse(pre_id, synapse))
    }

    /// Prunes the synapse from `pre` on neuron `post`, which lives here; returns its index there.
    pub(crate) fn prune_synapse_here(&self, pre: u32, post: u32) -> Result<Option<u32>, SessionError> {
        let (network, _) = self.wiring()?;
        let (pre_id, post_id) = (self.neuron_id(&network, pre)?, self.neuron_id(&network, post)?);
        self.on_neuron(post_id, move |actor| actor.prune_synapse(pre_id))
    }

    /// Changes the route of neuron `pre`, which lives here.
    pub(crate) fn change_route_here(&self, pre: u32, change: RouteChange) -> Result<(), SessionError> {
        let (network, wiring) = self.wiring()?;
        let route = *wiring
            .routes
            .get(pre as usize)
            .ok_or_else(|| SessionError::NotFound(format!("neuron {pre}")))?;
        let op = match change {
            RouteChange::Connect { post, delay_ns, edge } => {
                tempo_runtime::TopologyOp::Connect(tempo_runtime::RouteEdge {
                    target: self.neuron_id(&network, post)?,
                    delay: SimDuration::from_nanos(delay_ns),
                    edge: tempo_runtime::EdgeIdx(edge),
                })
            }
            RouteChange::Prune { post, edge } => tempo_runtime::TopologyOp::Prune {
                target: self.neuron_id(&network, post)?,
                edge: tempo_runtime::EdgeIdx(edge),
            },
        };
        self.change_route(route, op)
    }

    /// On a node of a cluster, once built: how many synapses this node made (the ones onto its neurons), and the
    /// routes owned on other nodes that reach its neurons, by owner node, each with its shortest edge (ns).
    pub(crate) fn reach_here(&self) -> Result<(u64, Vec<(u32, RouteReach)>), SessionError> {
        let inner = self.inner();
        match &inner.phase {
            Stage::Ready(built) => Ok((
                built.net.as_ref().map_or(0, |n| n.synapse_count),
                built
                    .rt
                    .reached_from_elsewhere()
                    .into_iter()
                    .map(|(node, routes)| (node, routes.into_iter().map(|(r, d)| (r.0, d.as_nanos())).collect()))
                    .collect(),
            )),
            other => Err(SessionError::State {
                state: state_of_phase(other),
                action: "exchange the routes of",
            }),
        }
    }

    /// On a node of a cluster: node `from` holds edges of these routes (owned here).
    pub(crate) fn add_route_nodes_here(&self, from: u32, routes: &[(u32, u64)]) -> Result<(), SessionError> {
        let mut inner = self.inner();
        match &mut inner.phase {
            Stage::Ready(built) => {
                built.rt.add_route_nodes(from, &route_reach(routes));
                Ok(())
            }
            other => Err(SessionError::State {
                state: state_of_phase(other),
                action: "exchange the routes of",
            }),
        }
    }

    /// The synapses of the neurons stored here (on a node of a cluster: its own).
    pub(crate) fn synapses_here(&self) -> Result<Vec<spikes_record::SynapseRow>, SessionError> {
        let inner = self.inner();
        match &inner.phase {
            Stage::Ready(built) => Ok(built
                .net
                .as_ref()
                .map(|net| spikes_record::synapse_rows(&built.rt, net))
                .unwrap_or_default()),
            other => Err(SessionError::State {
                state: state_of_phase(other),
                action: "list the synapses of",
            }),
        }
    }

    /// Neuron `pre` (here) sends along its route: it may have had no outgoing connections, and so not have been
    /// using it.
    pub(crate) fn set_out_here(&self, pre: u32) -> Result<(), SessionError> {
        let (network, wiring) = self.wiring()?;
        let route = wiring.routes[pre as usize];
        let id = self.neuron_id(&network, pre)?;
        self.on_neuron(id, move |actor| actor.out = Some(route))
    }

    /// Waits for a control-plane reply. If the run ends meanwhile, the command is still waiting in the runtime's
    /// inbox: apply it on the idle runtime, so it's answered now (and exactly once).
    fn await_reply<T>(&self, reply: tempo_runtime::Reply<T>) -> Result<T, SessionError> {
        let deadline = std::time::Instant::now() + QUERY_TIMEOUT;
        loop {
            match reply.wait_timeout(Duration::from_millis(10)) {
                Some(Ok(value)) => return Ok(value),
                Some(Err(_)) => {
                    return Err(SessionError::Invalid(
                        "the run ended before the request was handled".into(),
                    ));
                }
                None => {}
            }
            {
                let mut inner = self.inner();
                match &mut inner.phase {
                    Stage::Ready(built) => {
                        built.rt.apply_control();
                    }
                    Stage::Running { .. } => {}
                    other => {
                        return Err(SessionError::State {
                            state: state_of_phase(other),
                            action: "send requests to",
                        });
                    }
                }
            }
            if std::time::Instant::now() >= deadline {
                return Err(SessionError::Timeout);
            }
        }
    }

    fn wiring(&self) -> Result<(Arc<NetworkInfo>, Arc<Network>), SessionError> {
        let inner = self.inner();
        match &inner.wiring {
            Some(wiring) => Ok((Arc::clone(&inner.network), Arc::clone(wiring))),
            None => Err(SessionError::State {
                state: state_of(&inner),
                action: "change the connections of",
            }),
        }
    }

    fn neuron_id(&self, network: &NetworkInfo, index: u32) -> Result<tempo_runtime::ActorId, SessionError> {
        network
            .id(index)
            .ok_or_else(|| SessionError::NotFound(format!("neuron {index}")))
    }

    /// Runs `f` on a neuron: directly when ready, through the control plane while running.
    fn on_neuron<R: Send + 'static>(
        &self,
        id: tempo_runtime::ActorId,
        f: impl FnOnce(&mut NeuronActor) -> R + Send + 'static,
    ) -> Result<R, SessionError> {
        let mut inner = self.inner();
        let control = match &mut inner.phase {
            Stage::Ready(built) => {
                let actor = built
                    .rt
                    .actor_mut::<NeuronActor>(id)
                    .ok_or_else(|| SessionError::NotFound(format!("actor {id}")))?;
                return Ok(f(actor));
            }
            Stage::Running { control, .. } => control.clone(),
            other => {
                return Err(SessionError::State {
                    state: state_of_phase(other),
                    action: "change the connections of",
                });
            }
        };
        drop(inner); // the shards apply it at their next window
        let (tx, rx) = std::sync::mpsc::channel();
        let reply = control.update::<NeuronActor>(id, move |actor| {
            let _ = tx.send(f(actor));
        });
        if self.await_reply(reply)? {
            rx.recv().map_err(|_| SessionError::Timeout)
        } else {
            Err(SessionError::NotFound(format!("actor {id}")))
        }
    }

    fn change_route(&self, route: tempo_runtime::RouteId, op: tempo_runtime::TopologyOp) -> Result<(), SessionError> {
        let mut inner = self.inner();
        let control = match &mut inner.phase {
            Stage::Ready(built) => {
                built.rt.change_route(route, op);
                return Ok(());
            }
            Stage::Running { control, .. } => control.clone(),
            other => {
                return Err(SessionError::State {
                    state: state_of_phase(other),
                    action: "change the connections of",
                });
            }
        };
        drop(inner);
        self.await_reply(control.change_route(route, op)).map(|_| ())
    }

    /// Connects an external sensor. The network must be built.
    pub fn connect_sensor(&self, spec: SensorSpec) -> Result<Sensor, SessionError> {
        self.not_replay("take sensor input")?;
        let network = self.network();
        if network.neurons.is_empty() {
            return Err(SessionError::State {
                state: self.state(),
                action: "connect a sensor to",
            });
        }
        if spec.targets.is_empty() {
            return Err(SessionError::Invalid(format!("sensor '{}' has no channels", spec.name)));
        }
        if let Some(&bad) = spec.targets.iter().flatten().find(|&&n| network.id(n).is_none()) {
            return Err(SessionError::NotFound(format!("neuron {bad} (sensor '{}')", spec.name)));
        }
        let id = self
            .shared
            .next_sensor
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let seed = self.inner().settings.seed;
        Ok(Sensor::new(id, spec, self.clone(), seed))
    }

    /// The sensors connected to this session (through any connection), with their counters.
    pub fn sensors(&self) -> Vec<crate::sensors::SensorInfo> {
        lock(&self.shared.sensors).iter().filter_map(|e| e.info()).collect()
    }

    pub(crate) fn next_sensor_key(&self) -> u64 {
        self.shared
            .next_sensor_key
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    pub(crate) fn register_sensor(
        &self,
        key: u64,
        spec: Arc<SensorSpec>,
        inner: std::sync::Weak<crate::sensors::Inner>,
    ) {
        lock(&self.shared.sensors).push(crate::sensors::SensorEntry::new(key, spec, inner));
        self.notify(SessionNotice::SensorsChanged);
    }

    /// Another handle to the connected sensor called `name` (the newest, if several have it): how a program claims
    /// a sensor that was declared elsewhere, such as by the UI for a source of an input file. The sensor stays
    /// connected while any handle lives.
    pub fn claim_sensor(&self, name: &str) -> Result<Sensor, SessionError> {
        self.not_replay("take sensor input")?;
        lock(&self.shared.sensors)
            .iter()
            .rev()
            .filter(|e| e.spec.name == name)
            .find_map(|e| e.claim())
            .ok_or_else(|| SessionError::NotFound(format!("sensor '{name}'")))
    }

    pub(crate) fn unregister_sensor(&self, key: u64) {
        lock(&self.shared.sensors).retain(|e| e.key != key);
        self.notify(SessionNotice::SensorsChanged);
    }

    /// Where "now" is for a sensor, and how fast simulation time runs. Not running counts as paused.
    pub(crate) fn sensor_moment(&self) -> Result<Moment, SessionError> {
        let inner = self.inner();
        let speed = match inner.speed {
            Speed::Factor(f) => f,
            Speed::AsFastAsPossible => 1.0,
        };
        match &inner.phase {
            Stage::Ready(built) => Ok(Moment {
                now: built.rt.now(),
                paused: true,
                speed,
            }),
            Stage::Running { clock, .. } => Ok(Moment {
                now: clock.horizon().unwrap_or_else(|| clock.sim_time()),
                paused: clock.is_paused(),
                speed,
            }),
            other => Err(SessionError::State {
                state: state_of_phase(other),
                action: "send sensor data to",
            }),
        }
    }

    /// Injects a sensor's inputs `(neuron, time, mV)` and records them (and its samples
    /// `(channel, sensor time, time, value)`) if the run is recorded.
    pub(crate) fn sensor_input(
        &self,
        sensor: u32,
        inputs: &[(u32, SimTime, f32)],
        samples: &[(u32, u64, SimTime, f32)],
    ) -> Result<(), SessionError> {
        let network = self.network();
        let resolved: Vec<(tempo_runtime::ActorId, SimTime, f32)> = inputs
            .iter()
            .filter_map(|&(n, at, mv)| network.id(n).map(|id| (id, at, mv)))
            .collect();
        if let Some(log) = lock(&self.shared.sensor_log).as_mut() {
            let mut record = || -> spikes_record::Result<()> {
                for &(channel, t_sensor, t, v) in samples {
                    log.sample(sensor, channel, t_sensor, t, v)?;
                }
                for &(id, t, mv) in &resolved {
                    log.input(sensor, t, id.raw(), mv)?;
                }
                Ok(())
            };
            if let Err(e) = record() {
                self.notify(SessionNotice::RecordingFailed(format!("sensor data: {e}")));
            }
        }
        self.inject_at(resolved)
    }

    fn inject_at(&self, inputs: Vec<(tempo_runtime::ActorId, SimTime, f32)>) -> Result<(), SessionError> {
        let msg = |mv: f32| SimMsg::Neuron(NeuronMsg::External { mv });
        let mut inner = self.inner();
        match &mut inner.phase {
            Stage::Ready(built) => {
                let now = built.rt.now();
                for (id, at, mv) in inputs {
                    built.rt.inject(id, at.max(now), msg(mv));
                }
                Ok(())
            }
            Stage::Running { control, .. } => {
                drop(control.inject_batch(inputs.into_iter().map(|(id, at, mv)| (id, at, msg(mv))).collect()));
                Ok(())
            }
            other => Err(SessionError::State {
                state: state_of_phase(other),
                action: "send input to",
            }),
        }
    }

    /// Injects the sensor inputs recorded in run directory `dir`, at their recorded times (neurons are matched by
    /// name, so the network must be built from the same description). Rebuild and replay to reproduce a run.
    /// Returns how many inputs were injected.
    pub fn replay_sensor_inputs(&self, dir: &std::path::Path) -> Result<usize, SessionError> {
        let recorded = crate::sensors::recorded_inputs(dir)?;
        let network = self.network();
        let inputs = recorded
            .into_iter()
            .map(|(name, at, mv)| {
                let id = network.by_name(&name).and_then(|i| network.id(i));
                id.map(|id| (id, at, mv))
                    .ok_or_else(|| SessionError::NotFound(format!("neuron '{name}' from the recording")))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let n = inputs.len();
        self.inject_at(inputs)?;
        Ok(n)
    }

    /// Sends `mv` of external input to each of `neurons`, now.
    pub fn stimulate(&self, neurons: &[u32], mv: f32) -> Result<(), SessionError> {
        self.not_replay("take input")?;
        let msg = SimMsg::Neuron(NeuronMsg::External { mv });
        self.send_now(neurons, msg)
    }

    /// Releases `amount` of a modulator (by index) at the network's release site.
    pub fn release_modulator(&self, modulator: u16, amount: f32) -> Result<(), SessionError> {
        self.not_replay("release modulators")?;
        let site = self.inner().release_site;
        let Some(site) = site else {
            return Err(SessionError::Invalid("the network defines no modulators (MOD)".into()));
        };
        if usize::from(modulator) >= self.network().modulators.len() {
            return Err(SessionError::NotFound(format!("modulator {modulator}")));
        }
        let msg = SimMsg::Modulator(ModulatorMsg::Release {
            kind: modulator,
            amount,
        });
        self.send_to(&[site.id()], msg)
    }

    fn send_now(&self, neurons: &[u32], msg: SimMsg) -> Result<(), SessionError> {
        let network = self.network();
        let ids = neurons
            .iter()
            .map(|&i| {
                network
                    .id(i)
                    .ok_or_else(|| SessionError::NotFound(format!("neuron {i}")))
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.send_to(&ids, msg)
    }

    fn send_to(&self, ids: &[tempo_runtime::ActorId], msg: SimMsg) -> Result<(), SessionError> {
        let mut inner = self.inner();
        match &mut inner.phase {
            Stage::Ready(built) => {
                let now = built.rt.now();
                for &id in ids {
                    built.rt.inject(id, now, msg);
                }
                Ok(())
            }
            Stage::Running { control, .. } => {
                for &id in ids {
                    drop(control.inject_live(id, msg));
                }
                Ok(())
            }
            other => Err(SessionError::State {
                state: state_of_phase(other),
                action: "send input to",
            }),
        }
    }

    /// Turns membrane recording (every update) on or off for these neurons.
    pub fn observe(&self, neurons: &[u32], on: bool) -> Result<(), SessionError> {
        let network = self.network();
        let mut neurons = neurons.to_vec();
        if let Some(cluster) = self.coordinator() {
            let mut remote: std::collections::BTreeMap<usize, Vec<u32>> = Default::default();
            neurons.retain(|&i| match cluster.owner(i) {
                Some(node) => {
                    remote.entry(node).or_default().push(i);
                    false
                }
                None => true,
            });
            for (node, list) in remote {
                cluster.observe(node, list, on)?;
            }
        }
        let mut inner = self.inner();
        for &i in &neurons {
            let id = network
                .id(i)
                .ok_or_else(|| SessionError::NotFound(format!("neuron {i}")))?;
            match &mut inner.phase {
                Stage::Ready(built) => {
                    built.rt.observe(id, on);
                }
                Stage::Running { control, .. } => drop(control.observe(id, on)),
                other => {
                    return Err(SessionError::State {
                        state: state_of_phase(other),
                        action: "observe neurons in",
                    });
                }
            }
        }
        Ok(())
    }

    /// A neuron's current state.
    pub fn query_neuron(&self, neuron: u32) -> Result<NeuronSnapshot, SessionError> {
        self.not_replay("be queried (its neurons were recorded, not simulated)")?;
        if let Some(cluster) = self.coordinator()
            && let Some(node) = cluster.owner(neuron)
        {
            return cluster.query(node, neuron);
        }
        let network = self.network();
        let id = network
            .id(neuron)
            .ok_or_else(|| SessionError::NotFound(format!("neuron {neuron}")))?;
        let inner = self.inner();
        let control = match &inner.phase {
            Stage::Ready(built) => {
                let actor = built
                    .rt
                    .actor::<NeuronActor>(id)
                    .ok_or_else(|| SessionError::NotFound(format!("neuron {neuron}")))?;
                return Ok(snapshot(actor, &network));
            }
            Stage::Running { control, .. } => control.clone(),
            other => {
                return Err(SessionError::State {
                    state: state_of_phase(other),
                    action: "query",
                });
            }
        };
        drop(inner); // don't hold the session while the shards answer
        let reply = control.query::<NeuronActor, _>(id, {
            let network = Arc::clone(&network);
            move |actor| snapshot(actor, &network)
        });
        let deadline = std::time::Instant::now() + QUERY_TIMEOUT;
        loop {
            match reply.wait_timeout(Duration::from_millis(10)) {
                Some(Ok(Some(snapshot))) => return Ok(snapshot),
                Some(Ok(None)) => return Err(SessionError::NotFound(format!("neuron {neuron}"))),
                Some(Err(_)) | None => {}
            }
            // A query that arrives as the run ends waits for the next run; if the run is over, read the neuron
            // directly instead. (The stale query runs harmlessly at the next start.)
            let inner = self.inner();
            if let Stage::Ready(built) = &inner.phase {
                let actor = built
                    .rt
                    .actor::<NeuronActor>(id)
                    .ok_or_else(|| SessionError::NotFound(format!("neuron {neuron}")))?;
                return Ok(snapshot(actor, &network));
            }
            if !matches!(inner.phase, Stage::Running { .. }) {
                return Err(SessionError::State {
                    state: state_of(&inner),
                    action: "query",
                });
            }
            if std::time::Instant::now() >= deadline {
                return Err(SessionError::Timeout);
            }
        }
    }
}

/// Each neuron's node and shard: remembered, or read from the built runtime (`None` if neither).
fn homes(inner: &mut Inner) -> Option<Arc<Vec<(u32, u16)>>> {
    if inner.homes.is_none()
        && let Stage::Ready(built) = &inner.phase
    {
        let network = &inner.network;
        let homes = (0..network.neurons.len() as u32)
            .map(|i| network.id(i).and_then(|id| built.rt.home_of(id)).unwrap_or((0, 0)))
            .collect();
        inner.homes = Some(Arc::new(homes));
    }
    inner.homes.clone()
}

fn sign_of(inhibitory: Option<bool>, default: spikes_model::Sign) -> spikes_model::Sign {
    match inhibitory {
        Some(true) => spikes_model::Sign::Inhibitory,
        Some(false) => spikes_model::Sign::Excitatory,
        None => default,
    }
}

fn snapshot(actor: &NeuronActor, network: &NetworkInfo) -> NeuronSnapshot {
    let st = &actor.state;
    NeuronSnapshot {
        membrane_mv: st.u,
        intrinsic_mv: st.b,
        rate_hz: st.rate_hz,
        last_event: st.last_event,
        last_spike: st.last_fire,
        tonic: st.phase == Phase::Tonic,
        spikes: st.spikes,
        synapses: actor
            .synapses
            .iter()
            .zip(&actor.pre)
            .map(|(s, pre)| (network.index(*pre).unwrap_or(u32::MAX), s.weight, s.eligibility))
            .collect(),
    }
}

fn state_of_phase(phase: &Stage) -> State {
    match phase {
        Stage::Loaded => State::Loaded,
        Stage::Ready(_) => State::Ready,
        Stage::Running { clock, .. } if clock.is_paused() => State::Paused,
        Stage::Running { .. } => State::Running,
        Stage::Failed(_) => State::Failed,
    }
}

/// A saved run (see [`Session::save`]): what rebuilds the network, and the runtime's state.
#[derive(serde::Serialize, serde::Deserialize)]
struct SavedRun {
    description: String,
    seed: u64,
    min_delay: SimDuration,
    jitter: Jitter,
    record_weights: bool,
    inputs: Vec<(InputSpec, SimTime)>,
    snapshot: tempo_runtime::Snapshot,
}

/// A saved run's first bytes (with the format's version).
const SAVED_MAGIC: &[u8; 8] = b"SPKSAVE1";

/// Spawns a synthetic input that starts at `start`.
fn spawn_input(rt: &mut Runtime, network: &NetworkInfo, input: InputSpec, start: SimTime) -> Result<(), SessionError> {
    input.validate(network)?;
    let resolve =
        |list: &[u32]| -> Vec<NeuronRef> { list.iter().filter_map(|&i| network.id(i).map(NeuronRef::new)).collect() };
    let targets = resolve(&input.targets());
    let delay = spikes_network::INPUT_DELAY;
    match input {
        InputSpec::Poisson {
            rate_hz, mv, duration, ..
        } => {
            if !(rate_hz.is_finite() && rate_hz > 0.0) {
                return Err(SessionError::Invalid(format!("rate must be positive, not {rate_hz}")));
            }
            rt.spawn(PoissonInput {
                rate_hz,
                targets,
                mv,
                delay,
                start,
                until: duration.map_or(SimTime::MAX, |d| start.saturating_add(d)),
            });
        }
        InputSpec::Periodic {
            period, mv, duration, ..
        } => {
            if period.is_zero() {
                return Err(SessionError::Invalid("the period must be positive".into()));
            }
            rt.spawn(PeriodicInput {
                period,
                targets,
                mv,
                delay,
                start,
                until: duration.map_or(SimTime::MAX, |d| start.saturating_add(d)),
            });
        }
        InputSpec::Pulses { times, mv, .. } => {
            let mut times: Vec<SimTime> = times.iter().map(|&t| start.saturating_add(t)).collect();
            times.sort_unstable();
            rt.spawn(PulseInput {
                targets,
                times,
                mv,
                delay,
            });
        }
        InputSpec::Signal {
            channels,
            formula,
            sample,
            encoding,
            key,
            duration,
            data,
        } => {
            let formula = spikes_signal::Formula::compile(&formula)
                .map_err(|e| SessionError::Invalid(format!("formula: {e}")))?;
            let data = spikes_signal::DataSets::load(&data).map_err(|e| SessionError::Invalid(e.to_string()))?;
            rt.spawn(SignalInput {
                channels: channels.iter().map(|c| resolve(c)).collect(),
                formula,
                encoding,
                sample,
                key,
                delay,
                start,
                until: duration.map_or(SimTime::MAX, |d| start.saturating_add(d)),
                data,
                encoders: Vec::new(),
                next: 0,
            });
        }
    }
    Ok(())
}

fn state_of(inner: &Inner) -> State {
    state_of_phase(&inner.phase)
}

fn expect_not_running(inner: &Inner) -> Result<(), SessionError> {
    match inner.phase {
        Stage::Running { .. } => Err(SessionError::State {
            state: state_of(inner),
            action: "change the network of",
        }),
        _ => Ok(()),
    }
}

/// Routes with their shortest edges, as sent between nodes: (route id, ns).
pub(crate) type RouteReach = Vec<(u32, u64)>;

/// Routes and their shortest edges as sent between nodes (route ids, ns).
pub(crate) fn route_reach(routes: &[(u32, u64)]) -> Vec<(tempo_runtime::RouteId, SimDuration)> {
    routes
        .iter()
        .map(|&(r, d)| (tempo_runtime::RouteId(r), SimDuration::from_nanos(d)))
        .collect()
}
