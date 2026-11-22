//! The context an actor gets while handling a message.

use tempo_core::{ActorId, OrderKey, SeqCounter, SimDuration, SimTime};

use crate::shard::{Core, Item, World};
use crate::{ActorRng, Emitted, RouteId};

/// Per-actor framework state, stored next to the actor.
#[derive(Debug)]
pub(crate) struct ActorMeta {
    pub(crate) seq: SeqCounter,
    pub(crate) rng: ActorRng,
}

/// What an actor can do while handling a message: read the time, send messages, use its random-number stream and
/// emit events.
pub struct Ctx<'a, M, E> {
    pub(crate) now: SimTime,
    pub(crate) me: ActorId,
    pub(crate) meta: &'a mut ActorMeta,
    pub(crate) core: &'a mut Core<M, E>,
    pub(crate) world: &'a World,
}

impl<M, E> Ctx<'_, M, E> {
    /// This actor's ID.
    pub fn me(&self) -> ActorId {
        self.me
    }

    /// The current simulation time: the delivery time of the message being handled.
    pub fn now(&self) -> SimTime {
        self.now
    }

    /// Sends `msg` to `to`, delivered at the current time (after this handler returns).
    ///
    /// A message to an actor on another shard needs at least the runtime's lookahead as delay; shorter ones are
    /// delivered slightly late and counted in [`crate::Stats::lookahead_violations`].
    pub fn send(&mut self, to: ActorId, msg: M) {
        self.send_at(to, self.now, msg);
    }

    /// Sends `msg` to `to`, delivered `delay` from now.
    pub fn send_after(&mut self, to: ActorId, delay: SimDuration, msg: M) {
        self.send_at(to, self.now + delay, msg);
    }

    /// Sends `msg` to `to`, delivered at `at`.
    ///
    /// # Panics
    /// If `at` is earlier than [`Self::now`].
    pub fn send_at(&mut self, to: ActorId, at: SimTime, msg: M) {
        assert!(at >= self.now, "cannot send into the past: {at} < {}", self.now);
        let key = OrderKey {
            at,
            src: self.me,
            seq: self.meta.seq.next_seq(),
            sub: 0,
        };
        self.core.send(self.world, key, Item::Message { dst: to, msg });
    }

    /// Sends `msg` to this actor after `delay`, e.g. for a timer (see [`crate::Timer`]).
    pub fn schedule_self(&mut self, delay: SimDuration, msg: M) {
        self.send_after(self.me, delay, msg);
    }

    /// The actor's random-number stream, seeded from the runtime seed and the actor's ID. Use `rand::RngExt` for
    /// convenience methods such as `random::<f64>()`.
    pub fn rng(&mut self) -> &mut ActorRng {
        &mut self.meta.rng
    }

    /// Emits a domain event (a spike, a weight change, …) to the runtime's event output. Never blocks.
    pub fn emit(&mut self, event: E) {
        self.core.events.emit(Emitted {
            at: self.now,
            src: self.me,
            event,
        });
    }
}

impl<M: Clone, E> Ctx<'_, M, E> {
    /// Sends `msg` along every edge of `route`, each delivered after that edge's delay from now.
    ///
    /// # Panics
    /// If the route doesn't exist or isn't owned by this actor.
    pub fn fanout(&mut self, route: RouteId, msg: M) {
        self.fanout_at(route, self.now, msg);
    }

    /// Sends `msg` along every edge of `route`, each delivered at `base` plus that edge's delay. Useful when the
    /// send itself is delayed, e.g. by a neuron's rise time.
    ///
    /// # Panics
    /// If `base` is earlier than [`Self::now`], or the route doesn't exist or isn't owned by this actor.
    pub fn fanout_at(&mut self, route: RouteId, base: SimTime, msg: M) {
        assert!(base >= self.now, "cannot send into the past: {base} < {}", self.now);
        let seq = self.meta.seq.next_seq();
        let world = self.world;
        let route = world
            .routes
            .get(route.0 as usize)
            .unwrap_or_else(|| panic!("unknown route {route:?}"));
        assert_eq!(
            route.owner, self.me,
            "{route:?} belongs to {}, not {}",
            route.owner, self.me
        );
        for (sub, edge) in route.edges.iter().enumerate() {
            let sub = sub as u32;
            let delay = route.jitter.apply(edge.delay, world.seed, self.me, seq, sub);
            let key = OrderKey {
                at: base + delay,
                src: self.me,
                seq,
                sub,
            };
            let item = Item::Routed {
                dst: edge.target,
                edge: edge.edge,
                msg: msg.clone(),
            };
            self.core.send(world, key, item);
        }
        self.core.stats.routed_sends += route.edges.len() as u64;
    }
}

impl<M, E> std::fmt::Debug for Ctx<'_, M, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ctx")
            .field("now", &self.now)
            .field("me", &self.me)
            .finish_non_exhaustive()
    }
}
