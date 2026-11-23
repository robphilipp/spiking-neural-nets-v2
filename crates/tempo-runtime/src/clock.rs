//! The simulation clock: how simulation time maps to wall-clock time, controllable while a run is in progress.
//!
//! In real-time mode the mapping is `sim(w) = anchor_sim + (w − anchor_wall) / time_factor`, where `time_factor` is
//! wall seconds per simulated second. Changing the factor re-anchors the mapping at the current instant, so
//! simulation time never jumps. Pausing freezes it; stepping (while paused) moves it forward by a set amount.
//!
//! The clock only limits *how far* shards may run. The simulation time each actor sees always comes from its
//! message, never from the wall clock.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime};

use tempo_core::{SimDuration, SimTime};

/// How a run is paced against the wall clock.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Pacing {
    /// Run as fast as possible.
    AsFastAsPossible,
    /// Track the wall clock: one simulated second takes `time_factor` wall seconds (1.0 is real time, 10.0 is ten
    /// times slower, 0.5 twice as fast).
    RealTime {
        /// Wall seconds per simulated second.
        time_factor: f64,
    },
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) enum ClockState {
    Unpaced,
    Running {
        anchor_wall: Instant,
        anchor_sim: SimTime,
        time_factor: f64,
    },
    Paused {
        at: SimTime,
        time_factor: f64,
    },
}

impl ClockState {
    /// The latest simulation time the clock allows at wall time `now`; `None` means unlimited.
    pub(crate) fn horizon(&self, now: Instant) -> Option<SimTime> {
        match *self {
            Self::Unpaced => None,
            Self::Running {
                anchor_wall,
                anchor_sim,
                time_factor,
            } => {
                let wall_ns = now.saturating_duration_since(anchor_wall).as_nanos() as f64;
                Some(anchor_sim.saturating_add(SimDuration::from_nanos((wall_ns / time_factor) as u64)))
            }
            Self::Paused { at, .. } => Some(at),
        }
    }

    /// The wall time at which simulation time `t` is reached, if the clock is running.
    pub(crate) fn wall_for(&self, t: SimTime) -> Option<Instant> {
        match *self {
            Self::Running {
                anchor_wall,
                anchor_sim,
                time_factor,
            } => {
                let sim_ns = t.saturating_duration_since(anchor_sim).as_nanos() as f64;
                anchor_wall.checked_add(Duration::from_nanos((sim_ns * time_factor) as u64))
            }
            _ => None,
        }
    }

    fn time_factor(&self) -> Option<f64> {
        match *self {
            Self::Unpaced => None,
            Self::Running { time_factor, .. } | Self::Paused { time_factor, .. } => Some(time_factor),
        }
    }
}

/// A clock's mapping from wall time to simulation time, in a form another process (another node of a cluster) can
/// follow: wall times are system (calendar) times, which every process on synchronized machines agrees on.
#[derive(Copy, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ClockSchedule {
    /// As fast as possible.
    Unpaced,
    /// Simulation time `anchor_sim` at wall time `anchor_wall`, then `time_factor` wall seconds per simulated second.
    Running {
        /// When.
        anchor_wall: SystemTime,
        /// Where.
        anchor_sim: SimTime,
        /// Wall seconds per simulated second.
        time_factor: f64,
    },
    /// Stopped at `at`.
    Paused {
        /// Where.
        at: SimTime,
        /// The factor to resume with.
        time_factor: f64,
    },
}

#[derive(Debug)]
struct ClockShared {
    state: Mutex<ClockState>,
    /// The schedule was set from outside ([`ClockHandle::follow`]): a run's start keeps its anchor.
    following: AtomicBool,
    version: AtomicU64,
    stop_epoch: AtomicU64,
    progress: AtomicU64,
}

/// Marks a moment in a clock's stop history (see [`ClockHandle::stop_token`]).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct StopToken(pub(crate) u64);

/// A handle to a runtime's clock. Cheap to clone and safe to use from any thread (for example a UI thread) while
/// the runtime is running.
#[derive(Clone, Debug)]
pub struct ClockHandle {
    shared: Arc<ClockShared>,
}

impl ClockHandle {
    pub(crate) fn new(pacing: Pacing) -> Self {
        let state = match pacing {
            Pacing::AsFastAsPossible => ClockState::Unpaced,
            Pacing::RealTime { time_factor } => {
                check_factor(time_factor);
                ClockState::Running {
                    anchor_wall: Instant::now(),
                    anchor_sim: SimTime::ZERO,
                    time_factor,
                }
            }
        };
        Self {
            shared: Arc::new(ClockShared {
                state: Mutex::new(state),
                following: AtomicBool::new(false),
                version: AtomicU64::new(0),
                stop_epoch: AtomicU64::new(0),
                progress: AtomicU64::new(0),
            }),
        }
    }

    /// Runs in real time at `time_factor` wall seconds per simulated second, continuing from the current
    /// simulation time. Works whether the clock was running, paused (stays paused, with the new factor for
    /// resuming) or unpaced.
    ///
    /// # Panics
    /// If `time_factor` isn't a positive, finite number.
    pub fn set_time_factor(&self, time_factor: f64) {
        check_factor(time_factor);
        let progress = self.sim_time();
        self.update(|state, now| {
            *state = match *state {
                ClockState::Running { .. } => ClockState::Running {
                    anchor_wall: now,
                    anchor_sim: state.horizon(now).expect("running clock has a horizon"),
                    time_factor,
                },
                ClockState::Paused { at, .. } => ClockState::Paused { at, time_factor },
                ClockState::Unpaced => ClockState::Running {
                    anchor_wall: now,
                    anchor_sim: progress,
                    time_factor,
                },
            }
        });
    }

    /// Stops pacing: run as fast as possible.
    pub fn set_as_fast_as_possible(&self) {
        self.update(|state, _| *state = ClockState::Unpaced);
    }

    /// Freezes simulation time at its current value. Shards finish their current window and wait.
    pub fn pause(&self) {
        let progress = self.sim_time();
        self.update(|state, now| {
            *state = match *state {
                ClockState::Running { time_factor, .. } => ClockState::Paused {
                    at: state.horizon(now).expect("running clock has a horizon"),
                    time_factor,
                },
                ClockState::Unpaced => ClockState::Paused {
                    at: progress,
                    time_factor: 1.0,
                },
                paused @ ClockState::Paused { .. } => paused,
            }
        });
    }

    /// Resumes real-time pacing from where the clock was paused. Does nothing unless paused.
    pub fn resume(&self) {
        self.update(|state, now| {
            if let ClockState::Paused { at, time_factor } = *state {
                *state = ClockState::Running {
                    anchor_wall: now,
                    anchor_sim: at,
                    time_factor,
                };
            }
        });
    }

    /// While paused, lets the simulation advance by `by` and pause again. Does nothing unless paused.
    pub fn step(&self, by: SimDuration) {
        self.update(|state, _| {
            if let ClockState::Paused { at, time_factor } = *state {
                *state = ClockState::Paused {
                    at: at.saturating_add(by),
                    time_factor,
                };
            }
        });
    }

    /// Ends the current run at the next window boundary. The runtime's `run…` call then returns, and a later call
    /// continues from where it stopped.
    pub fn stop(&self) {
        self.shared.stop_epoch.fetch_add(1, Ordering::AcqRel);
        self.shared.version.fetch_add(1, Ordering::AcqRel);
    }

    /// A marker for "now", for [`crate::Runtime::run_until_unless_stopped`]: stops issued after it end that run.
    pub fn stop_token(&self) -> StopToken {
        StopToken(self.stop_epoch())
    }

    /// `true` while paused.
    pub fn is_paused(&self) -> bool {
        matches!(*self.lock(), ClockState::Paused { .. })
    }

    /// The current time factor, or `None` when running as fast as possible.
    pub fn time_factor(&self) -> Option<f64> {
        self.lock().time_factor()
    }

    /// How far the simulation has progressed: the start of the most recent window.
    pub fn sim_time(&self) -> SimTime {
        SimTime::from_nanos(self.shared.progress.load(Ordering::Acquire))
    }

    /// The latest simulation time the clock currently allows (`None` when running as fast as possible).
    pub fn horizon(&self) -> Option<SimTime> {
        self.lock().horizon(Instant::now())
    }

    /// The clock's schedule, for another node to follow.
    pub fn schedule(&self) -> ClockSchedule {
        match *self.lock() {
            ClockState::Unpaced => ClockSchedule::Unpaced,
            ClockState::Running {
                anchor_wall,
                anchor_sim,
                time_factor,
            } => {
                // An Instant means nothing to another process: express the anchor as a system time.
                let (now_i, now_s) = (Instant::now(), SystemTime::now());
                let anchor_wall = if anchor_wall <= now_i {
                    now_s - (now_i - anchor_wall)
                } else {
                    now_s + (anchor_wall - now_i)
                };
                ClockSchedule::Running {
                    anchor_wall,
                    anchor_sim,
                    time_factor,
                }
            }
            ClockState::Paused { at, time_factor } => ClockSchedule::Paused { at, time_factor },
        }
    }

    /// Follows a schedule (another node's, see [`Self::schedule`]): from now on a run's start keeps the schedule's
    /// anchor instead of anchoring at its own start, so every node paces from the same instant.
    pub fn follow(&self, schedule: ClockSchedule) {
        self.shared.following.store(true, Ordering::Release);
        self.update(|state, now_i| {
            *state = match schedule {
                ClockSchedule::Unpaced => ClockState::Unpaced,
                ClockSchedule::Running {
                    anchor_wall,
                    anchor_sim,
                    time_factor,
                } => {
                    let now_s = SystemTime::now();
                    let anchor_wall = match now_s.duration_since(anchor_wall) {
                        Ok(ago) => now_i.checked_sub(ago).unwrap_or(now_i),
                        Err(ahead) => now_i + ahead.duration(),
                    };
                    ClockState::Running {
                        anchor_wall,
                        anchor_sim,
                        time_factor,
                    }
                }
                ClockSchedule::Paused { at, time_factor } => ClockState::Paused { at, time_factor },
            };
        });
    }

    /// Stops following: a run's start anchors the clock again.
    pub fn unfollow(&self) {
        self.shared.following.store(false, Ordering::Release);
    }

    pub(crate) fn snapshot(&self) -> (u64, ClockState) {
        let version = self.shared.version.load(Ordering::Acquire);
        (version, *self.lock())
    }

    pub(crate) fn version(&self) -> u64 {
        self.shared.version.load(Ordering::Acquire)
    }

    pub(crate) fn stop_epoch(&self) -> u64 {
        self.shared.stop_epoch.load(Ordering::Acquire)
    }

    pub(crate) fn publish_progress(&self, t: SimTime) {
        self.shared.progress.fetch_max(t.as_nanos(), Ordering::AcqRel);
    }

    /// Called when a run starts at simulation time `at`: a running clock is re-anchored so time spent between runs
    /// doesn't count as falling behind.
    pub(crate) fn start_run(&self, at: SimTime) {
        self.shared.progress.fetch_max(at.as_nanos(), Ordering::AcqRel);
        let following = self.shared.following.load(Ordering::Acquire);
        self.update(|state, now| match *state {
            ClockState::Running { .. } if following => {}
            ClockState::Running { time_factor, .. } => {
                *state = ClockState::Running {
                    anchor_wall: now,
                    anchor_sim: at,
                    time_factor,
                };
            }
            ClockState::Paused {
                at: paused,
                time_factor,
            } => {
                *state = ClockState::Paused {
                    at: paused.max(at),
                    time_factor,
                };
            }
            ClockState::Unpaced => {}
        });
    }

    fn update(&self, f: impl FnOnce(&mut ClockState, Instant)) {
        let mut state = self.lock();
        f(&mut state, Instant::now());
        self.shared.version.fetch_add(1, Ordering::AcqRel);
    }

    fn lock(&self) -> MutexGuard<'_, ClockState> {
        self.shared
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn check_factor(time_factor: f64) {
    assert!(
        time_factor.is_finite() && time_factor > 0.0,
        "time_factor must be a positive, finite number, got {time_factor}"
    );
}

/// A shard's cached copy of the clock state, refreshed only when the clock changes.
pub(crate) struct ClockView<'a> {
    handle: &'a ClockHandle,
    version: u64,
    pub(crate) state: ClockState,
}

impl<'a> ClockView<'a> {
    pub(crate) fn new(handle: &'a ClockHandle) -> Self {
        let (version, state) = handle.snapshot();
        Self { handle, version, state }
    }

    pub(crate) fn refresh(&mut self) {
        if self.handle.version() != self.version {
            (self.version, self.state) = self.handle.snapshot();
        }
    }
}

/// Longest single sleep while waiting, so clock changes (speed, pause, stop) take effect promptly.
const MAX_SLEEP: Duration = Duration::from_millis(2);

/// Waits until the clock allows processing events at simulation time `t`. Sleeps while far away and spins for the
/// last `spin_threshold`. Returns `false` if `abort` becomes true first.
pub(crate) fn wait_for(
    view: &mut ClockView<'_>,
    t: SimTime,
    spin_threshold: Duration,
    abort: impl Fn() -> bool,
) -> bool {
    loop {
        if abort() {
            return false;
        }
        view.refresh();
        let now = Instant::now();
        match view.state.horizon(now) {
            None => return true,
            Some(h) if h >= t => return true,
            Some(_) => {}
        }
        match view.state.wall_for(t).map(|w| w.saturating_duration_since(now)) {
            Some(remaining) if remaining > spin_threshold => {
                std::thread::sleep((remaining - spin_threshold).min(MAX_SLEEP))
            }
            Some(_) => std::hint::spin_loop(),
            None => std::thread::sleep(MAX_SLEEP),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(v: u64) -> SimTime {
        SimTime::from_start(SimDuration::from_millis(v))
    }

    #[test]
    fn horizon_follows_the_time_factor() {
        let t0 = Instant::now();
        let state = ClockState::Running {
            anchor_wall: t0,
            anchor_sim: ms(10),
            time_factor: 2.0,
        };
        assert_eq!(state.horizon(t0 + Duration::from_millis(100)), Some(ms(60)));
        assert_eq!(state.wall_for(ms(60)), Some(t0 + Duration::from_millis(100)));
        assert_eq!(ClockState::Unpaced.horizon(t0), None);
    }

    #[test]
    fn changing_speed_is_continuous() {
        let clock = ClockHandle::new(Pacing::RealTime { time_factor: 1.0 });
        clock.start_run(ms(1_000));
        std::thread::sleep(Duration::from_millis(5));
        let before = clock.horizon().unwrap();
        clock.set_time_factor(100.0);
        let after = clock.horizon().unwrap();
        // The horizon carries on from where it was (allowing for the instant between the two reads).
        assert!(after >= before);
        assert!(after - before < SimDuration::from_millis(2), "{before} -> {after}");
        assert_eq!(clock.time_factor(), Some(100.0));
    }

    #[test]
    fn pause_step_resume() {
        let clock = ClockHandle::new(Pacing::RealTime { time_factor: 1.0 });
        clock.start_run(ms(0));
        clock.pause();
        assert!(clock.is_paused());
        let frozen = clock.horizon().unwrap();
        std::thread::sleep(Duration::from_millis(3));
        assert_eq!(clock.horizon(), Some(frozen));
        clock.step(SimDuration::from_millis(10));
        assert_eq!(clock.horizon(), Some(frozen + SimDuration::from_millis(10)));
        clock.resume();
        assert!(!clock.is_paused());
        assert!(clock.horizon().unwrap() >= frozen + SimDuration::from_millis(10));
    }

    #[test]
    fn wait_for_returns_when_the_clock_reaches_the_time() {
        let clock = ClockHandle::new(Pacing::RealTime { time_factor: 1.0 });
        clock.start_run(SimTime::ZERO);
        let mut view = ClockView::new(&clock);
        let start = Instant::now();
        assert!(wait_for(&mut view, ms(5), Duration::from_micros(200), || false));
        let waited = start.elapsed();
        assert!(
            waited >= Duration::from_millis(5) - Duration::from_micros(50),
            "{waited:?}"
        );
        assert!(waited < Duration::from_millis(50), "{waited:?}");
    }

    #[test]
    fn a_followed_schedule_paces_from_its_anchor() {
        let leader = ClockHandle::new(Pacing::RealTime { time_factor: 1.0 });
        leader.start_run(ms(100));
        std::thread::sleep(Duration::from_millis(5));
        let follower = ClockHandle::new(Pacing::AsFastAsPossible);
        follower.follow(leader.schedule());
        // A run starting later keeps the leader's anchor.
        follower.start_run(ms(100));
        let (a, b) = (leader.horizon().unwrap(), follower.horizon().unwrap());
        let gap = if a > b { a - b } else { b - a };
        assert!(gap < SimDuration::from_micros(500), "{a} vs {b}");
        assert!(b >= ms(105));
        leader.pause();
        follower.follow(leader.schedule());
        assert_eq!(follower.horizon(), leader.horizon());
    }

    #[test]
    fn wait_for_aborts() {
        let clock = ClockHandle::new(Pacing::RealTime { time_factor: 1.0 });
        clock.pause();
        let mut view = ClockView::new(&clock);
        assert!(!wait_for(&mut view, ms(5), Duration::from_micros(200), || true));
    }
}
