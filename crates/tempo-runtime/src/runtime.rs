//! The single-threaded executor. In phase 2 this becomes the event loop of one shard.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::fmt;

use rand_core::SeedableRng;
use tempo_core::{ActorId, BlockId, OrderKey, SeqCounter, SimDuration, SimTime};
use tempo_queue::{HeapQueue, TimeQueue, WheelQueue};

use crate::ctx::ActorMeta;
use crate::events::{EventCallback, Events};
use crate::route::{Route, splitmix};
use crate::{Actor, ActorRng, Ctx, EdgeIdx, Emitted, Jitter, RouteEdge, RouteId};

/// Which [`TimeQueue`] implementation holds pending events.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum QueueKind {
    /// Binary heap: O(log n). Fine for small simulations.
    Heap,
    /// Hierarchical timing wheel with the given bucket width: about O(1). The default, with 1 µs buckets.
    Wheel {
        /// Bucket width. Ordering is exact for any width; it only affects speed.
        bucket: SimDuration,
    },
}

/// Runtime settings.
#[derive(Clone, Debug)]
pub struct RuntimeConfig {
    /// Seed for every actor's random-number stream and for route jitter.
    pub seed: u64,
    /// Event queue implementation.
    pub queue: QueueKind,
    /// Record every delivery in a trace (for determinism tests; costs memory).
    pub trace: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            seed: 0,
            queue: QueueKind::Wheel {
                bucket: SimDuration::MICROSECOND,
            },
            trace: false,
        }
    }
}

/// Counters describing what the runtime has done.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// Messages delivered to actors (direct and routed).
    pub delivered: u64,
    /// Of those, messages delivered through routes.
    pub routed_delivered: u64,
    /// Route edges sent (each fan-out send counts once per edge).
    pub routed_sends: u64,
    /// Actors started.
    pub started: u64,
    /// Messages dropped because the recipient doesn't exist.
    pub dropped_unknown: u64,
}

/// What kind of delivery a [`TraceEntry`] records.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum DeliveryKind {
    /// The actor's `on_start`.
    Start,
    /// A direct message.
    Message,
    /// A routed message, with the receiver's edge index.
    Routed(EdgeIdx),
}

/// One delivery, as recorded when [`RuntimeConfig::trace`] is on.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct TraceEntry {
    /// The receiving actor.
    pub dst: ActorId,
    /// The event's order key (its time, sender, sequence and sub-index).
    pub key: OrderKey,
    /// What was delivered.
    pub kind: DeliveryKind,
}

/// A queued delivery.
pub(crate) enum Item<M> {
    Message { dst: ActorId, msg: M },
    Routed { dst: ActorId, edge: EdgeIdx, msg: M },
}

impl<M> Item<M> {
    fn dst(&self) -> ActorId {
        match self {
            Self::Message { dst, .. } | Self::Routed { dst, .. } => *dst,
        }
    }
}

/// The configured queue, as an enum so dispatch is a `match` rather than a virtual call.
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

/// State that handlers reach through [`Ctx`].
pub(crate) struct Core<M, E> {
    pub(crate) queue: Queue<Item<M>>,
    pub(crate) routes: Vec<Route>,
    pub(crate) seed: u64,
    pub(crate) events: Events<E>,
    pub(crate) stats: Stats,
}

/// Type-erased storage for all actors of one Rust type ("kind"). Dispatch into the arena is one virtual call;
/// inside it, the actor's handlers are called statically.
trait KindStore<M, E>: Send {
    fn dispatch(&mut self, index: u32, core: &mut Core<M, E>, key: OrderKey, item: Item<M>);
    fn start(&mut self, index: u32, core: &mut Core<M, E>, now: SimTime, me: ActorId);
    fn memory_bytes(&self) -> usize;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

struct Arena<A> {
    actors: Vec<A>,
    meta: Vec<ActorMeta>,
}

impl<M: 'static, E: 'static, A: Actor<M, E>> KindStore<M, E> for Arena<A> {
    fn dispatch(&mut self, index: u32, core: &mut Core<M, E>, key: OrderKey, item: Item<M>) {
        let i = index as usize;
        let mut ctx = Ctx {
            now: key.at,
            me: item.dst(),
            meta: &mut self.meta[i],
            core,
        };
        let actor = &mut self.actors[i];
        match item {
            Item::Message { msg, .. } => actor.on_message(&mut ctx, key.src, msg),
            Item::Routed { edge, msg, .. } => actor.on_route_message(&mut ctx, key.src, edge, msg),
        }
    }

    fn start(&mut self, index: u32, core: &mut Core<M, E>, now: SimTime, me: ActorId) {
        let i = index as usize;
        let mut ctx = Ctx {
            now,
            me,
            meta: &mut self.meta[i],
            core,
        };
        self.actors[i].on_start(&mut ctx);
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

/// Where a block's actors live locally: their kind, and the arena index of the block's first ID.
#[derive(Copy, Clone, Debug)]
struct BlockEntry {
    kind: u16,
    base: u32,
    count: u32,
}

/// A single-threaded, as-fast-as-possible executor of timed actors.
///
/// Events are processed strictly in [`OrderKey`] order, so a run is fully determined by the seed, the actors and
/// the injected messages.
pub struct Runtime<M, E> {
    core: Core<M, E>,
    kinds: Vec<Box<dyn KindStore<M, E>>>,
    kind_types: Vec<TypeId>,
    kind_ids: HashMap<TypeId, u16>,
    /// Per kind: the block currently being filled and the next index in it.
    open_blocks: Vec<Option<(BlockId, u32)>>,
    /// The directory: block number → where its actors live.
    blocks: Vec<Option<BlockEntry>>,
    pending_starts: Vec<ActorId>,
    /// Everything before this time has been processed.
    now: SimTime,
    world_seq: SeqCounter,
    trace: Option<Vec<TraceEntry>>,
}

impl<M: 'static, E: 'static> Runtime<M, E> {
    /// A runtime with the given settings. Emitted events are discarded until you choose an output with
    /// [`Self::buffer_events`] or [`Self::on_event`].
    pub fn new(config: RuntimeConfig) -> Self {
        let queue = match config.queue {
            QueueKind::Heap => Queue::Heap(HeapQueue::new()),
            QueueKind::Wheel { bucket } => Queue::Wheel(WheelQueue::with_bucket(bucket)),
        };
        Self {
            core: Core {
                queue,
                routes: Vec::new(),
                seed: config.seed,
                events: Events::Discard,
                stats: Stats::default(),
            },
            kinds: Vec::new(),
            kind_types: Vec::new(),
            kind_ids: HashMap::new(),
            open_blocks: Vec::new(),
            blocks: Vec::new(),
            pending_starts: Vec::new(),
            now: SimTime::ZERO,
            world_seq: SeqCounter::new(),
            trace: config.trace.then(Vec::new),
        }
    }

    /// Collects emitted events in memory; read them with [`Self::take_events`].
    pub fn buffer_events(&mut self) {
        self.core.events = Events::Buffer(Vec::new());
    }

    /// Passes each emitted event to `callback`.
    pub fn on_event(&mut self, callback: EventCallback<E>) {
        self.core.events = Events::Callback(callback);
    }

    /// Discards emitted events (the default).
    pub fn discard_events(&mut self) {
        self.core.events = Events::Discard;
    }

    /// Returns and clears the buffered events (empty unless [`Self::buffer_events`] is on).
    pub fn take_events(&mut self) -> Vec<Emitted<E>> {
        match &mut self.core.events {
            Events::Buffer(buffer) => std::mem::take(buffer),
            _ => Vec::new(),
        }
    }

    /// Adds an actor and returns its ID. Its `on_start` runs at the current time, before the next delivery.
    pub fn spawn<A: Actor<M, E>>(&mut self, actor: A) -> ActorId {
        let kind = self.kind_of::<A>();
        let k = usize::from(kind);

        let (block, index) = match self.open_blocks[k] {
            Some((block, next)) if u64::from(next) < ActorId::BLOCK_SIZE => (block, next),
            _ => {
                let block = BlockId::new(self.blocks.len() as u64);
                assert!(!block.is_reserved(), "actor IDs exhausted");
                let base = self.arena_mut::<A>(kind).actors.len();
                self.blocks.push(Some(BlockEntry {
                    kind,
                    base: u32::try_from(base).expect("too many actors of one kind"),
                    count: 0,
                }));
                (block, 0)
            }
        };
        self.open_blocks[k] = Some((block, index + 1));
        if let Some(entry) = self.blocks[block.raw() as usize].as_mut() {
            entry.count += 1;
        }

        let id = ActorId::from_parts(block, index as u16);
        let rng = ActorRng::seed_from_u64(splitmix(self.core.seed) ^ id.raw());
        let arena = self.arena_mut::<A>(kind);
        arena.actors.push(actor);
        arena.meta.push(ActorMeta {
            seq: SeqCounter::new(),
            rng,
        });
        self.pending_starts.push(id);
        id
    }

    /// Reserves room for `additional` more actors of type `A`, so spawning a known number of actors doesn't
    /// over-allocate (a growing `Vec` can end up with up to twice the room it needs).
    pub fn reserve<A: Actor<M, E>>(&mut self, additional: usize) {
        let kind = self.kind_of::<A>();
        let arena = self.arena_mut::<A>(kind);
        arena.actors.reserve_exact(additional);
        arena.meta.reserve_exact(additional);
        self.blocks.reserve(additional.div_ceil(ActorId::BLOCK_SIZE as usize));
    }

    /// Adds a fan-out route owned by `owner`, which can then send along it with [`Ctx::fanout`].
    pub fn add_route(&mut self, owner: ActorId, edges: Vec<RouteEdge>, jitter: Jitter) -> RouteId {
        let id = RouteId(u32::try_from(self.core.routes.len()).expect("too many routes"));
        self.core.routes.push(Route { owner, edges, jitter });
        id
    }

    /// Sends `msg` from the outside world ([`ActorId::WORLD`]) to `to`, delivered at `at`.
    ///
    /// # Panics
    /// If `at` is earlier than [`Self::now`] (that time has already been processed).
    pub fn inject(&mut self, to: ActorId, at: SimTime, msg: M) {
        assert!(at >= self.now, "cannot inject into the past: {at} < {}", self.now);
        let key = OrderKey {
            at,
            src: ActorId::WORLD,
            seq: self.world_seq.next_seq(),
            sub: 0,
        };
        self.core.queue.push(key, Item::Message { dst: to, msg });
    }

    /// The simulation time processed so far: everything before it has been delivered.
    pub fn now(&self) -> SimTime {
        self.now
    }

    /// The time of the next pending event, if any.
    pub fn next_event_time(&mut self) -> Option<SimTime> {
        self.core.queue.peek_key().map(|k| k.at)
    }

    /// Number of pending events.
    pub fn pending(&self) -> usize {
        self.core.queue.len()
    }

    /// Delivers the single next event and returns its time, or `None` if nothing is pending.
    pub fn step(&mut self) -> Option<SimTime> {
        self.run_starts();
        let (key, item) = self.core.queue.pop()?;
        self.deliver(key, item);
        Some(key.at)
    }

    /// Delivers every event before `end`, then sets [`Self::now`] to `end`. Returns the number delivered.
    pub fn run_until(&mut self, end: SimTime) -> u64 {
        self.run_starts();
        let mut delivered = 0;
        while let Some((key, item)) = self.core.queue.pop_before(end) {
            self.deliver(key, item);
            delivered += 1;
        }
        self.now = self.now.max(end);
        delivered
    }

    /// Delivers events until none are pending. Returns the number delivered.
    pub fn run(&mut self) -> u64 {
        self.run_starts();
        let mut delivered = 0;
        while let Some((key, item)) = self.core.queue.pop() {
            self.deliver(key, item);
            delivered += 1;
        }
        delivered
    }

    /// Counters for this runtime.
    pub fn stats(&self) -> &Stats {
        &self.core.stats
    }

    /// Returns and clears the delivery trace (empty unless [`RuntimeConfig::trace`] is on).
    pub fn take_trace(&mut self) -> Vec<TraceEntry> {
        self.trace.as_mut().map(std::mem::take).unwrap_or_default()
    }

    /// The actor with this ID, if it exists and is of type `A`.
    pub fn actor<A: Actor<M, E>>(&self, id: ActorId) -> Option<&A> {
        let (kind, index) = self.locate(id)?;
        if self.kind_types[usize::from(kind)] != TypeId::of::<A>() {
            return None;
        }
        let arena = self.kinds[usize::from(kind)].as_any().downcast_ref::<Arena<A>>()?;
        arena.actors.get(index as usize)
    }

    /// Mutable access to the actor with this ID, if it exists and is of type `A`. For setup and inspection between
    /// runs, e.g. giving an actor the route it should fan out on.
    pub fn actor_mut<A: Actor<M, E>>(&mut self, id: ActorId) -> Option<&mut A> {
        let (kind, index) = self.locate(id)?;
        if self.kind_types[usize::from(kind)] != TypeId::of::<A>() {
            return None;
        }
        let arena = self.kinds[usize::from(kind)].as_any_mut().downcast_mut::<Arena<A>>()?;
        arena.actors.get_mut(index as usize)
    }

    /// Approximate memory held by actors, their framework state, the directory and routes, in bytes.
    /// Excludes pending events.
    pub fn memory_bytes(&self) -> usize {
        let arenas: usize = self.kinds.iter().map(|k| k.memory_bytes()).sum();
        let directory = self.blocks.capacity() * size_of::<Option<BlockEntry>>();
        let routes: usize = self
            .core
            .routes
            .iter()
            .map(|r| size_of::<Route>() + r.edges.capacity() * size_of::<RouteEdge>())
            .sum();
        arenas + directory + routes
    }

    fn kind_of<A: Actor<M, E>>(&mut self) -> u16 {
        let type_id = TypeId::of::<A>();
        if let Some(&kind) = self.kind_ids.get(&type_id) {
            return kind;
        }
        let kind = u16::try_from(self.kinds.len()).expect("too many actor kinds");
        self.kinds.push(Box::new(Arena::<A> {
            actors: Vec::new(),
            meta: Vec::new(),
        }));
        self.kind_types.push(type_id);
        self.kind_ids.insert(type_id, kind);
        self.open_blocks.push(None);
        kind
    }

    fn arena_mut<A: Actor<M, E>>(&mut self, kind: u16) -> &mut Arena<A> {
        self.kinds[usize::from(kind)]
            .as_any_mut()
            .downcast_mut::<Arena<A>>()
            .expect("kind table matches arena type")
    }

    /// Finds an actor's kind and arena index.
    fn locate(&self, id: ActorId) -> Option<(u16, u32)> {
        let entry = self.blocks.get(id.block().raw() as usize)?.as_ref()?;
        let index = u32::from(id.index_in_block());
        (index < entry.count).then_some((entry.kind, entry.base + index))
    }

    fn run_starts(&mut self) {
        if self.pending_starts.is_empty() {
            return;
        }
        for id in std::mem::take(&mut self.pending_starts) {
            let (kind, index) = self.locate(id).expect("spawned actor is in the directory");
            if let Some(trace) = &mut self.trace {
                trace.push(TraceEntry {
                    dst: id,
                    key: OrderKey {
                        at: self.now,
                        src: ActorId::COORDINATOR,
                        seq: 0,
                        sub: 0,
                    },
                    kind: DeliveryKind::Start,
                });
            }
            self.kinds[usize::from(kind)].start(index, &mut self.core, self.now, id);
            self.core.stats.started += 1;
        }
    }

    fn deliver(&mut self, key: OrderKey, item: Item<M>) {
        self.now = key.at;
        let dst = item.dst();
        let Some((kind, index)) = self.locate(dst) else {
            self.core.stats.dropped_unknown += 1;
            return;
        };
        let routed = matches!(item, Item::Routed { .. });
        if let Some(trace) = &mut self.trace {
            trace.push(TraceEntry {
                dst,
                key,
                kind: match &item {
                    Item::Message { .. } => DeliveryKind::Message,
                    Item::Routed { edge, .. } => DeliveryKind::Routed(*edge),
                },
            });
        }
        self.kinds[usize::from(kind)].dispatch(index, &mut self.core, key, item);
        self.core.stats.delivered += 1;
        self.core.stats.routed_delivered += u64::from(routed);
    }
}

impl<M, E> fmt::Debug for Runtime<M, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Runtime")
            .field("now", &self.now)
            .field("pending", &self.core.queue.len())
            .field("kinds", &self.kinds.len())
            .field("stats", &self.core.stats)
            .finish_non_exhaustive()
    }
}
