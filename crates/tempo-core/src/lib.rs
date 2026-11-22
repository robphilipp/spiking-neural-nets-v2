//! Core types for the `tempo` timed actor framework.
//!
//! These are the types every other `tempo-*` crate shares:
//!
//! - **Time**: [`SimTime`] and [`SimDuration`], integer nanoseconds of *simulation* time. Every message carries the
//!   exact time it should be delivered; actors never read the wall clock.
//! - **Identity**: [`ActorId`], allocated in blocks ([`BlockId`]) that the directory maps to a node and shard;
//!   [`Location`] says where an actor physically runs.
//! - **Messages and ordering**: [`Envelope`] wraps a message with its delivery time and addressing. [`OrderKey`]
//!   defines the order events are processed in: `(time, sender, sender sequence number)`.
//!
//! This crate has no runtime, threads or I/O, and must not depend on any `spikes-*` crate.
//! The design is in `docs/plans/actors/`.
//!
//! # Features
//! - `serde`: `Serialize`/`Deserialize` for all types, needed to send them between processes.

mod envelope;
mod id;
mod time;

pub use envelope::{Envelope, OrderKey, SeqCounter};
pub use id::{ActorId, BlockId, Location, NodeId, ShardId};
pub use time::{FromFloatError, SimDuration, SimTime};
