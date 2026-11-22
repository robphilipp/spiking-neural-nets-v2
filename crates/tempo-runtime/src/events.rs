//! Domain-event output: what actors report with [`crate::Ctx::emit`].
//!
//! For now events are discarded, buffered in memory (per shard, merged on read), or passed to a callback. The
//! batched UI stream and the Arrow file sink come in phase 3.

use std::sync::Arc;

use tempo_core::{ActorId, SimTime};

/// An event emitted by an actor, stamped with when and by whom.
#[derive(Clone, Debug, PartialEq)]
pub struct Emitted<E> {
    /// Simulation time of the emitting handler.
    pub at: SimTime,
    /// The emitting actor.
    pub src: ActorId,
    /// The event.
    pub event: E,
}

/// Callback that receives each emitted event. With several shards it is called from several threads at once.
pub type EventCallback<E> = Arc<dyn Fn(Emitted<E>) + Send + Sync>;

pub(crate) enum Events<E> {
    Discard,
    Buffer(Vec<Emitted<E>>),
    Callback(EventCallback<E>),
}

impl<E> Events<E> {
    /// The same output mode with nothing buffered yet: one per shard.
    pub(crate) fn fresh(&self) -> Self {
        match self {
            Self::Discard => Self::Discard,
            Self::Buffer(_) => Self::Buffer(Vec::new()),
            Self::Callback(callback) => Self::Callback(Arc::clone(callback)),
        }
    }

    pub(crate) fn emit(&mut self, event: Emitted<E>) {
        match self {
            Self::Discard => {}
            Self::Buffer(buffer) => buffer.push(event),
            Self::Callback(callback) => callback(event),
        }
    }
}
