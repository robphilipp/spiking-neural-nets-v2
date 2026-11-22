//! Executor for the `tempo` timed actor framework.
//!
//! An actor is a plain struct implementing [`Actor`]. Actors are stored in per-type arenas: there is no task,
//! thread or mailbox per actor, so millions fit in memory. Pending messages live in one time-ordered queue and are
//! delivered strictly in `(time, sender, sequence, sub)` order, with [`Ctx::now`] equal to the message's delivery
//! time.
//!
//! This is phase 1: a single-threaded, as-fast-as-possible executor. Phase 2 runs one of these per core and
//! synchronizes them with time windows; see `docs/plans/actors/`.
//!
//! ```
//! use tempo_runtime::{Actor, ActorId, Ctx, Runtime, RuntimeConfig, SimDuration, SimTime};
//!
//! struct Echo;
//! impl Actor<u32, u32> for Echo {
//!     fn on_message(&mut self, ctx: &mut Ctx<'_, u32, u32>, _from: ActorId, n: u32) {
//!         ctx.emit(n);
//!         if n < 3 {
//!             ctx.schedule_self(SimDuration::from_micros(10), n + 1);
//!         }
//!     }
//! }
//!
//! let mut rt = Runtime::new(RuntimeConfig::default());
//! rt.buffer_events();
//! let echo = rt.spawn(Echo);
//! rt.inject(echo, SimTime::ZERO, 0);
//! rt.run();
//! let times: Vec<_> = rt.take_events().iter().map(|e| e.at.as_nanos()).collect();
//! assert_eq!(times, [0, 10_000, 20_000, 30_000]);
//! ```

mod actor;
mod ctx;
mod events;
mod route;
mod runtime;

pub use actor::{Actor, EdgeIdx, RouteId, Timer, TimerToken};
pub use ctx::Ctx;
pub use events::{Emitted, EventCallback};
pub use route::{Jitter, RouteEdge};
pub use runtime::{DeliveryKind, QueueKind, Runtime, RuntimeConfig, Stats, TraceEntry};
pub use tempo_core::{ActorId, SimDuration, SimTime};

/// Each actor's random-number generator: Xoshiro256++ (32 bytes), seeded from the runtime seed and the actor's ID.
pub type ActorRng = rand_xoshiro::Xoshiro256PlusPlus;
