//! The context an actor gets while handling a message.

use std::any::TypeId;

use tempo_core::{ActorId, OrderKey, SeqCounter, SimDuration, SimTime};

use crate::broadcast::{GroupId, SpatialBroadcast, Target};
use crate::control::TopologyOp;
use crate::shard::{Core, Item, Routes, World};
use crate::{ActorRng, Emitted, Field, RouteId};

/// Per-actor framework state, stored next to the actor.
#[derive(Debug)]
pub(crate) struct ActorMeta {
    pub(crate) seq: SeqCounter,
    pub(crate) rng: ActorRng,
    /// Set by the control plane: emit detailed events for this actor.
    pub(crate) observed: bool,
    /// A handler panicked (with panic isolation on): further messages are dropped.
    pub(crate) failed: bool,
}

impl ActorMeta {
    pub(crate) fn new(rng: ActorRng) -> Self {
        Self {
            seq: SeqCounter::new(),
            rng,
            observed: false,
            failed: false,
        }
    }
}

/// What an actor can do while handling a message: read the time, send messages, use its random-number stream,
/// read shared fields, and emit events.
pub struct Ctx<'a, M, E> {
    pub(crate) now: SimTime,
    pub(crate) me: ActorId,
    pub(crate) meta: &'a mut ActorMeta,
    pub(crate) core: &'a mut Core<M, E>,
    pub(crate) routes: &'a Routes,
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

    /// `true` if this actor is being observed (see [`crate::Control::observe`]). Use it to decide whether to emit
    /// detailed events, such as every membrane-potential update.
    pub fn is_observed(&self) -> bool {
        self.meta.observed
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
        let key = self.next_key(at);
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

    /// This shard's copy of field `F` (see [`crate::Field`]), or `None` if no such field was added.
    pub fn field<F: Field>(&self) -> Option<&F> {
        let index = self.world.field_types.iter().position(|t| *t == TypeId::of::<F>())?;
        self.core.fields[index].as_any().downcast_ref()
    }

    /// This shard's copy of field `F` together with the actor's random stream, for handlers that need both at
    /// once (the borrow checker won't allow [`Self::field`] and [`Self::rng`] together).
    pub fn field_and_rng<F: Field>(&mut self) -> (Option<&F>, &mut ActorRng) {
        let field = self
            .world
            .field_types
            .iter()
            .position(|t| *t == TypeId::of::<F>())
            .and_then(|index| self.core.fields[index].as_any().downcast_ref());
        (field, &mut self.meta.rng)
    }

    /// Broadcasts an update to field `F` on every shard, applied `delay` from now. With several shards, `delay`
    /// should be at least the lookahead.
    ///
    /// # Panics
    /// If no field of type `F` was added to the runtime.
    pub fn update_field<F: Field>(&mut self, delay: SimDuration, update: F::Update) {
        let field = self
            .world
            .field_types
            .iter()
            .position(|t| *t == TypeId::of::<F>())
            .unwrap_or_else(|| panic!("no field of type {}", std::any::type_name::<F>()));
        let key = self.next_key(self.now + delay);
        for shard in 0..self.world.shards {
            let item = Item::Field {
                field: field as u16,
                update: Box::new(update.clone()),
            };
            self.core.send_to_shard(shard as u16, key, item);
        }
    }

    /// Adds an edge to, or prunes an edge from, a route (for example structural plasticity). The change is made on
    /// the route owner's shard, at the next window boundary.
    pub fn change_route(&mut self, route: RouteId, op: TopologyOp) {
        let key = self.next_key(self.now);
        self.core.send(
            self.world,
            key,
            Item::Topology {
                route,
                op: Box::new(op),
            },
        );
    }

    fn next_key(&mut self, at: SimTime) -> OrderKey {
        OrderKey {
            at,
            src: self.me,
            seq: self.meta.seq.next_seq(),
            sub: 0,
        }
    }
}

impl<M, E: Clone> Ctx<'_, M, E> {
    /// Emits a domain event (a spike, a weight change, …) to the runtime's event outputs. Never blocks.
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
        let r = self
            .routes
            .get(&route)
            .unwrap_or_else(|| panic!("unknown route {route:?} (or not on this actor's shard)"));
        assert_eq!(r.owner, self.me, "{route:?} belongs to {}, not {}", r.owner, self.me);
        for (sub, edge) in r.edges.iter().enumerate() {
            let sub = sub as u32;
            let delay = r.jitter.apply(edge.delay, world.seed, self.me, seq, sub);
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
        self.core.stats.routed_sends += r.edges.len() as u64;
    }

    /// Sends `msg` to every member of `group` (see [`crate::Runtime::add_group`]), delivered `delay` from now.
    /// One message goes to each shard and is expanded there.
    pub fn multicast(&mut self, group: GroupId, delay: SimDuration, msg: M) {
        self.broadcast(Target::Group(group), self.now + delay, msg);
    }

    /// Sends `msg` to every actor with a position (see [`crate::Runtime::set_position`]) within the region, each
    /// after `base_delay + distance / speed`.
    pub fn broadcast_spatial(&mut self, region: SpatialBroadcast, msg: M) {
        self.broadcast(Target::Region(region), self.now + region.base_delay, msg);
    }

    fn broadcast(&mut self, target: Target, at: SimTime, msg: M) {
        let key = self.next_key(at);
        for shard in 0..self.world.shards {
            let item = Item::Broadcast {
                target: Box::new(target),
                msg: msg.clone(),
            };
            self.core.send_to_shard(shard as u16, key, item);
        }
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
