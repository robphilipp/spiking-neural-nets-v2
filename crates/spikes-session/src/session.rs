//! One network: its description, the built runtime, and the run thread.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;

use spikes_dna::NetworkDescription;
use spikes_model::Phase;
use spikes_network::{
    BuildOptions, Event, ModulatorMsg, ModulatorRef, Network, NeuronActor, NeuronMsg, Runtime, SimMsg,
};
use spikes_record::{RecordOptions, Recorder, RunInfo, Summary};
use tempo_runtime::{ClockHandle, Control, Jitter, RuntimeConfig, SimDuration, SimTime};

use crate::network::NetworkInfo;
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
    fn apply(self, clock: &ClockHandle) {
        match self {
            Self::AsFastAsPossible => clock.set_as_fast_as_possible(),
            Self::Factor(f) => clock.set_time_factor(1.0 / f),
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

/// How to build the network.
#[derive(Copy, Clone, Debug)]
pub struct BuildSettings {
    /// Seed for the runtime and for random initial weights.
    pub seed: u64,
    /// Worker threads.
    pub shards: usize,
    /// The shortest connection delay (see `BuildOptions::min_delay`).
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
            min_delay: options.min_delay,
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

/// Something that happened to a session.
#[derive(Clone, Debug, PartialEq)]
pub enum SessionNotice {
    /// The state changed.
    StateChanged(State),
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
    release_site: Option<ModulatorRef>,
}

struct Shared {
    id: SessionId,
    name: String,
    inner: Mutex<Inner>,
    /// Signalled when a run ends.
    changed: Condvar,
    listeners: Mutex<Vec<Sender<SessionNotice>>>,
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
        Ok(Self {
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
                    release_site: None,
                }),
                changed: Condvar::new(),
                listeners: Mutex::new(Vec::new()),
            }),
        })
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

    /// The current simulation time.
    pub fn sim_time(&self) -> SimTime {
        match &self.inner().phase {
            Stage::Ready(built) => built.rt.now(),
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
        inner.release_site =
            (!network.modulators.is_empty()).then(|| net.add_release_site(&mut rt, network.centroid()));
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
                match Recorder::start(dir, &rt, &net, &inner.description, info, RecordOptions::default()) {
                    Ok(recorder) => Some(recorder),
                    Err(e) => {
                        inner.phase = Stage::Ready(Box::new(Built { rt, net }));
                        return Err(SessionError::Record(e.to_string()));
                    }
                }
            }
            None => None,
        };
        let clock = rt.clock();
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
            .spawn(move || session.run_thread(rt, net, recorder, end))
            .map_err(|e| SessionError::Invalid(format!("couldn't start the run thread: {e}")))?;
        self.notify(SessionNotice::StateChanged(State::Running));
        Ok(())
    }

    fn run_thread(self, mut rt: Runtime, net: Network, recorder: Option<Recorder>, end: SimTime) {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| rt.run_until(end)));
        let recording = recorder.map(Recorder::finish);
        let mut inner = self.inner();
        let notices = match outcome {
            Ok(_) => {
                let at = rt.now();
                inner.phase = Stage::Ready(Box::new(Built { rt, net }));
                let mut notices = Vec::new();
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
        drop(inner);
        self.shared.changed.notify_all();
        for notice in notices {
            self.notify(notice);
        }
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
                let end = built.rt.now().saturating_add(by);
                built.rt.run_until(end);
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
        let reply = control.query::<NeuronActor, _>(id, move |actor| snapshot(actor, &network));
        match reply.wait_timeout(QUERY_TIMEOUT) {
            Some(Ok(Some(snapshot))) => Ok(snapshot),
            Some(Ok(None)) => Err(SessionError::NotFound(format!("neuron {neuron}"))),
            Some(Err(_)) => Err(SessionError::Invalid(
                "the run ended before the query was answered".into(),
            )),
            None => Err(SessionError::Timeout),
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
