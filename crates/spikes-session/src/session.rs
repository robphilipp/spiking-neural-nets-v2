//! One network: its description, the built runtime, and the run thread.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;

use spikes_dna::NetworkDescription;
use spikes_model::Phase;
use spikes_network::{
    BuildOptions, Event, ModulatorMsg, ModulatorRef, Network, NeuronActor, NeuronMsg, NeuronRef, PeriodicInput,
    PoissonInput, Runtime, SimMsg,
};
use spikes_record::{RecordOptions, Recorder, RunInfo, SensorLog, Summary};
use tempo_runtime::{ClockHandle, Control, Jitter, RuntimeConfig, SimDuration, SimTime};

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
#[derive(Clone, Debug, PartialEq)]
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
}

impl InputSpec {
    fn targets(&self) -> &[u32] {
        match self {
            Self::Poisson { targets, .. } | Self::Periodic { targets, .. } => targets,
        }
    }
}

/// Something that happened to a session.
#[derive(Clone, Debug, PartialEq)]
pub enum SessionNotice {
    /// The state changed.
    StateChanged(State),
    /// Connections were added or pruned: fetch the network again.
    NetworkChanged,
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
#[derive(Clone, Debug, PartialEq)]
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
    net: Network,
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
    /// The built network's wiring (routes, delays, synapse parameter sets), for connections made later.
    wiring: Option<Arc<Network>>,
    release_site: Option<ModulatorRef>,
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
                    release_site: None,
                }),
                changed: Condvar::new(),
                listeners: Mutex::new(Vec::new()),
                hub: Hub::start(id.0),
                sensor_log: Mutex::new(None),
                next_sensor: std::sync::atomic::AtomicU32::new(0),
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
        let mut rt = Runtime::new(RuntimeConfig {
            seed: settings.seed,
            shards: settings.shards,
            min_remote_delay: Some(settings.min_delay),
            ..RuntimeConfig::default()
        });
        let options = BuildOptions {
            seed: settings.seed,
            min_delay: settings.min_delay,
            jitter: settings.jitter,
            record_weights: settings.record_weights,
        };
        let net = spikes_network::build(&mut rt, &inner.description, &options);
        let network = Arc::new(NetworkInfo::new(&rt, &net, &inner.description));
        inner.wiring = Some(Arc::new(net.clone()));
        inner.release_site =
            (!network.modulators.is_empty()).then(|| net.add_release_site(&mut rt, network.centroid()));
        self.shared.hub.connect(&rt, Arc::clone(&network));
        for i in self.shared.hub.membrane_neurons() {
            if let Some(id) = network.id(i) {
                rt.observe(id, true);
            }
        }
        inner.network = Arc::clone(&network);
        inner.settings = settings;
        inner.phase = Stage::Ready(Box::new(Built { rt, net }));
        drop(inner);
        self.notify(SessionNotice::StateChanged(State::Ready));
        Ok(network)
    }

    /// Starts (or continues) a run on a thread of its own.
    pub fn start(&self, run: RunSettings) -> Result<(), SessionError> {
        let speed = run.speed.check()?;
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
        let recorder = match &run.record {
            Some(dir) => {
                let info = RunInfo {
                    seed: inner.settings.seed,
                    description: &inner.text,
                };
                let started = Recorder::start(dir, &rt, &net, &inner.description, info, RecordOptions::default())
                    .and_then(|recorder| Ok((recorder, SensorLog::create(dir, RecordOptions::default().batch_rows)?)));
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
            None => None,
        };
        let clock = rt.clock();
        // Taken before the run thread exists, so a stop() right after start() returns can't be missed.
        let stop_token = clock.stop_token();
        speed.apply(&clock);
        inner.speed = speed;
        let end = run.duration.map_or(SimTime::MAX, |d| rt.now().saturating_add(d));
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
        net: Network,
        recorder: Option<Recorder>,
        end: SimTime,
        stop_token: tempo_runtime::StopToken,
    ) {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            rt.run_until_unless_stopped(end, stop_token)
        }));
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

    /// Pauses a run.
    pub fn pause(&self) -> Result<(), SessionError> {
        self.clock()?.pause();
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
        self.notify(SessionNotice::StateChanged(State::Running));
        Ok(())
    }

    /// Advances by `by`: while paused, lets the run go on for that long and pause again; when ready, runs that
    /// long as fast as possible before returning.
    pub fn step(&self, by: SimDuration) -> Result<(), SessionError> {
        let mut inner = self.inner();
        match &mut inner.phase {
            Stage::Running { clock, .. } if clock.is_paused() => {
                clock.step(by);
                Ok(())
            }
            Stage::Ready(built) => {
                // As fast as possible, whatever state the last run left the clock in (a paused clock would never
                // get there). The next start sets the speed again.
                built.rt.clock().set_as_fast_as_possible();
                let end = built.rt.now().saturating_add(by);
                built.rt.run_until(end);
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
        let membrane = settings.membrane.clone();
        self.shared
            .hub
            .update(viewer.id(), settings, self.network().neurons.len());
        let _ = self.observe(&membrane, true);
    }

    /// Adds a synthetic input that runs inside the simulation. Only while ready (actors can't be added during a
    /// run); a rebuild removes it.
    pub fn add_input(&self, input: InputSpec) -> Result<(), SessionError> {
        let network = self.network();
        let targets = input
            .targets()
            .iter()
            .map(|&i| {
                network
                    .id(i)
                    .map(NeuronRef::new)
                    .ok_or_else(|| SessionError::NotFound(format!("neuron {i}")))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut inner = self.inner();
        let state = state_of(&inner);
        let Stage::Ready(built) = &mut inner.phase else {
            return Err(SessionError::State {
                state,
                action: "add an input to",
            });
        };
        let start = built.rt.now();
        let delay = SimDuration::from_millis(1);
        match input {
            InputSpec::Poisson {
                rate_hz, mv, duration, ..
            } => {
                if !(rate_hz.is_finite() && rate_hz > 0.0) {
                    return Err(SessionError::Invalid(format!("rate must be positive, not {rate_hz}")));
                }
                built.rt.spawn(PoissonInput {
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
                built.rt.spawn(PeriodicInput {
                    period,
                    targets,
                    mv,
                    delay,
                    start,
                    until: duration.map_or(SimTime::MAX, |d| start.saturating_add(d)),
                });
            }
        }
        Ok(())
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
        let (network, wiring) = self.wiring()?;
        let (pre_id, post_id) = (self.neuron_id(&network, pre)?, self.neuron_id(&network, post)?);
        let new = wiring
            .connection(pre as usize, post as usize, learning)
            .ok_or_else(|| SessionError::NotFound(format!("learning rule '{}'", learning.unwrap_or("flat"))))?;
        let sign = match inhibitory {
            Some(true) => spikes_model::Sign::Inhibitory,
            Some(false) => spikes_model::Sign::Excitatory,
            None => new.sign,
        };
        let route = wiring.routes[pre as usize];
        let mut synapse = spikes_model::Synapse::new(f64::from(weight), f64::from(weight), sign, new.params);
        synapse.last = self.sim_time();
        let edge = |index: u32| tempo_runtime::RouteEdge {
            target: post_id,
            delay: new.delay,
            edge: tempo_runtime::EdgeIdx(index),
        };
        let index = self.on_neuron(post_id, move |actor| actor.add_synapse(pre_id, synapse))?;
        self.change_route(route, tempo_runtime::TopologyOp::Connect(edge(index)))?;
        // The sender may have had no outgoing connections, and so not have been using its route.
        self.on_neuron(pre_id, move |actor| actor.out = Some(route))?;
        let mut updated = (*network).clone();
        updated.synapses.push(crate::SynapseInfo {
            post,
            synapse: index,
            pre,
            weight,
            inhibitory: sign == spikes_model::Sign::Inhibitory,
        });
        self.inner().network = Arc::new(updated);
        self.notify(SessionNotice::NetworkChanged);
        Ok(())
    }

    /// Removes the connection from `pre` to `post` (indices). Works while running; spikes already on their way
    /// arrive at a synapse of weight zero.
    pub fn prune(&self, pre: u32, post: u32) -> Result<(), SessionError> {
        let (network, wiring) = self.wiring()?;
        let (pre_id, post_id) = (self.neuron_id(&network, pre)?, self.neuron_id(&network, post)?);
        let index = self
            .on_neuron(post_id, move |actor| actor.prune_synapse(pre_id))?
            .ok_or_else(|| SessionError::NotFound(format!("a connection from neuron {pre} to neuron {post}")))?;
        let route = wiring.routes[pre as usize];
        self.change_route(
            route,
            tempo_runtime::TopologyOp::Prune {
                target: post_id,
                edge: tempo_runtime::EdgeIdx(index),
            },
        )?;
        let mut updated = (*network).clone();
        updated.synapses.retain(|s| !(s.post == post && s.synapse == index));
        self.inner().network = Arc::new(updated);
        self.notify(SessionNotice::NetworkChanged);
        Ok(())
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
        let msg = SimMsg::Neuron(NeuronMsg::External { mv });
        self.send_now(neurons, msg)
    }

    /// Releases `amount` of a modulator (by index) at the network's release site.
    pub fn release_modulator(&self, modulator: u16, amount: f32) -> Result<(), SessionError> {
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
        let mut inner = self.inner();
        for &i in neurons {
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
