//! The [`Actor`] trait and the small types actors use.

use tempo_core::ActorId;

use crate::Ctx;

/// An actor: a plain struct that reacts to timed messages.
///
/// Every handler runs with [`Ctx::now`] equal to the delivery time of the message being handled, and an actor's
/// messages arrive in `(time, sender, sequence, sub)` order. `M` is the simulation's data-plane message type and `E`
/// its domain-event type; every actor in one runtime shares them.
pub trait Actor<M, E>: Send + 'static {
    /// Called once, when the actor is spawned (at time 0 for actors spawned before the run starts).
    fn on_start(&mut self, ctx: &mut Ctx<'_, M, E>) {
        let _ = ctx;
    }

    /// Called for each message sent directly to this actor with `send`, `send_after` or `send_at`.
    fn on_message(&mut self, ctx: &mut Ctx<'_, M, E>, from: ActorId, msg: M);

    /// Called for each message that reaches this actor through a fan-out route. `edge` is the index this actor
    /// gave the connection when the route was built, typically an index into its incoming-synapse table.
    ///
    /// The default ignores the message; actors that are route targets must override it.
    fn on_route_message(&mut self, ctx: &mut Ctx<'_, M, E>, from: ActorId, edge: EdgeIdx, msg: M) {
        let _ = (ctx, from, edge, msg);
    }
}

/// Index of a connection on the receiving actor, e.g. into a neuron's incoming-synapse table.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EdgeIdx(pub u32);

/// Identifies a fan-out route.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RouteId(pub u32);

/// A handle for a cancellable, re-armable self-scheduled timer, with no framework cost.
///
/// Cancellation is lazy: the timer message carries a [`TimerToken`], and when it arrives the actor checks it with
/// [`Timer::fire`]. Arming the timer again or cancelling it makes earlier tokens stale, so their messages are
/// ignored. One `Timer` tracks one pending timer; use several for independent timers.
///
/// ```
/// # use tempo_runtime::{Timer, TimerToken};
/// let mut timer = Timer::default();
/// let first = timer.arm();          // schedule a message carrying `first`
/// let second = timer.arm();         // re-arm: `first` is now stale
/// assert!(!timer.fire(first));
/// assert!(timer.fire(second));      // the live token fires once...
/// assert!(!timer.fire(second));     // ...and only once
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Timer {
    generation: u32,
    armed: bool,
}

/// Identifies one arming of a [`Timer`]. Put it in the timer message.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct TimerToken(u32);

impl Timer {
    /// Arms the timer, invalidating any earlier token, and returns the token for the new timer message.
    pub fn arm(&mut self) -> TimerToken {
        self.generation = self.generation.wrapping_add(1);
        self.armed = true;
        TimerToken(self.generation)
    }

    /// Cancels the pending timer, if any.
    pub fn cancel(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.armed = false;
    }

    /// Call when a timer message arrives. Returns `true` if `token` is the live one (and disarms the timer), or
    /// `false` if the message is stale and should be ignored.
    pub fn fire(&mut self, token: TimerToken) -> bool {
        if self.armed && token.0 == self.generation {
            self.armed = false;
            true
        } else {
            false
        }
    }

    /// `true` while a timer is pending.
    pub fn is_armed(&self) -> bool {
        self.armed
    }
}
