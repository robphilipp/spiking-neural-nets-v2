//! The session engine: everything a running simulation needs, behind a thread-safe API that the CLI, the
//! WebSocket server and the Tauri app all use (app plan 01).
//!
//! An [`Engine`] holds [`Session`]s. A session is one network: its `.boo` description, the built runtime, a run
//! thread, the clock (speed, pause, step), recording, and live input. Sessions go
//! [`State::Loaded`] → [`State::Ready`] (built) → [`State::Running`] ⇄ [`State::Paused`] → back to
//! [`State::Ready`] when the run ends; a later run continues from where the last one stopped.
//!
//! Outside the engine, neurons are numbered densely in description order ([`NetworkInfo`]); actor ids stay
//! inside.

pub mod cluster;
mod connection;
mod network;
mod replay;
mod sensors;
mod session;
mod viewer;

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

pub use connection::{Connection, Outgoing, api_error};
pub use network::{GRID, NetworkInfo, NeuronInfo, SYNAPSE_LIST_LIMIT, SynapseInfo};
pub use sensors::{Encoding, OnPause, SampleBatch, Sensor, SensorSpec, SensorStats, SpikeBatch};
pub use session::{
    BuildSettings, DEFAULT_MIN_DELAY, InputSpec, NeuronSnapshot, RunSettings, Session, SessionNotice, Speed, State,
};
pub use viewer::{FrameStats, Selection, ViewSettings, Viewer};

/// Identifies a session within its engine.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SessionId(pub u32);

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Why a session operation failed.
#[derive(Debug)]
pub enum SessionError {
    /// The description doesn't parse.
    Parse(Vec<spikes_dna::ParseError>),
    /// Not allowed in the current state.
    State {
        /// The current state.
        state: State,
        /// What was attempted, as in "can't {action} a {state} session".
        action: &'static str,
    },
    /// No such session, neuron or modulator.
    NotFound(String),
    /// An argument is out of range.
    Invalid(String),
    /// Recording couldn't start.
    Record(String),
    /// A running simulation didn't answer in time.
    Timeout,
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(errors) => {
                write!(f, "the description has {} error(s)", errors.len())?;
                if let Some(first) = errors.first() {
                    write!(f, "; first: {first}")?;
                }
                Ok(())
            }
            Self::State { state, action } => write!(f, "can't {action} a session that is {state:?}"),
            Self::NotFound(what) => write!(f, "{what} not found"),
            Self::Invalid(why) => f.write_str(why),
            Self::Record(why) => write!(f, "recording failed: {why}"),
            Self::Timeout => f.write_str("the simulation didn't answer in time"),
        }
    }
}

impl std::error::Error for SessionError {}

/// The sessions of one process.
#[derive(Debug, Default)]
pub struct Engine {
    sessions: Mutex<BTreeMap<SessionId, Session>>,
    next: AtomicU32,
}

impl Engine {
    /// An engine with no sessions.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a session from a `.boo` description.
    pub fn create(&self, name: impl Into<String>, description: impl Into<String>) -> Result<Session, SessionError> {
        let id = SessionId(self.next.fetch_add(1, Ordering::Relaxed));
        let session = Session::new(id, name.into(), description.into())?;
        self.lock().insert(id, session.clone());
        Ok(session)
    }

    /// Creates a session from a description built in code (e.g. a large generated network). `text` stands in for
    /// the source in recordings.
    pub fn create_described(
        &self,
        name: impl Into<String>,
        text: impl Into<String>,
        description: spikes_dna::NetworkDescription,
    ) -> Session {
        let id = SessionId(self.next.fetch_add(1, Ordering::Relaxed));
        let session = Session::described(id, name.into(), text.into(), description);
        self.lock().insert(id, session.clone());
        session
    }

    /// Opens a recorded run (a `spikes-record` directory) as a replay session: build it, then start it like a run.
    pub fn open_recording(&self, dir: &std::path::Path) -> Result<Session, SessionError> {
        let data = replay::ReplayData::load(dir)?;
        let id = SessionId(self.next.fetch_add(1, Ordering::Relaxed));
        let session = Session::replay_of(id, data);
        self.lock().insert(id, session.clone());
        Ok(session)
    }

    /// The session with this id.
    pub fn session(&self, id: SessionId) -> Result<Session, SessionError> {
        self.lock()
            .get(&id)
            .cloned()
            .ok_or_else(|| SessionError::NotFound(format!("session {id}")))
    }

    /// All sessions, by id.
    pub fn sessions(&self) -> Vec<Session> {
        self.lock().values().cloned().collect()
    }

    /// Stops a session's run, if any, and removes it.
    pub fn close(&self, id: SessionId) -> Result<(), SessionError> {
        let session = self
            .lock()
            .remove(&id)
            .ok_or_else(|| SessionError::NotFound(format!("session {id}")))?;
        session.stop()?;
        session.leave_cluster();
        Ok(())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<SessionId, Session>> {
        self.sessions.lock().unwrap_or_else(|p| p.into_inner())
    }
}
