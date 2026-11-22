//! One shard: a single-threaded event loop owning a subset of the actors.
//!
//! Shards run in parallel using conservative time windows. With lookahead `L` (the minimum delay of any message
//! that crosses shards), once every shard has finished everything before `T`, no shard can still create a
//! cross-shard message earlier than `T + L`. So each window is processed by all shards in parallel, then they meet
//! at the [`WindowBarrier`], exchange the cross-shard messages they produced, and the last to arrive decides the
//! next window for all. Unpaced, idle stretches are skipped entirely. Paced, windows follow the clock through idle
//! stretches, so live input never waits for the next scheduled event.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tempo_core::{ActorId, OrderKey, SimDuration, SimTime};
use tempo_queue::{HeapQueue, TimeQueue, WheelQueue};

use crate::barrier::{Decision, WindowBarrier};
use crate::broadcast::{GroupId, SpatialIndex, Target};
use crate::clock::{ClockHandle, ClockState, ClockView, wait_for};
use crate::control::{ControlShared, TopologyOp};
use crate::ctx::ActorMeta;
use crate::events::EventOut;
use crate::field::AnyField;
use crate::metrics::{PacingStats, Stats};
use crate::route::Route;
use crate::system::{SystemBus, SystemEvent};
use crate::{Actor, Ctx, EdgeIdx, RouteId};

/// What kind of delivery a [`TraceEntry`] records.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum DeliveryKind {
    /// The actor's `on_start`.
    Start,
    /// A direct message (or one expanded from a broadcast).
    Message,
    /// A routed message, with the receiver's edge index.
    Routed(EdgeIdx),
}

/// One delivery, as recorded when tracing is on.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct TraceEntry {
    /// The receiving actor.
    pub dst: ActorId,
    /// The event's order key (its time, sender, sequence and sub-index).
    pub key: OrderKey,
    /// What was delivered.
    pub kind: DeliveryKind,
}

/// The routes owned by a shard's actors.
pub(crate) type Routes = HashMap<RouteId, Route>;

/// Deliveries in transit between shards.
pub(crate) type Batch<M> = Vec<(OrderKey, Item<M>)>;

/// A queued event.
pub(crate) enum Item<M> {
    /// A message for one actor.
    Message { dst: ActorId, msg: M },
    /// A message for one actor through a route edge.
    Routed { dst: ActorId, edge: EdgeIdx, msg: M },
    /// A broadcast, sent once to every shard and expanded there. (Boxed: rare, and keeps every item small.)
    Broadcast { target: Box<Target>, msg: M },
    /// An update to a shard-local field, sent once to every shard.
    Field { field: u16, update: Box<dyn Any + Send> },
    /// A change to a route, applied on the route owner's shard. (Boxed, like broadcasts.)
    Topology { route: RouteId, op: Box<TopologyOp> },
}

// Every pending event is one of these, so keep them small: rare variants are boxed.
const _: () = assert!(size_of::<Item<u64>>() <= 24);

/// The configured queue, as an enum so calls are a `match` rather than a virtual call.
pub(crate) enum Queue<T> {
    Heap(HeapQueue<T>),
    Wheel(WheelQueue<T>),
}

impl<T> TimeQueue<T> for Queue<T> {
    fn push(&mut self, key: OrderKey, item: T) {
        match self {
            Self::Heap(q) => q.push(key, item),
            Self::Wheel(q) => q.push(key, item),
        }
    }

    fn peek_key(&mut self) -> Option<OrderKey> {
        match self {
            Self::Heap(q) => q.peek_key(),
            Self::Wheel(q) => q.peek_key(),
        }
    }

    fn pop(&mut self) -> Option<(OrderKey, T)> {
        match self {
            Self::Heap(q) => q.pop(),
            Self::Wheel(q) => q.pop(),
        }
    }

    fn len(&self) -> usize {
        match self {
            Self::Heap(q) => q.len(),
            Self::Wheel(q) => q.len(),
        }
    }
}

/// Where an ID block lives: its shard, its actor kind, and which of the shard's stores holds it.
#[derive(Copy, Clone, Debug)]
pub(crate) struct BlockEntry {
    pub(crate) shard: u16,
    pub(crate) kind: u16,
    pub(crate) store: u32,
    pub(crate) count: u32,
}

/// An actor's physical position.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct Slot {
    pub(crate) shard: u16,
    pub(crate) kind: u16,
    pub(crate) store: u32,
    pub(crate) index: u32,
}

/// Where actors and routes live. Shared (read-only) by shards during a run and by the control plane.
#[derive(Clone, Debug, Default)]
pub(crate) struct Directory {
    pub(crate) blocks: Vec<Option<BlockEntry>>,
    /// The shard that owns each route.
    pub(crate) route_shards: Vec<u16>,
}

impl Directory {
    #[inline(always)]
    pub(crate) fn locate(&self, id: ActorId) -> Option<Slot> {
        let entry = self.blocks.get(id.block().raw() as usize)?.as_ref()?;
        let index = u32::from(id.index_in_block());
        (index < entry.count).then_some(Slot {
            shard: entry.shard,
            kind: entry.kind,
            store: entry.store,
            index,
        })
    }
}

/// Everything shards share read-only while running.
pub(crate) struct World {
    pub(crate) dir: Arc<Directory>,
    pub(crate) kind_types: Vec<TypeId>,
    pub(crate) field_types: Vec<TypeId>,
    pub(crate) shards: usize,
    pub(crate) seed: u64,
}

impl World {
    #[inline(always)]
    pub(crate) fn locate(&self, id: ActorId) -> Option<Slot> {
        self.dir.locate(id)
    }
}

/// A shard's mutable state that handlers reach through [`Ctx`].
pub(crate) struct Core<M, E> {
    pub(crate) shard: u16,
    pub(crate) queue: Queue<Item<M>>,
    pub(crate) events: EventOut<E>,
    pub(crate) stats: Stats,
    /// This shard's copy of each field, indexed like `World::field_types`.
    pub(crate) fields: Vec<Box<dyn AnyField>>,
    /// Cross-shard messages produced this window, per destination shard.
    pub(crate) outbox: Vec<Batch<M>>,
    /// Earliest time among this window's cross-shard messages.
    pub(crate) outbound_min: SimTime,
    /// End of the current window: cross-shard messages earlier than this would be late.
    pub(crate) window_end: SimTime,
    /// The lookahead this shard proposes at the next barrier (lowered when short cross-shard edges are added).
    pub(crate) lookahead_proposal: SimDuration,
    pub(crate) system: Arc<SystemBus>,
    pub(crate) isolate_panics: bool,
    topology_added: u64,
    topology_pruned: u64,
    last_topology_notice: Instant,
}

impl<M, E> Core<M, E> {
    /// Queues an event for wherever its target lives.
    #[inline(always)]
    pub(crate) fn send(&mut self, world: &World, key: OrderKey, item: Item<M>) {
        let shard = match &item {
            Item::Message { dst, .. } | Item::Routed { dst, .. } => world.locate(*dst).map(|slot| slot.shard),
            Item::Topology { route, .. } => world.dir.route_shards.get(route.0 as usize).copied(),
            Item::Broadcast { .. } | Item::Field { .. } => unreachable!("broadcasts are sent to each shard"),
        };
        match shard {
            Some(shard) => self.send_to_shard(shard, key, item),
            None => self.stats.dropped_unknown += 1,
        }
    }

    /// Queues an event on a specific shard.
    #[inline(always)]
    pub(crate) fn send_to_shard(&mut self, shard: u16, mut key: OrderKey, item: Item<M>) {
        if shard == self.shard {
            self.queue.push(key, item);
            return;
        }
        if key.at < self.window_end {
            // Shorter than the lookahead: the receiver may already be past this time. Deliver at the next window
            // start instead (deterministically). Route changes aren't timing-critical, so they aren't counted.
            if !matches!(item, Item::Topology { .. }) {
                let error = self.window_end - key.at;
                self.stats.lookahead_violations += 1;
                self.stats.max_lookahead_violation = self.stats.max_lookahead_violation.max(error);
            }
            key.at = self.window_end;
        }
        self.outbound_min = self.outbound_min.min(key.at);
        self.stats.remote_sends += 1;
        self.outbox[usize::from(shard)].push((key, item));
    }
}

/// Type-erased storage for one ID block's actors (all of one Rust type). Dispatch is one virtual call; inside it
/// the actor's handlers are called statically.
pub(crate) trait KindStore<M, E>: Send {
    fn dispatch(
        &mut self,
        index: u32,
        core: &mut Core<M, E>,
        routes: &Routes,
        world: &World,
        key: OrderKey,
        item: Item<M>,
    );
    fn start(&mut self, index: u32, core: &mut Core<M, E>, routes: &Routes, world: &World, now: SimTime, me: ActorId);
    fn meta_mut(&mut self, index: u32) -> Option<&mut ActorMeta>;
    fn shrink_to_fit(&mut self);
    fn memory_bytes(&self) -> usize;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

pub(crate) struct Arena<A> {
    pub(crate) actors: Vec<A>,
    pub(crate) meta: Vec<ActorMeta>,
}

/// Runs a handler; with panic isolation on, a panic disables the actor instead of unwinding.
#[inline(always)]
fn guarded<M, E>(
    core: &mut Core<M, E>,
    meta: &mut ActorMeta,
    me: ActorId,
    now: SimTime,
    handler: impl FnOnce(&mut Core<M, E>, &mut ActorMeta),
) {
    if meta.failed {
        core.stats.dropped_failed += 1;
        return;
    }
    if !core.isolate_panics {
        handler(core, meta);
        return;
    }
    if let Err(panic) = catch_unwind(AssertUnwindSafe(|| handler(core, meta))) {
        meta.failed = true;
        let message = panic
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| panic.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "non-string panic payload".to_string());
        core.system.publish(now, SystemEvent::ActorFailed { id: me, message });
    }
}

impl<M: 'static, E: 'static, A: Actor<M, E>> KindStore<M, E> for Arena<A> {
    fn dispatch(
        &mut self,
        index: u32,
        core: &mut Core<M, E>,
        routes: &Routes,
        world: &World,
        key: OrderKey,
        item: Item<M>,
    ) {
        let i = index as usize;
        let me = match &item {
            Item::Message { dst, .. } | Item::Routed { dst, .. } => *dst,
            _ => unreachable!("only messages are dispatched to actors"),
        };
        let actor = &mut self.actors[i];
        guarded(core, &mut self.meta[i], me, key.at, |core, meta| {
            let mut ctx = Ctx {
                now: key.at,
                me,
                meta,
                core,
                routes,
                world,
            };
            match item {
                Item::Message { msg, .. } => actor.on_message(&mut ctx, key.src, msg),
                Item::Routed { edge, msg, .. } => actor.on_route_message(&mut ctx, key.src, edge, msg),
                _ => unreachable!(),
            }
        });
    }

    fn start(&mut self, index: u32, core: &mut Core<M, E>, routes: &Routes, world: &World, now: SimTime, me: ActorId) {
        let i = index as usize;
        let actor = &mut self.actors[i];
        guarded(core, &mut self.meta[i], me, now, |core, meta| {
            let mut ctx = Ctx {
                now,
                me,
                meta,
                core,
                routes,
                world,
            };
            actor.on_start(&mut ctx);
        });
    }

    fn meta_mut(&mut self, index: u32) -> Option<&mut ActorMeta> {
        self.meta.get_mut(index as usize)
    }

    fn shrink_to_fit(&mut self) {
        self.actors.shrink_to_fit();
        self.meta.shrink_to_fit();
    }

    fn memory_bytes(&self) -> usize {
        self.actors.capacity() * size_of::<A>() + self.meta.capacity() * size_of::<ActorMeta>()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Settings each shard needs.
#[derive(Copy, Clone, Debug)]
pub(crate) struct ShardSettings {
    pub(crate) shards: usize,
    pub(crate) trace: bool,
    pub(crate) isolate_panics: bool,
    pub(crate) spatial_cell: f64,
}

/// One shard.
pub(crate) struct Shard<M, E> {
    pub(crate) core: Core<M, E>,
    pub(crate) stores: Vec<Box<dyn KindStore<M, E>>>,
    pub(crate) pending_starts: Vec<ActorId>,
    /// Routes owned by actors on this shard.
    pub(crate) routes: Routes,
    pub(crate) trace: Option<Vec<TraceEntry>>,
    pub(crate) pacing: PacingStats,
    /// Members of each group that live on this shard, indexed by group.
    pub(crate) groups: Vec<Vec<ActorId>>,
    pub(crate) spatial: SpatialIndex,
    /// Time of the latest delivered event.
    pub(crate) last_event: SimTime,
    /// Reused buffer for draining mailboxes.
    spare: Batch<M>,
}

/// The shared state of one multi-shard run.
pub(crate) struct RunShared<'a, M, E> {
    pub(crate) world: &'a World,
    /// `mailboxes[from * n + to]`: cross-shard messages from one shard to another, handed over once per window.
    pub(crate) mailboxes: Vec<Mutex<Batch<M>>>,
    pub(crate) barrier: WindowBarrier,
    pub(crate) clock: &'a ClockHandle,
    pub(crate) control: &'a ControlShared<M, E>,
    pub(crate) stop_epoch: u64,
}

impl<M, E> RunShared<'_, M, E> {
    fn stop_requested(&self) -> bool {
        self.clock.stop_epoch() != self.stop_epoch
    }

    fn should_abort(&self) -> bool {
        self.stop_requested() || self.barrier.is_poisoned()
    }
}

/// What a run asks of each shard.
#[derive(Copy, Clone, Debug)]
pub(crate) struct RunPlan {
    pub(crate) start: SimTime,
    /// Process events before this time.
    pub(crate) end: SimTime,
    pub(crate) lookahead: SimDuration,
    /// Longest window while paced, so shards never run more than this far ahead of the clock.
    pub(crate) max_paced_window: SimDuration,
    pub(crate) spin_threshold: Duration,
    /// Warn (once) when the simulation falls this far behind the clock.
    pub(crate) keep_up_warning: Duration,
}

/// How a shard's part of a run ended.
#[derive(Copy, Clone, Debug)]
pub(crate) struct ShardOutcome {
    /// Everything before this time has been processed by this shard.
    pub(crate) processed: SimTime,
    pub(crate) stopped: bool,
}

/// Decides the next window. Called by whichever shard reaches the barrier last.
fn decide(plan: &RunPlan, clock: &ClockHandle, stop: bool, last_end: SimTime, next: u64, lookahead: u64) -> Decision {
    let lookahead = SimDuration::from_nanos(lookahead);
    let (_, state) = clock.snapshot();
    let paced = !matches!(state, ClockState::Unpaced);
    let mut start = SimTime::from_nanos(next);
    if paced {
        // Follow the clock through idle stretches instead of jumping to the next event, so live input injected
        // meanwhile is processed promptly.
        if let Some(horizon) = state.horizon(Instant::now()) {
            start = start.min(horizon.max(last_end));
        }
    }
    let span = if paced {
        lookahead.min(plan.max_paced_window)
    } else {
        lookahead
    };
    let end = start.saturating_add(span.max(SimDuration::NANOSECOND)).min(plan.end);
    Decision {
        start: start.as_nanos(),
        end: end.as_nanos(),
        lookahead: lookahead.as_nanos(),
        stop,
    }
}

impl<M: Clone + 'static, E: Clone + 'static> Shard<M, E> {
    pub(crate) fn new(
        index: u16,
        queue: Queue<Item<M>>,
        events: EventOut<E>,
        system: Arc<SystemBus>,
        settings: ShardSettings,
    ) -> Self {
        Self {
            core: Core {
                shard: index,
                queue,
                events,
                stats: Stats::default(),
                fields: Vec::new(),
                outbox: (0..settings.shards).map(|_| Vec::new()).collect(),
                outbound_min: SimTime::MAX,
                window_end: SimTime::ZERO,
                lookahead_proposal: SimDuration::MAX,
                system,
                isolate_panics: settings.isolate_panics,
                topology_added: 0,
                topology_pruned: 0,
                last_topology_notice: Instant::now(),
            },
            stores: Vec::new(),
            pending_starts: Vec::new(),
            routes: HashMap::new(),
            trace: settings.trace.then(Vec::new),
            pacing: PacingStats::default(),
            groups: Vec::new(),
            spatial: SpatialIndex::new(settings.spatial_cell),
            last_event: SimTime::ZERO,
            spare: Vec::new(),
        }
    }

    /// The actor with this ID, if it lives on this shard and is of type `A`.
    pub(crate) fn actor_mut<A: Actor<M, E>>(&mut self, world: &World, id: ActorId) -> Option<&mut A> {
        let slot = world.locate(id)?;
        if slot.shard != self.core.shard || world.kind_types[usize::from(slot.kind)] != TypeId::of::<A>() {
            return None;
        }
        let store = self.stores[slot.store as usize].as_any_mut();
        store.downcast_mut::<Arena<A>>()?.actors.get_mut(slot.index as usize)
    }

    pub(crate) fn set_observed(&mut self, world: &World, id: ActorId, observed: bool) -> bool {
        let Some(slot) = world.locate(id).filter(|s| s.shard == self.core.shard) else {
            return false;
        };
        match self.stores[slot.store as usize].meta_mut(slot.index) {
            Some(meta) => {
                meta.observed = observed;
                true
            }
            None => false,
        }
    }

    /// Applies a route change. Returns `true` if the route exists (and, for a prune, the edge was found).
    pub(crate) fn apply_topology(&mut self, world: &World, route: RouteId, op: TopologyOp) -> bool {
        let shard = self.core.shard;
        let Some(r) = self.routes.get_mut(&route) else {
            return false;
        };
        match op {
            TopologyOp::Connect(edge) => {
                if world.locate(edge.target).is_some_and(|t| t.shard != shard) {
                    let delay = edge.delay.saturating_sub(r.jitter.max_reduction());
                    self.core.lookahead_proposal = self.core.lookahead_proposal.min(delay.max(SimDuration::NANOSECOND));
                }
                r.edges.push(edge);
                self.core.topology_added += 1;
                true
            }
            TopologyOp::Prune { target, edge } => {
                match r.edges.iter().position(|e| e.target == target && e.edge == edge) {
                    Some(position) => {
                        r.edges.remove(position);
                        self.core.topology_pruned += 1;
                        true
                    }
                    None => false,
                }
            }
        }
    }

    pub(crate) fn run_starts(&mut self, world: &World, now: SimTime) {
        for id in std::mem::take(&mut self.pending_starts) {
            let slot = world.locate(id).expect("spawned actor is in the directory");
            if let Some(trace) = &mut self.trace {
                trace.push(TraceEntry {
                    dst: id,
                    key: OrderKey {
                        at: now,
                        src: ActorId::COORDINATOR,
                        seq: 0,
                        sub: 0,
                    },
                    kind: DeliveryKind::Start,
                });
            }
            self.stores[slot.store as usize].start(slot.index, &mut self.core, &self.routes, world, now, id);
            self.core.stats.started += 1;
        }
    }

    /// Runs the control commands queued for this shard.
    pub(crate) fn apply_commands(&mut self, control: &ControlShared<M, E>, world: &World, window_start: SimTime) {
        for command in control.take(self.core.shard) {
            command(self, world, window_start);
        }
    }

    pub(crate) fn deliver(&mut self, world: &World, key: OrderKey, item: Item<M>) {
        self.last_event = key.at;
        match item {
            Item::Message { .. } | Item::Routed { .. } => self.deliver_to_actor(world, key, item),
            Item::Broadcast { target, msg } => match *target {
                Target::Group(group) => {
                    let members = self.groups.get(group.0 as usize).cloned().unwrap_or_default();
                    for dst in members {
                        self.deliver_to_actor(world, key, Item::Message { dst, msg: msg.clone() });
                    }
                }
                Target::Region(region) => {
                    let Self { spatial, core, .. } = self;
                    spatial.for_each_within(region.center, region.radius, |dst, distance| {
                        let travel = region.delay_at(distance) - region.base_delay;
                        let at_target = OrderKey {
                            at: key.at + travel,
                            ..key
                        };
                        core.queue.push(at_target, Item::Message { dst, msg: msg.clone() });
                    });
                }
            },
            Item::Field { field, update } => self.core.fields[usize::from(field)].apply_any(key.at, update),
            Item::Topology { route, op } => {
                self.apply_topology(world, route, *op);
            }
        }
    }

    fn deliver_to_actor(&mut self, world: &World, key: OrderKey, item: Item<M>) {
        let (dst, kind) = match &item {
            Item::Message { dst, .. } => (*dst, DeliveryKind::Message),
            Item::Routed { dst, edge, .. } => (*dst, DeliveryKind::Routed(*edge)),
            _ => unreachable!(),
        };
        let Some(slot) = world.locate(dst) else {
            self.core.stats.dropped_unknown += 1;
            return;
        };
        debug_assert_eq!(slot.shard, self.core.shard, "delivered to the wrong shard");
        if let Some(trace) = &mut self.trace {
            trace.push(TraceEntry { dst, key, kind });
        }
        self.stores[slot.store as usize].dispatch(slot.index, &mut self.core, &self.routes, world, key, item);
        self.core.stats.delivered += 1;
        self.core.stats.routed_delivered += u64::from(matches!(kind, DeliveryKind::Routed(_)));
    }

    /// The earliest time this shard knows of: its own next event, or a message it just sent elsewhere.
    fn local_next(&mut self) -> SimTime {
        let next = self.core.queue.peek_key().map_or(SimTime::MAX, |k| k.at);
        next.min(self.core.outbound_min)
    }

    /// Hands this window's cross-shard messages to their destinations' mailboxes.
    fn flush_outbox(&mut self, shared: &RunShared<'_, M, E>, n: usize) {
        let me = usize::from(self.core.shard);
        for (to, outbox) in self.core.outbox.iter_mut().enumerate() {
            if outbox.is_empty() {
                continue;
            }
            let mut mailbox = shared.mailboxes[me * n + to].lock().unwrap_or_else(|p| p.into_inner());
            if mailbox.is_empty() {
                std::mem::swap(&mut *mailbox, outbox);
            } else {
                mailbox.append(outbox);
            }
        }
    }

    /// Moves messages other shards sent here into the local queue.
    fn drain(&mut self, shared: &RunShared<'_, M, E>, n: usize) {
        let me = usize::from(self.core.shard);
        for from in (0..n).filter(|&from| from != me) {
            {
                let mut mailbox = shared.mailboxes[from * n + me]
                    .lock()
                    .unwrap_or_else(|p| p.into_inner());
                if mailbox.is_empty() {
                    continue;
                }
                std::mem::swap(&mut *mailbox, &mut self.spare);
            }
            for (key, item) in self.spare.drain(..) {
                self.core.queue.push(key, item);
            }
        }
    }

    /// End-of-window housekeeping: hand events to subscribers and report route changes (at most every 100 ms).
    pub(crate) fn publish_outputs(&mut self, sim_time: SimTime, force: bool) {
        self.core.events.flush();
        let core = &mut self.core;
        let changed = core.topology_added + core.topology_pruned > 0;
        if changed && (force || core.last_topology_notice.elapsed() >= Duration::from_millis(100)) {
            core.system.publish(
                sim_time,
                SystemEvent::TopologyChanged {
                    shard: core.shard,
                    added: core.topology_added,
                    pruned: core.topology_pruned,
                },
            );
            core.topology_added = 0;
            core.topology_pruned = 0;
            core.last_topology_notice = Instant::now();
        }
    }

    fn arrive(&mut self, shared: &RunShared<'_, M, E>, plan: &RunPlan, last_end: SimTime) -> Decision {
        let next = self.local_next().as_nanos();
        let proposal = self.core.lookahead_proposal.as_nanos();
        let decision = shared.barrier.arrive(next, proposal, |next, lookahead| {
            decide(plan, shared.clock, shared.stop_requested(), last_end, next, lookahead)
        });
        self.core.outbound_min = SimTime::MAX;
        decision
    }

    /// Waits until the clock allows simulation time `t`, applying control commands meanwhile. Returns `false` if
    /// the run is stopped first.
    fn wait_until(
        &mut self,
        clock: &mut ClockView<'_>,
        t: SimTime,
        shared: &RunShared<'_, M, E>,
        plan: &RunPlan,
    ) -> bool {
        let me = self.core.shard;
        loop {
            if wait_for(clock, t, plan.spin_threshold, || {
                shared.should_abort() || shared.control.has_pending(me)
            }) {
                return true;
            }
            if shared.should_abort() {
                return false;
            }
            self.apply_commands(shared.control, shared.world, t);
        }
    }

    /// This shard's part of a run: the window loop. Every shard runs it, in parallel.
    pub(crate) fn work(&mut self, shared: &RunShared<'_, M, E>, plan: &RunPlan, n: usize) -> ShardOutcome {
        let world = shared.world;
        let leader = self.core.shard == 0;
        let mut clock = ClockView::new(shared.clock);
        let mut lookahead = plan.lookahead;
        let mut behind = false;
        let mut last_end = plan.start;

        self.core.lookahead_proposal = lookahead;
        self.core.window_end = plan.start;
        self.apply_commands(shared.control, world, plan.start);
        self.run_starts(world, plan.start);
        self.flush_outbox(shared, n);
        self.publish_outputs(plan.start, false);
        let mut decision = self.arrive(shared, plan, last_end);

        loop {
            if decision.stop {
                self.publish_outputs(last_end, true);
                return ShardOutcome {
                    processed: last_end,
                    stopped: true,
                };
            }
            let agreed = SimDuration::from_nanos(decision.lookahead);
            if leader && agreed < lookahead {
                self.core.system.publish(
                    SimTime::from_nanos(decision.start),
                    SystemEvent::LookaheadChanged {
                        old: lookahead,
                        new: agreed,
                    },
                );
            }
            lookahead = agreed;
            self.core.lookahead_proposal = lookahead;
            self.drain(shared, n);
            let start = SimTime::from_nanos(decision.start);
            self.apply_commands(shared.control, world, start);

            if start >= plan.end {
                // Nothing left before the end. A paced run still waits for the clock to reach the end.
                let reached = plan.end == SimTime::MAX || self.wait_until(&mut clock, plan.end, shared, plan);
                self.publish_outputs(plan.end.min(last_end.max(start)), true);
                return ShardOutcome {
                    processed: if reached && plan.end != SimTime::MAX {
                        plan.end
                    } else {
                        last_end
                    },
                    stopped: !reached,
                };
            }

            // If the run is stopped while waiting, skip this window. Other shards may have processed it; that is
            // safe, since no shard goes beyond it before the next barrier.
            if self.wait_until(&mut clock, start, shared, plan) {
                if leader {
                    shared.clock.publish_progress(start);
                    if let Some(due) = clock.state.wall_for(start) {
                        let lag = Instant::now().saturating_duration_since(due);
                        self.pacing.windows += 1;
                        self.pacing.lateness.record(lag);
                        if !behind && lag > plan.keep_up_warning {
                            behind = true;
                            let time_factor = match clock.state {
                                ClockState::Running { time_factor, .. } => time_factor,
                                _ => 1.0,
                            };
                            self.core
                                .system
                                .publish(start, SystemEvent::CannotKeepUp { lag, time_factor });
                        } else if behind && lag < plan.keep_up_warning / 10 {
                            behind = false;
                            self.core.system.publish(start, SystemEvent::CaughtUp { lag });
                        }
                    }
                }
                let end = SimTime::from_nanos(decision.end);
                self.core.window_end = end;
                while let Some((key, item)) = self.core.queue.pop_before(end) {
                    self.deliver(world, key, item);
                }
                self.core.stats.windows += 1;
                last_end = end;
            }

            self.flush_outbox(shared, n);
            self.publish_outputs(last_end, false);
            decision = self.arrive(shared, plan, last_end);
        }
    }

    /// Moves this shard's outboxes out (for single-threaded stepping, where the runtime delivers them directly).
    pub(crate) fn take_outbox(&mut self) -> Vec<Batch<M>> {
        self.core.outbound_min = SimTime::MAX;
        let n = self.core.outbox.len();
        std::mem::replace(&mut self.core.outbox, (0..n).map(|_| Vec::new()).collect())
    }

    /// The groups vector grown to hold `group`.
    pub(crate) fn group_mut(&mut self, group: GroupId) -> &mut Vec<ActorId> {
        let i = group.0 as usize;
        if self.groups.len() <= i {
            self.groups.resize_with(i + 1, Vec::new);
        }
        &mut self.groups[i]
    }
}
