//! Domain-event output: what actors report with [`crate::Ctx::emit`].
//!
//! Each shard can, independently: keep events in memory (read with `Runtime::take_events`), pass each one to a
//! callback, and hand them in per-window batches to live subscribers ([`crate::EventStream`]). The Arrow file
//! writer belongs to the neuron crates, since it needs the domain's event schema; it plugs in as a subscriber.

use std::sync::Arc;

use tempo_core::{ActorId, SimTime};

use crate::subscribe::Subscribers;

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

/// One shard's event outputs.
pub(crate) struct EventOut<E> {
    pub(crate) buffer: Option<Vec<Emitted<E>>>,
    pub(crate) callback: Option<EventCallback<E>>,
    pending: Vec<Emitted<E>>,
    subscribers: Arc<Subscribers<E>>,
}

impl<E: Clone> EventOut<E> {
    pub(crate) fn new(subscribers: Arc<Subscribers<E>>) -> Self {
        Self {
            buffer: None,
            callback: None,
            pending: Vec::new(),
            subscribers,
        }
    }

    pub(crate) fn emit(&mut self, event: Emitted<E>) {
        if let Some(callback) = &self.callback {
            callback(event.clone());
        }
        let live = self.subscribers.active();
        match (&mut self.buffer, live) {
            (Some(buffer), true) => {
                buffer.push(event.clone());
                self.pending.push(event);
            }
            (Some(buffer), false) => buffer.push(event),
            (None, true) => self.pending.push(event),
            (None, false) => {}
        }
    }

    /// Hands this window's events to the subscribers.
    pub(crate) fn flush(&mut self) {
        if !self.pending.is_empty() {
            self.subscribers.publish(std::mem::take(&mut self.pending));
        }
    }
}
