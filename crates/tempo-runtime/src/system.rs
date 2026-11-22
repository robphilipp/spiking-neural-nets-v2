//! System events: notices about the runtime itself (not the model), such as "can't keep up with the clock".
//! They go onto a queue that a UI or log can subscribe to.

use std::collections::VecDeque;
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, SystemTime};

use tempo_core::{ActorId, SimDuration, SimTime};

/// How serious a notice is.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// For information.
    Info,
    /// Something the user should probably act on.
    Warning,
    /// Something failed.
    Error,
}

/// What happened.
#[derive(Clone, Debug, PartialEq)]
pub enum SystemEvent {
    /// The simulation has fallen behind the clock by more than the warning threshold. The clock is *not* slowed;
    /// cross-shard and input timing degrade until it catches up or the user lowers the speed.
    CannotKeepUp {
        /// How far behind the clock the simulation is.
        lag: Duration,
        /// The requested time factor.
        time_factor: f64,
    },
    /// The simulation is keeping up with the clock again.
    CaughtUp {
        /// Current lag.
        lag: Duration,
    },
    /// The lookahead changed, e.g. because a shorter connection between shards was added.
    LookaheadChanged {
        /// Previous lookahead.
        old: SimDuration,
        /// New lookahead.
        new: SimDuration,
    },
    /// An actor's handler panicked; the actor is disabled and its later messages are dropped.
    ActorFailed {
        /// The actor.
        id: ActorId,
        /// The panic message.
        message: String,
    },
    /// Connections were added or pruned on a shard since its last notice (reported at most every 100 ms).
    TopologyChanged {
        /// The shard.
        shard: u16,
        /// Connections added.
        added: u64,
        /// Connections pruned.
        pruned: u64,
    },
}

impl SystemEvent {
    /// The event's severity.
    pub fn severity(&self) -> Severity {
        match self {
            Self::CannotKeepUp { .. } => Severity::Warning,
            Self::ActorFailed { .. } => Severity::Error,
            Self::CaughtUp { .. } | Self::LookaheadChanged { .. } | Self::TopologyChanged { .. } => Severity::Info,
        }
    }
}

/// A system event with when it happened.
#[derive(Clone, Debug, PartialEq)]
pub struct SystemNotice {
    /// Simulation time (start of the window in which it was noticed).
    pub sim_time: SimTime,
    /// Wall-clock time.
    pub wall_time: SystemTime,
    /// How serious it is.
    pub severity: Severity,
    /// What happened.
    pub event: SystemEvent,
}

/// How many recent notices are kept for [`crate::Runtime::recent_system_events`].
const RECENT: usize = 256;
/// How many notices a subscriber can fall behind before new ones are dropped for it.
const SUBSCRIBER_CAPACITY: usize = 1024;

/// The system event queue: fans notices out to subscribers and keeps the most recent ones.
#[derive(Debug, Default)]
pub(crate) struct SystemBus {
    subscribers: Mutex<Vec<SyncSender<SystemNotice>>>,
    recent: Mutex<VecDeque<SystemNotice>>,
}

impl SystemBus {
    pub(crate) fn publish(&self, sim_time: SimTime, event: SystemEvent) {
        let notice = SystemNotice {
            sim_time,
            wall_time: SystemTime::now(),
            severity: event.severity(),
            event,
        };
        {
            let mut recent = lock(&self.recent);
            if recent.len() == RECENT {
                recent.pop_front();
            }
            recent.push_back(notice.clone());
        }
        lock(&self.subscribers).retain(|tx| !matches!(tx.try_send(notice.clone()), Err(TrySendError::Disconnected(_))));
    }

    pub(crate) fn subscribe(&self) -> SystemEvents {
        let (tx, rx) = sync_channel(SUBSCRIBER_CAPACITY);
        lock(&self.subscribers).push(tx);
        SystemEvents { rx }
    }

    pub(crate) fn recent(&self) -> Vec<SystemNotice> {
        lock(&self.recent).iter().cloned().collect()
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// A subscription to system events. Notices published after subscribing arrive here; if the subscriber falls more
/// than 1,024 behind, newer ones are dropped for it (the simulation never waits).
#[derive(Debug)]
pub struct SystemEvents {
    rx: Receiver<SystemNotice>,
}

impl SystemEvents {
    /// The next notice, if one is waiting.
    pub fn try_next(&self) -> Option<SystemNotice> {
        self.rx.try_recv().ok()
    }

    /// Waits up to `timeout` for the next notice.
    pub fn next_timeout(&self, timeout: Duration) -> Option<SystemNotice> {
        self.rx.recv_timeout(timeout).ok()
    }

    /// All notices waiting now.
    pub fn drain(&self) -> Vec<SystemNotice> {
        self.rx.try_iter().collect()
    }
}
