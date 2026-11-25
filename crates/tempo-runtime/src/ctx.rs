//! The context an actor gets while handling a message.

use std::any::TypeId;

use tempo_core::{ActorId, OrderKey, SeqCounter, SimDuration, SimTime};

use crate::broadcast::{GroupId, SpatialBroadcast, Target};
use crate::cluster::ALL_SHARDS;
use crate::control::TopologyOp;
use crate::shard::{Core, Item, Routes, World, bits};
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
        self.core.send_everywhere(self.world, key, || Item::Field {
            field: field as u16,
            update: Box::new(update.clone()),
        });
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
    /// A route's edges live with their targets: the edges to actors on this shard are delivered from here, and every
    /// other shard (or node) holding some of the route's edges gets the message once and delivers them there. The
    /// events are the same either way: each is keyed by this send and the receiver's edge index.
    ///
    /// # Panics
    /// If `base` is earlier than [`Self::now`], or the route doesn't exist or isn't owned by this actor.
    pub fn fanout_at(&mut self, route: RouteId, base: SimTime, msg: M) {
        assert!(base >= self.now, "cannot send into the past: {base} < {}", self.now);
        let seq = self.meta.seq.next_seq();
        let world = self.world;
        let home = world
            .dir
            .routes
            .get(route.0 as usize)
            .unwrap_or_else(|| panic!("unknown route {route:?}"));
        assert_eq!(
            home.owner, self.me,
            "{route:?} belongs to {}, not {}",
            home.owner, self.me
        );
        let shard = self.core.shard;
        // This shard's slice.
        if let Some(edges) = self.routes.get(&route) {
            for e in edges {
                let delay = home
                    .jitter
                    .apply(e.delay(), world.seed, self.me, seq, e.target, e.edge.0);
                let key = OrderKey {
                    at: base + delay,
                    src: self.me,
                    seq,
                    sub: e.edge.0,
                };
                let item = Item::Routed {
                    dst: e.target,
                    edge: e.edge,
                    msg: msg.clone(),
                };
                self.core.send_to_shard(shard, key, item);
            }
            self.core.stats.routed_sends += edges.len() as u64;
        }
        // Every other shard and node with a slice: once each, timed by the route's shortest edge away from here
        // (the slices' events are all at or after it; arriving no earlier than needed leaves other nodes slack).
        let others = world.reach.slices(route) & !(1 << shard);
        let nodes = world.reach.nodes(route);
        if others | nodes == 0 {
            return;
        }
        let soonest = world.reach.elsewhere(route).saturating_sub(home.jitter.max_reduction());
        let key = OrderKey {
            at: (base + soonest).max(self.core.window_end),
            src: self.me,
            seq,
            sub: 0,
        };
        for to in bits(others) {
            self.core.send_to_shard(
                to as u16,
                key,
                Item::Fanout {
                    route,
                    base,
                    msg: msg.clone(),
                },
            );
        }
        for node in bits(nodes) {
            self.core.send_to_node(
                node,
                ALL_SHARDS,
                key,
                Item::Fanout {
                    route,
                    base,
                    msg: msg.clone(),
                },
            );
        }
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
        self.core.send_everywhere(self.world, key, || Item::Broadcast {
            target: Box::new(target),
            msg: msg.clone(),
        });
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
