//! Domain-event output: what actors report with [`crate::Ctx::emit`].
//!
//! Phase 1 keeps this simple: events are discarded, buffered in memory, or passed to a callback. The batched
//! UI stream and the Arrow file sink come in phase 3.

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

/// Callback that receives each emitted event.
pub type EventCallback<E> = Box<dyn FnMut(Emitted<E>) + Send>;

pub(crate) enum Events<E> {
    Discard,
    Buffer(Vec<Emitted<E>>),
    Callback(EventCallback<E>),
}

impl<E> Events<E> {
    pub(crate) fn emit(&mut self, event: Emitted<E>) {
        match self {
            Self::Discard => {}
            Self::Buffer(buffer) => buffer.push(event),
            Self::Callback(callback) => callback(event),
        }
    }
}
