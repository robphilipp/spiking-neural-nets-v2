//! Executor for the `tempo` timed actor framework.
//!
//! An actor is a plain struct implementing [`Actor`]. Actors are stored in per-type arenas: there is no task,
//! thread or mailbox per actor, so millions fit in memory. Pending messages live in one time-ordered queue and are
//! delivered strictly in `(time, sender, sequence, sub)` order, with [`Ctx::now`] equal to the message's delivery
//! time.
//!
//! Actors are spread over **shards**, one thread each, that run in parallel and synchronize at the end of each
//! time window (see the `shard` module docs). Placement is by **partition** ([`placement`]), so results don't
//! depend on the number of shards. The **clock** ([`ClockHandle`]) paces a run against the wall clock and can be
//! sped up, slowed down, paused, stepped or stopped from another thread while it runs. The **control plane**
//! ([`Control`]) inspects and changes a running simulation; **subscriptions** ([`EventStream`]) and **system
//! events** ([`SystemEvents`]) stream what happens without ever blocking it. See `docs/plans/actors/`.
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
mod barrier;
mod broadcast;
mod clock;
pub mod cluster;
mod control;
mod ctx;
mod events;
mod field;
mod metrics;
pub mod placement;
mod reply;
mod route;
mod runtime;
mod shard;
#[cfg(feature = "serde")]
mod snapshot;
mod subscribe;
mod system;

pub use actor::{Actor, EdgeIdx, RouteId, Timer, TimerToken};
pub use broadcast::{GroupId, SpatialBroadcast};
pub use clock::{ClockHandle, ClockSchedule, Pacing, StopToken};
pub use cluster::{Link, NodePlacement};
pub use control::{Control, Injected, TopologyOp};
pub use ctx::Ctx;
pub use events::{Emitted, EventCallback};
pub use field::Field;
pub use metrics::{LatencyHistogram, PacingStats, Stats};
pub use placement::Partition;
pub use reply::{Cancelled, Reply};
pub use route::{Jitter, MAX_ROUTE_DELAY, RouteEdge};
pub use runtime::{EventPublisher, QueueKind, Runtime, RuntimeConfig, SystemPublisher};
pub use shard::{DeliveryKind, TraceEntry};
#[cfg(feature = "serde")]
pub use snapshot::{ActorRecord, EventRecord, Persist, PersistField, Snapshot, SnapshotCodec, SnapshotError};
pub use subscribe::{Closed, EventFilter, EventStream};
pub use system::{Severity, SystemEvent, SystemEvents, SystemNotice};
pub use tempo_core::{ActorId, SimDuration, SimTime};

/// Each actor's random-number generator: Xoshiro256++ (32 bytes), seeded from the runtime seed and the actor's ID.
pub type ActorRng = rand_xoshiro::Xoshiro256PlusPlus;
