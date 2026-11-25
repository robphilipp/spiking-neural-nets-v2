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
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tempo_core::{ActorId, OrderKey, SimDuration, SimTime};
use tempo_queue::{HeapQueue, TimeQueue, WheelQueue};

use crate::barrier::{Decision, WindowBarrier};
use crate::broadcast::{GroupId, SpatialIndex, Target};
use crate::clock::{ClockHandle, ClockState, ClockView, wait_for};
use crate::cluster::{ALL_SHARDS, Link, Remote, RemoteItem, WindowDecision, WindowReport};
use crate::control::{ControlShared, TopologyOp};
use crate::ctx::ActorMeta;
use crate::events::EventOut;
use crate::field::AnyField;
use crate::metrics::{PacingStats, Stats};
use crate::route::{Edge, Jitter};
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

/// A shard's slices of routes: for each route that reaches actors on this shard, the edges to them. A route's edges
/// live with their targets, so a send crosses to another shard or node once, not once per edge (see
/// [`crate::Ctx::fanout`]).
pub(crate) type Routes = HashMap<RouteId, Vec<Edge>>;

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
    /// A send along a route, for the slice of it on the receiving shard: each of its edges is delivered at `base`
    /// plus the edge's delay. The key's sender and sequence number are the send's.
    Fanout { route: RouteId, base: SimTime, msg: M },
    /// A change to a route's slice on the receiving shard (forwarded by the route owner's shard).
    Slice { route: RouteId, op: Box<TopologyOp> },
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

/// Where an ID block lives: its node and shard, its actor kind, and which of the shard's stores holds it (on its
/// own node; other nodes keep no store for it).
#[derive(Copy, Clone, Debug)]
pub(crate) struct BlockEntry {
    pub(crate) node: u32,
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

/// Where actors and routes live. Shared (read-only) by shards during a run and by the control plane. Every node of
/// a cluster has the same directory.
#[derive(Clone, Debug, Default)]
pub(crate) struct Directory {
    /// This node.
    pub(crate) node: u32,
    pub(crate) blocks: Vec<Option<BlockEntry>>,
    /// Each route's owner, where it lives, and the route's jitter.
    pub(crate) routes: Vec<RouteHome>,
}

/// A route's owner and where it lives (the same on every node).
#[derive(Copy, Clone, Debug)]
pub(crate) struct RouteHome {
    pub(crate) node: u32,
    pub(crate) shard: u16,
    pub(crate) owner: ActorId,
    pub(crate) jitter: Jitter,
}

/// Where each route reaches, as this node knows it: the shards here with a slice of it, and (for routes owned here)
/// the other nodes with one, and the shortest delay of its edges away from the owner's shard. These only ever grow
/// (or, the delay, shrink) while running, with a new connection, so they're atomics in the otherwise read-only world.
/// At most 64 shards per node and 64 nodes.
#[derive(Debug, Default)]
pub(crate) struct Reach {
    slices: Vec<AtomicU64>,
    nodes: Vec<AtomicU64>,
    /// In ns (`u32::MAX`: none, or longer than a route edge can be).
    elsewhere: Vec<AtomicU32>,
}

impl Reach {
    pub(crate) fn push(&mut self) {
        self.slices.push(AtomicU64::new(0));
        self.nodes.push(AtomicU64::new(0));
        self.elsewhere.push(AtomicU32::new(u32::MAX));
    }

    #[cfg_attr(not(feature = "serde"), allow(dead_code))] // (for restoring snapshots)
    pub(crate) fn clear(&mut self) {
        for bits in self.slices.iter_mut().chain(&mut self.nodes) {
            *bits.get_mut() = 0;
        }
        for delay in &mut self.elsewhere {
            *delay.get_mut() = u32::MAX;
        }
    }

    /// The shortest delay of the route's edges away from its owner's shard: no event of a send reaches another
    /// shard or node sooner.
    #[inline(always)]
    pub(crate) fn elsewhere(&self, route: RouteId) -> SimDuration {
        SimDuration::from_nanos(u64::from(
            self.elsewhere
                .get(route.0 as usize)
                .map_or(0, |d| d.load(Ordering::Relaxed)),
        ))
    }

    pub(crate) fn add_elsewhere(&self, route: RouteId, delay: SimDuration) {
        if let Some(d) = self.elsewhere.get(route.0 as usize) {
            d.fetch_min(u32::try_from(delay.as_nanos()).unwrap_or(u32::MAX), Ordering::Relaxed);
        }
    }

    #[inline(always)]
    pub(crate) fn slices(&self, route: RouteId) -> u64 {
        self.slices
            .get(route.0 as usize)
            .map_or(0, |b| b.load(Ordering::Relaxed))
    }

    #[inline(always)]
    pub(crate) fn nodes(&self, route: RouteId) -> u64 {
        self.nodes
            .get(route.0 as usize)
            .map_or(0, |b| b.load(Ordering::Relaxed))
    }

    pub(crate) fn add_slice(&self, route: RouteId, shard: u16) {
        if let Some(b) = self.slices.get(route.0 as usize) {
            b.fetch_or(1 << shard, Ordering::Relaxed);
        }
    }

    pub(crate) fn add_node(&self, route: RouteId, node: u32) {
        if let Some(b) = self.nodes.get(route.0 as usize) {
            b.fetch_or(1 << node, Ordering::Relaxed);
        }
    }
}

/// The set bits of `bits`, lowest first.
pub(crate) fn bits(mut bits: u64) -> impl Iterator<Item = u32> {
    std::iter::from_fn(move || {
        (bits != 0).then(|| {
            let i = bits.trailing_zeros();
            bits &= bits - 1;
            i
        })
    })
}

/// Where a route change goes after its owner's shard has noted it: the shard holding the slice it changes.
pub(crate) struct Forward {
    pub(crate) node: u32,
    pub(crate) shard: u16,
    pub(crate) op: TopologyOp,
}

impl Directory {
    /// Where an actor lives on this node, or `None` if it doesn't exist or lives on another node.
    #[inline(always)]
    pub(crate) fn locate(&self, id: ActorId) -> Option<Slot> {
        let entry = self.blocks.get(id.block().raw() as usize)?.as_ref()?;
        let index = u32::from(id.index_in_block());
        (index < entry.count && entry.node == self.node).then_some(Slot {
            shard: entry.shard,
            kind: entry.kind,
            store: entry.store,
            index,
        })
    }

    /// The node and shard an actor lives on, wherever that is.
    #[inline(always)]
    pub(crate) fn home(&self, id: ActorId) -> Option<(u32, u16)> {
        let entry = self.blocks.get(id.block().raw() as usize)?.as_ref()?;
        (u32::from(id.index_in_block()) < entry.count).then_some((entry.node, entry.shard))
    }

    /// The shard owning a route, if it's on this node.
    pub(crate) fn route_shard(&self, route: RouteId) -> Option<u16> {
        self.routes
            .get(route.0 as usize)
            .filter(|home| home.node == self.node)
            .map(|home| home.shard)
    }
}

/// Everything shards share read-only while running.
pub(crate) struct World {
    pub(crate) dir: Arc<Directory>,
    pub(crate) kind_types: Vec<TypeId>,
    pub(crate) field_types: Vec<TypeId>,
    /// Shards on this node.
    pub(crate) shards: usize,
    /// Nodes in the cluster (1 without one).
    pub(crate) nodes: u32,
    pub(crate) seed: u64,
    /// Where each route reaches.
    pub(crate) reach: Reach,
}

impl World {
    #[inline(always)]
    pub(crate) fn locate(&self, id: ActorId) -> Option<Slot> {
        self.dir.locate(id)
    }
}

/// A shard's mutable state that handlers reach through [`Ctx`].
pub(crate) struct Core<M, E> {
    pub(crate) node: u32,
    pub(crate) shard: u16,
    pub(crate) queue: Queue<Item<M>>,
    pub(crate) events: EventOut<E>,
    pub(crate) stats: Stats,
    /// This shard's copy of each field, indexed like `World::field_types`.
    pub(crate) fields: Vec<Box<dyn AnyField>>,
    /// Cross-shard messages produced this window, per destination shard.
    pub(crate) outbox: Vec<Batch<M>>,
    /// Messages for other nodes produced this window (or while idle), per destination node.
    pub(crate) remote: Vec<Vec<Remote<M>>>,
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
        let home = match &item {
            Item::Message { dst, .. } | Item::Routed { dst, .. } => world.dir.home(*dst),
            Item::Topology { route, .. } => world.dir.routes.get(route.0 as usize).map(|h| (h.node, h.shard)),
            Item::Broadcast { .. } | Item::Field { .. } | Item::Fanout { .. } | Item::Slice { .. } => {
                unreachable!("sent to a chosen shard")
            }
        };
        match home {
            Some((node, shard)) if node == self.node => self.send_to_shard(shard, key, item),
            Some((node, shard)) => self.send_to_node(node, shard, key, item),
            None => self.stats.dropped_unknown += 1,
        }
    }

    /// Queues a broadcast or field update for every shard of every node; `make` builds each copy.
    pub(crate) fn send_everywhere(&mut self, world: &World, key: OrderKey, mut make: impl FnMut() -> Item<M>) {
        for shard in 0..world.shards {
            self.send_to_shard(shard as u16, key, make());
        }
        let me = self.node;
        for node in (0..world.nodes).filter(|&n| n != me) {
            self.send_to_node(node, ALL_SHARDS, key, make());
        }
    }

    /// Queues an event for a shard of another node (or [`ALL_SHARDS`]). It leaves at the end of the window.
    pub(crate) fn send_to_node(&mut self, node: u32, shard: u16, mut key: OrderKey, item: Item<M>) {
        if key.at < self.window_end {
            // As for cross-shard messages: the receiver may be past this time already.
            if !matches!(item, Item::Topology { .. } | Item::Slice { .. }) && key.src != ActorId::WORLD {
                let error = self.window_end - key.at;
                self.stats.lookahead_violations += 1;
                self.stats.max_lookahead_violation = self.stats.max_lookahead_violation.max(error);
            }
            key.at = self.window_end;
        }
        let item = match item {
            Item::Message { dst, msg } => RemoteItem::Message { dst, msg },
            Item::Routed { dst, edge, msg } => RemoteItem::Routed { dst, edge, msg },
            Item::Broadcast { target, msg } => RemoteItem::Broadcast { target: *target, msg },
            Item::Field { field, update } => RemoteItem::Field { field, update },
            Item::Topology { route, op } => RemoteItem::Topology { route, op: *op },
            Item::Fanout { route, base, msg } => RemoteItem::Fanout { route, base, msg },
            Item::Slice { route, op } => RemoteItem::Slice { route, op: *op },
        };
        self.outbound_min = self.outbound_min.min(key.at);
        self.stats.remote_sends += 1;
        self.stats.node_sends += 1;
        self.remote[node as usize].push(Remote { shard, key, item });
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
            if !matches!(item, Item::Topology { .. } | Item::Slice { .. }) {
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
    pub(crate) node: u32,
    pub(crate) nodes: u32,
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
    /// Wall time spent waiting for the other shards (and nodes) at window ends.
    pub(crate) sync_wait: std::time::Duration,
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
    /// The cluster, if this runtime is one node of several.
    pub(crate) link: Option<&'a dyn Link<M>>,
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
                node: settings.node,
                shard: index,
                queue,
                events,
                stats: Stats::default(),
                fields: Vec::new(),
                outbox: (0..settings.shards).map(|_| Vec::new()).collect(),
                remote: (0..settings.nodes).map(|_| Vec::new()).collect(),
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
            sync_wait: std::time::Duration::ZERO,
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

    /// Notes a route change on the route owner's shard: where the route now reaches. Returns whether the route is
    /// owned here (and its target exists), and where the change itself goes: the slice of the target's shard.
    pub(crate) fn note_topology(&mut self, world: &World, route: RouteId, op: TopologyOp) -> (bool, Option<Forward>) {
        let me = (world.dir.node, self.core.shard);
        let owned = world
            .dir
            .routes
            .get(route.0 as usize)
            .is_some_and(|h| (h.node, h.shard) == me);
        let target = match &op {
            TopologyOp::Connect(edge) => edge.target,
            TopologyOp::Prune { target, .. } => *target,
        };
        let (Some((node, shard)), true) = (world.dir.home(target), owned) else {
            return (false, None);
        };
        if let TopologyOp::Connect(edge) = &op {
            if node == world.dir.node {
                world.reach.add_slice(route, shard);
            } else {
                world.reach.add_node(route, node);
            }
            if (node, shard) != me {
                world.reach.add_elsewhere(route, edge.delay);
            }
        }
        (true, Some(Forward { node, shard, op }))
    }

    /// Applies a route change to this shard's slice of the route. Returns `true` if it was applied (for a prune:
    /// the edge was found).
    pub(crate) fn apply_slice(&mut self, world: &World, route: RouteId, op: TopologyOp) -> bool {
        let shard = self.core.shard;
        let Some(home) = world.dir.routes.get(route.0 as usize) else {
            return false;
        };
        match op {
            TopologyOp::Connect(edge) => {
                if (home.node, home.shard) != (world.dir.node, shard) {
                    // From another shard: the edge crosses shards, and may shorten the lookahead.
                    let delay = edge.delay.saturating_sub(home.jitter.max_reduction());
                    self.core.lookahead_proposal = self.core.lookahead_proposal.min(delay.max(SimDuration::NANOSECOND));
                }
                world.reach.add_slice(route, shard);
                self.routes.entry(route).or_default().push(Edge::new(edge));
                self.core.topology_added += 1;
                true
            }
            TopologyOp::Prune { target, edge } => {
                let Some(edges) = self.routes.get_mut(&route) else {
                    return false;
                };
                match edges.iter().position(|e| e.target == target && e.edge == edge) {
                    Some(position) => {
                        edges.remove(position);
                        self.core.topology_pruned += 1;
                        true
                    }
                    None => false,
                }
            }
        }
    }

    /// A route change on the owner's shard, during a run: noted here, then applied to the target's slice (here, or
    /// sent on to its shard). Returns whether the route is owned here (and, if the slice is here too, whether the
    /// change applied).
    pub(crate) fn change_route_now(&mut self, world: &World, key: OrderKey, route: RouteId, op: TopologyOp) -> bool {
        let (owned, forward) = self.note_topology(world, route, op);
        match forward {
            Some(f) if f.node == world.dir.node && f.shard == self.core.shard => self.apply_slice(world, route, f.op),
            Some(f) => {
                let item = Item::Slice {
                    route,
                    op: Box::new(f.op),
                };
                if f.node == world.dir.node {
                    self.core.send_to_shard(f.shard, key, item);
                } else {
                    self.core.send_to_node(f.node, f.shard, key, item);
                }
                owned
            }
            None => owned,
        }
    }

    /// Delivers a send along a route to this shard's slice of it: each edge's event, at `base` plus the edge's
    /// delay (with jitter), keyed by the send's sender and sequence number and the receiver's edge index, exactly as
    /// if the sender had queued them itself.
    fn expand(&mut self, world: &World, key: OrderKey, route: RouteId, base: SimTime, msg: &M) {
        let Some(edges) = self.routes.get(&route) else {
            return; // (sent to every shard of this node: not all have a slice)
        };
        let jitter = world
            .dir
            .routes
            .get(route.0 as usize)
            .map_or(Jitter::None, |h| h.jitter);
        for e in edges {
            let delay = jitter.apply(e.delay(), world.seed, key.src, key.seq, e.target, e.edge.0);
            let mut at = base + delay;
            if at < key.at {
                // Shorter than the window it was sent in (a connection added during a run, or jitter beyond its
                // allowance): as for any cross-shard message, delivered as soon as it can be, and counted.
                self.core.stats.lookahead_violations += 1;
                self.core.stats.max_lookahead_violation = self.core.stats.max_lookahead_violation.max(key.at - at);
                at = key.at;
            }
            let event = OrderKey {
                at,
                src: key.src,
                seq: key.seq,
                sub: e.edge.0,
            };
            self.core.queue.push(
                event,
                Item::Routed {
                    dst: e.target,
                    edge: e.edge,
                    msg: msg.clone(),
                },
            );
        }
        self.core.stats.routed_sends += edges.len() as u64;
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
                self.change_route_now(world, key, route, *op);
            }
            Item::Fanout { route, base, msg } => self.expand(world, key, route, base, &msg),
            Item::Slice { route, op } => {
                self.apply_slice(world, route, *op);
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

    /// Hands this window's cross-shard messages to their destinations' mailboxes, and other nodes' to the link.
    fn flush_outbox(&mut self, shared: &RunShared<'_, M, E>, n: usize) {
        if let Some(link) = shared.link {
            for (node, events) in self.core.remote.iter_mut().enumerate() {
                if !events.is_empty() {
                    link.send(node as u32, std::mem::take(events));
                }
            }
        }
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

    /// Moves events other nodes sent here into the local queue (see [`Self::drain`]).
    fn drain_link(&mut self, shared: &RunShared<'_, M, E>, start: SimTime) {
        // Never before an event already delivered here (after a best-effort run, nodes may have stopped at
        // slightly different times).
        let start = start.max(self.last_event);
        if let Some(link) = shared.link {
            let core = &mut self.core;
            link.receive(core.shard, &mut |mut key, item| {
                let mut late = SimDuration::ZERO;
                if key.at < start {
                    late = start - key.at;
                    core.stats.late_remote += 1;
                    core.stats.max_remote_lateness = core.stats.max_remote_lateness.max(late);
                    key.at = start;
                }
                let item = match item {
                    RemoteItem::Message { dst, msg } => Item::Message { dst, msg },
                    RemoteItem::Routed { dst, edge, msg } => Item::Routed { dst, edge, msg },
                    RemoteItem::Broadcast { target, msg } => Item::Broadcast {
                        target: Box::new(target),
                        msg,
                    },
                    RemoteItem::Field { field, update } => Item::Field { field, update },
                    RemoteItem::Topology { route, op } => Item::Topology {
                        route,
                        op: Box::new(op),
                    },
                    RemoteItem::Fanout { route, base, msg } => Item::Fanout { route, base, msg },
                    RemoteItem::Slice { route, op } => Item::Slice {
                        route,
                        op: Box::new(op),
                    },
                };
                core.stats.node_receives += 1;
                core.queue.push(key, item);
                late
            });
        }
    }

    /// Moves messages other shards (and nodes) sent here into the local queue. Nothing goes in earlier than
    /// `start`, the window about to be processed: a message from another node that arrives too late for its time
    /// is delivered at `start` instead, and counted.
    fn drain(&mut self, shared: &RunShared<'_, M, E>, n: usize, start: SimTime) {
        self.drain_link(shared, start);
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
        let me = self.core.shard;
        // In a cluster, waiting at the barrier can take a while (a paused cluster waits here): keep applying control
        // commands, at the end of the window just processed. (A single node applies them at the next window start.)
        let mut idle = || {
            if shared.link.is_some() && shared.control.has_pending(me) {
                self.apply_commands(shared.control, shared.world, last_end);
            }
        };
        let waiting = std::time::Instant::now();
        let decision = shared
            .barrier
            .arrive(next, proposal, &mut idle, |next, lookahead, idle| match shared.link {
                None => decide(plan, shared.clock, shared.stop_requested(), last_end, next, lookahead),
                Some(link) => {
                    let report = WindowReport {
                        next: SimTime::from_nanos(next),
                        lookahead: SimDuration::from_nanos(lookahead),
                        stop: shared.stop_requested(),
                    };
                    let decide_all = |all: WindowReport| {
                        let d = decide(
                            plan,
                            shared.clock,
                            all.stop,
                            last_end,
                            all.next.as_nanos(),
                            all.lookahead.as_nanos(),
                        );
                        WindowDecision {
                            start: SimTime::from_nanos(d.start),
                            end: SimTime::from_nanos(d.end),
                            lookahead: SimDuration::from_nanos(d.lookahead),
                            stop: d.stop,
                        }
                    };
                    let d = link.decide(report, &decide_all, idle);
                    Decision {
                        start: d.start.as_nanos(),
                        end: d.end.as_nanos(),
                        lookahead: d.lookahead.as_nanos(),
                        stop: d.stop,
                    }
                }
            });
        self.core.outbound_min = SimTime::MAX;
        self.sync_wait += waiting.elapsed();
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
        // On a cluster each node starts from its own lookahead; the first window's agreement (every node's edges
        // count) sets where the run starts, and changes are reported from then on. (A change that comes with the
        // run's first commands is then not reported on a cluster.)
        let mut agreed_once = shared.link.is_none();
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
            // The lookahead is agreed by the whole cluster: report its change once (from the first node).
            if leader && self.core.node == 0 && agreed_once && agreed < lookahead {
                self.core.system.publish(
                    SimTime::from_nanos(decision.start),
                    SystemEvent::LookaheadChanged {
                        old: lookahead,
                        new: agreed,
                    },
                );
            }
            lookahead = agreed;
            agreed_once = true;
            self.core.lookahead_proposal = lookahead;
            let start = SimTime::from_nanos(decision.start);
            self.drain(shared, n, start.min(plan.end));
            self.apply_commands(shared.control, world, start);

            if start >= plan.end {
                // Nothing left before the end. A paced run still waits for the clock to reach the end.
                // (A cluster node treats the end as reached either way: the others may not have waited.)
                let reached = plan.end == SimTime::MAX
                    || self.wait_until(&mut clock, plan.end, shared, plan)
                    || shared.link.is_some();
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
            // safe, since no shard goes beyond it before the next barrier. A cluster node processes it anyway, so
            // every node stops at the same time (the stop takes effect at the next barrier).
            if self.wait_until(&mut clock, start, shared, plan) || shared.link.is_some() {
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
                // Events from other nodes that arrived while waiting for the clock (best effort: that wait is
                // their slack) go in before the window is processed.
                if shared.link.is_some() {
                    self.drain_link(shared, start);
                }
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
