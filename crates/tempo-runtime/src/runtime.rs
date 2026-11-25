//! The runtime: owns the shards, builds the network, and runs it.

use crate::StopToken;
use std::any::TypeId;
use std::collections::HashMap;
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rand_core::SeedableRng;
use tempo_core::{ActorId, BlockId, OrderKey, SimDuration, SimTime};
use tempo_queue::{HeapQueue, TimeQueue, WheelQueue};

use crate::barrier::WindowBarrier;
use crate::broadcast::GroupId;
use crate::clock::{ClockHandle, Pacing};
use crate::cluster::{Link, NodePlacement};
use crate::control::{Control, ControlShared, Injected, TopologyOp};
use crate::ctx::ActorMeta;
use crate::events::{Emitted, EventOut};
use crate::field::Field;
use crate::metrics::{PacingStats, Stats};
use crate::placement::Partition;
use crate::route::{Route, splitmix};
use crate::shard::{
    Arena, BlockEntry, Directory, Item, Queue, RunPlan, RunShared, Shard, ShardOutcome, ShardSettings, TraceEntry,
    World,
};
use crate::subscribe::{EventFilter, EventStream, Subscribers};
use crate::system::{SystemBus, SystemEvents, SystemNotice};
use crate::{Actor, ActorRng, Jitter, RouteEdge, RouteId};

/// Which [`TimeQueue`] implementation holds each shard's pending events.
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
    /// Number of shards (threads). Each runs a share of the partitions.
    pub shards: usize,
    /// Number of partitions (placement groups). Must be at least `shards`. Keep it fixed for a network so its
    /// results don't depend on the number of shards.
    pub partitions: u32,
    /// Initial pacing; change it while running through [`Runtime::clock`].
    pub pacing: Pacing,
    /// The shortest delay of a *direct* message (`send`, `send_after`, `send_at`, broadcasts, field updates)
    /// between actors on different shards, if any are sent. Route delays are known in advance; direct sends are
    /// not, so declare their minimum here. It bounds the lookahead, and therefore the window length.
    pub min_remote_delay: Option<SimDuration>,
    /// While paced, windows are at most this long, so no shard gets further than this ahead of the clock.
    pub max_paced_window: SimDuration,
    /// While waiting for the clock, spin (rather than sleep) for the last this-much wall time.
    pub spin_threshold: Duration,
    /// Post a [`crate::SystemEvent::CannotKeepUp`] warning when a paced run falls this far behind the clock.
    pub keep_up_warning: Duration,
    /// Live input ([`Control::inject_live`]) is delivered this long after the clock's current simulation time.
    pub ingress_delay: SimDuration,
    /// Record messages injected through [`Control`] for replay ([`Runtime::take_journal`]).
    pub journal: bool,
    /// If an actor's handler panics, disable that actor and report it (a [`crate::SystemEvent::ActorFailed`])
    /// instead of ending the run.
    pub isolate_panics: bool,
    /// Grid cell size of the spatial index used by spatial broadcasts, in position units.
    pub spatial_cell: f64,
    /// This runtime is one node of a cluster (see [`crate::cluster`]); its `shards` must match its entry. Install
    /// the connection to the other nodes with [`Runtime::set_link`] before running.
    pub cluster: Option<NodePlacement>,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            seed: 0,
            queue: QueueKind::Wheel {
                bucket: SimDuration::MICROSECOND,
            },
            trace: false,
            shards: 1,
            partitions: 64,
            pacing: Pacing::AsFastAsPossible,
            min_remote_delay: None,
            max_paced_window: SimDuration::from_millis(1),
            spin_threshold: Duration::from_micros(200),
            keep_up_warning: Duration::from_millis(20),
            ingress_delay: SimDuration::from_millis(1),
            journal: false,
            isolate_panics: false,
            spatial_cell: 50.0,
            cluster: None,
        }
    }
}

/// A runtime of timed actors, spread over one or more shards (threads).
///
/// Events are processed in `(time, sender, sequence, sub)` order for each actor. As long as cross-shard messages
/// respect the lookahead (see [`Stats::lookahead_violations`]), results are identical for any number of shards.
pub struct Runtime<M, E> {
    config: RuntimeConfig,
    pub(crate) world: World,
    pub(crate) shards: Vec<Shard<M, E>>,
    kind_ids: HashMap<TypeId, u16>,
    /// The block currently being filled for each (partition, kind), and the next index in it.
    open_blocks: HashMap<(u32, u16), (BlockId, u32)>,
    next_partition: u32,
    next_group: u32,
    /// Everything before this time has been processed.
    pub(crate) now: SimTime,
    clock: ClockHandle,
    pub(crate) control: Arc<ControlShared<M, E>>,
    subscribers: Arc<Subscribers<E>>,
    system: Arc<SystemBus>,
    dropped_unknown: u64,
    pub(crate) needs_shrink: bool,
    placement: NodePlacement,
    link: Option<Arc<dyn Link<M>>>,
}

impl<M: Clone + Send + 'static, E: Clone + Send + 'static> Runtime<M, E> {
    /// A runtime with the given settings. Emitted events are discarded until you choose an output with
    /// [`Self::buffer_events`], [`Self::on_event`] or [`Self::subscribe`].
    ///
    /// # Panics
    /// If `shards` is zero or more than `partitions`.
    pub fn new(config: RuntimeConfig) -> Self {
        assert!(config.shards >= 1, "need at least one shard");
        assert!(
            config.partitions as usize >= config.shards,
            "need at least as many partitions ({}) as shards ({})",
            config.partitions,
            config.shards
        );
        let placement = config
            .cluster
            .clone()
            .unwrap_or_else(|| NodePlacement::single(config.shards));
        assert!(
            placement.node < placement.nodes() && placement.shards_per_node[placement.node as usize] == config.shards,
            "node {} of {:?} doesn't have {} shards",
            placement.node,
            placement.shards_per_node,
            config.shards
        );
        for node in 0..placement.nodes() {
            let (first, end) = placement.partitions_of(node, config.partitions);
            assert!(
                (end - first) as usize >= placement.shards_per_node[node as usize].max(1),
                "node {node} has {} partitions for {} shards: use more partitions",
                end - first,
                placement.shards_per_node[node as usize]
            );
        }
        let subscribers = Arc::new(Subscribers::default());
        let system = Arc::new(SystemBus::default());
        let settings = ShardSettings {
            shards: config.shards,
            node: placement.node,
            nodes: placement.nodes(),
            trace: config.trace,
            isolate_panics: config.isolate_panics,
            spatial_cell: config.spatial_cell,
        };
        let shards = (0..config.shards)
            .map(|i| {
                let queue = match config.queue {
                    QueueKind::Heap => Queue::Heap(HeapQueue::new()),
                    QueueKind::Wheel { bucket } => Queue::Wheel(WheelQueue::with_bucket(bucket)),
                };
                let events = EventOut::new(Arc::clone(&subscribers));
                Shard::new(i as u16, queue, events, Arc::clone(&system), settings)
            })
            .collect();
        let world = World {
            dir: Arc::new(Directory {
                node: placement.node,
                ..Directory::default()
            }),
            kind_types: Vec::new(),
            field_types: Vec::new(),
            shards: config.shards,
            nodes: placement.nodes(),
            seed: config.seed,
        };
        let clock = ClockHandle::new(config.pacing);
        let control = Arc::new(ControlShared::new(
            config.shards,
            Arc::clone(&world.dir),
            clock.clone(),
            config.ingress_delay,
            config.journal,
        ));
        Self {
            world,
            shards,
            kind_ids: HashMap::new(),
            open_blocks: HashMap::new(),
            next_partition: 0,
            next_group: 0,
            now: SimTime::ZERO,
            clock,
            control,
            subscribers,
            system,
            dropped_unknown: 0,
            needs_shrink: false,
            placement,
            link: None,
            config,
        }
    }

    // ----- outputs -----

    /// Collects emitted events in memory; read them with [`Self::take_events`].
    pub fn buffer_events(&mut self) {
        for shard in &mut self.shards {
            shard.core.events.buffer = Some(Vec::new());
        }
    }

    /// Passes each emitted event to `callback`. With several shards, it is called from several threads.
    pub fn on_event(&mut self, callback: impl Fn(Emitted<E>) + Send + Sync + 'static) {
        let callback: Arc<dyn Fn(Emitted<E>) + Send + Sync> = Arc::new(callback);
        for shard in &mut self.shards {
            shard.core.events.callback = Some(Arc::clone(&callback));
        }
    }

    /// Stops buffering events and calling the callback (subscriptions are unaffected).
    pub fn discard_events(&mut self) {
        for shard in &mut self.shards {
            shard.core.events.buffer = None;
            shard.core.events.callback = None;
        }
    }

    /// Returns and clears the buffered events, ordered by time and then emitting actor (each actor's own events
    /// stay in the order it emitted them). Empty unless [`Self::buffer_events`] is on.
    pub fn take_events(&mut self) -> Vec<Emitted<E>> {
        let mut all = Vec::new();
        for shard in &mut self.shards {
            if let Some(buffer) = &mut shard.core.events.buffer {
                all.append(buffer);
            }
        }
        all.sort_by_key(|e| (e.at, e.src));
        all
    }

    /// Subscribes to emitted events as a live, lossy stream. `capacity` is how many batches (one per shard per
    /// window) may wait unread before newer ones are dropped for this subscriber. Can be called while running.
    pub fn subscribe(&self, capacity: usize) -> EventStream<E> {
        self.subscribers.subscribe(Some(capacity), None)
    }

    /// Subscribes to every emitted event, losing none: for recording. The buffer is unbounded, so if the
    /// consumer falls behind, memory grows rather than events being dropped; the simulation never waits.
    pub fn subscribe_lossless(&self) -> EventStream<E> {
        self.subscribers.subscribe(None, None)
    }

    /// Like [`Self::subscribe`], receiving only the events `filter` accepts.
    pub fn subscribe_filtered(
        &self,
        capacity: usize,
        filter: impl Fn(&Emitted<E>) -> bool + Send + Sync + 'static,
    ) -> EventStream<E> {
        let filter: EventFilter<E> = Arc::new(filter);
        self.subscribers.subscribe(Some(capacity), Some(filter))
    }

    /// Subscribes to system events (warnings and notices about the runtime itself).
    pub fn system_events(&self) -> SystemEvents {
        self.system.subscribe()
    }

    /// The most recent system events (up to 256).
    pub fn recent_system_events(&self) -> Vec<SystemNotice> {
        self.system.recent()
    }

    /// A handle for publishing events from elsewhere (another node of a cluster) to this runtime's subscribers, as
    /// if its own actors had emitted them.
    pub fn event_publisher(&self) -> EventPublisher<E> {
        EventPublisher {
            subscribers: Arc::clone(&self.subscribers),
        }
    }

    /// A handle for publishing system events from elsewhere (another node of a cluster) as this runtime's own.
    pub fn system_publisher(&self) -> SystemPublisher {
        SystemPublisher {
            bus: Arc::clone(&self.system),
        }
    }

    // ----- cluster -----

    /// Connects this node to the rest of its cluster (see [`crate::cluster`]). Needed before running whenever
    /// [`RuntimeConfig::cluster`] has more than one node.
    pub fn set_link(&mut self, link: Arc<dyn Link<M>>) {
        self.link = Some(link);
    }

    /// Where this runtime sits in its cluster (a single node without one).
    pub fn placement(&self) -> &NodePlacement {
        &self.placement
    }

    /// `true` if the actor exists and lives on this node.
    pub fn is_local(&self, id: ActorId) -> bool {
        self.world.locate(id).is_some()
    }

    /// The node an actor lives on, if it exists.
    pub fn node_of(&self, id: ActorId) -> Option<u32> {
        self.world.dir.home(id).map(|(node, _)| node)
    }

    // ----- control -----

    /// A handle for controlling the simulation from other threads while it runs.
    pub fn control(&self) -> Control<M, E> {
        self.control.set_directory(Arc::clone(&self.world.dir));
        Control {
            shared: Arc::clone(&self.control),
        }
    }

    /// A handle to the clock, for changing speed, pausing, stepping or stopping, from any thread, while running.
    pub fn clock(&self) -> ClockHandle {
        self.clock.clone()
    }

    /// The messages injected through [`Control`], if [`RuntimeConfig::journal`] is on, in delivery order. Inject
    /// them into a fresh runtime to replay a live session.
    pub fn take_journal(&mut self) -> Vec<Injected<M>> {
        let mut journal = self
            .control
            .journal
            .as_ref()
            .map(|j| std::mem::take(&mut *j.lock().unwrap_or_else(|p| p.into_inner())))
            .unwrap_or_default();
        journal.sort_by_key(|i| i.at);
        journal
    }

    // ----- building -----

    /// Adds an actor, placed round-robin over the partitions, and returns its ID. Its `on_start` runs at the
    /// current time, before the next delivery.
    pub fn spawn<A: Actor<M, E>>(&mut self, actor: A) -> ActorId {
        let partition = Partition(self.next_partition);
        self.next_partition = (self.next_partition + 1) % self.config.partitions;
        self.spawn_in(partition, actor)
    }

    /// Adds an actor in a specific partition (see [`crate::placement`]) and returns its ID.
    ///
    /// # Panics
    /// If the partition doesn't exist.
    pub fn spawn_in<A: Actor<M, E>>(&mut self, partition: Partition, actor: A) -> ActorId {
        assert!(partition.0 < self.config.partitions, "no {partition:?}");
        let kind = self.kind_of::<A>();
        let (node, shard) = self.placement.home_of(partition.0, self.config.partitions);
        let local = node == self.placement.node;
        let dir = Arc::make_mut(&mut self.world.dir);

        let (block, index) = match self.open_blocks.get(&(partition.0, kind)) {
            Some(&(block, next)) if u64::from(next) < ActorId::BLOCK_SIZE => (block, next),
            _ => {
                let block = BlockId::new(dir.blocks.len() as u64);
                assert!(!block.is_reserved(), "actor IDs exhausted");
                let store = if local {
                    let stores = &mut self.shards[usize::from(shard)].stores;
                    stores.push(Box::new(Arena::<A> {
                        actors: Vec::new(),
                        meta: Vec::new(),
                    }));
                    u32::try_from(stores.len() - 1).expect("too many blocks on one shard")
                } else {
                    u32::MAX // lives on another node: only its place in the directory is kept here
                };
                dir.blocks.push(Some(BlockEntry {
                    node,
                    shard,
                    kind,
                    store,
                    count: 0,
                }));
                (block, 0)
            }
        };
        self.open_blocks.insert((partition.0, kind), (block, index + 1));
        let entry = dir.blocks[block.raw() as usize]
            .as_mut()
            .expect("open block is in the directory");
        entry.count += 1;
        let store = entry.store as usize;

        let id = ActorId::from_parts(block, index as u16);
        if !local {
            return id;
        }
        let rng = ActorRng::seed_from_u64(splitmix(self.config.seed) ^ id.raw());
        let shard = &mut self.shards[usize::from(shard)];
        let arena = shard.stores[store]
            .as_any_mut()
            .downcast_mut::<Arena<A>>()
            .expect("block store matches its kind");
        arena.actors.push(actor);
        arena.meta.push(ActorMeta::new(rng));
        shard.pending_starts.push(id);
        self.needs_shrink = true;
        id
    }

    /// Gives an actor a position, so spatial broadcasts can reach it. Returns `false` if the actor doesn't exist.
    pub fn set_position(&mut self, id: ActorId, position: [f64; 3]) -> bool {
        match self.world.locate(id) {
            Some(slot) => {
                self.shards[usize::from(slot.shard)].spatial.insert(id, position);
                true
            }
            None => false,
        }
    }

    /// Creates a group of actors for [`crate::Ctx::multicast`]. Unknown IDs are ignored.
    pub fn add_group(&mut self, members: &[ActorId]) -> GroupId {
        let group = GroupId(self.next_group);
        self.next_group += 1;
        for shard in &mut self.shards {
            shard.group_mut(group);
        }
        for &member in members {
            if let Some(slot) = self.world.locate(member) {
                self.shards[usize::from(slot.shard)].group_mut(group).push(member);
            }
        }
        group
    }

    /// Adds a shard-local field (see [`Field`]): every shard gets a copy of `field`.
    ///
    /// # Panics
    /// If a field of this type was already added.
    pub fn add_field<F: Field>(&mut self, field: F) {
        let type_id = TypeId::of::<F>();
        assert!(
            !self.world.field_types.contains(&type_id),
            "field {} already added",
            std::any::type_name::<F>()
        );
        self.world.field_types.push(type_id);
        for shard in &mut self.shards {
            shard.core.fields.push(Box::new(field.clone()));
        }
    }

    /// Field `F` as seen by the first shard (every shard's copy receives the same updates).
    pub fn field<F: Field>(&self) -> Option<&F> {
        let index = self.world.field_types.iter().position(|t| *t == TypeId::of::<F>())?;
        self.shards[0].core.fields[index].as_any().downcast_ref()
    }

    /// Adds a fan-out route owned by `owner`, which can then send along it with [`crate::Ctx::fanout`].
    ///
    /// # Panics
    /// If `owner` doesn't exist.
    pub fn add_route(&mut self, owner: ActorId, edges: Vec<RouteEdge>, jitter: Jitter) -> RouteId {
        let (node, shard) = self.world.dir.home(owner).expect("route owner exists");
        let dir = Arc::make_mut(&mut self.world.dir);
        let id = RouteId(u32::try_from(dir.route_homes.len()).expect("too many routes"));
        dir.route_homes.push((node, shard));
        if node == self.placement.node {
            self.shards[usize::from(shard)].routes.insert(
                id,
                Route {
                    owner,
                    edges: {
                        // (Collected in place, the 24-byte edges' buffer would be kept: trim it.)
                        let mut compact: Vec<crate::route::Edge> =
                            edges.into_iter().map(crate::route::Edge::new).collect();
                        compact.shrink_to_fit();
                        compact
                    },
                    jitter,
                },
            );
        }
        id
    }

    /// Adds or prunes a route edge now (between runs; while running use [`Control::change_route`] or
    /// [`crate::Ctx::change_route`]). Returns `true` if the route existed (and, for a prune, the edge was found).
    pub fn change_route(&mut self, route: RouteId, op: TopologyOp) -> bool {
        match self.world.dir.route_shard(route) {
            Some(shard) => self.shards[usize::from(shard)].apply_topology(&self.world, route, op),
            None => false,
        }
    }

    /// Marks an actor as observed or not (between runs; while running use [`Control::observe`]).
    pub fn observe(&mut self, id: ActorId, observed: bool) -> bool {
        match self.world.locate(id) {
            Some(slot) => self.shards[usize::from(slot.shard)].set_observed(&self.world, id, observed),
            None => false,
        }
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
            seq: self.control.world_seq.fetch_add(1, Ordering::AcqRel),
            sub: 0,
        };
        match self.world.dir.home(to) {
            Some((node, shard)) if node == self.placement.node => self.shards[usize::from(shard)]
                .core
                .queue
                .push(key, Item::Message { dst: to, msg }),
            // Leaves for its node at the start of the next run.
            Some((node, shard)) => self.shards[0]
                .core
                .send_to_node(node, shard, key, Item::Message { dst: to, msg }),
            None => self.dropped_unknown += 1,
        }
    }

    // ----- inspection -----

    /// The simulation time processed so far: everything before it has been delivered.
    pub fn now(&self) -> SimTime {
        self.now
    }

    /// Number of shards.
    pub fn shards(&self) -> usize {
        self.shards.len()
    }

    /// Number of partitions.
    pub fn partitions(&self) -> u32 {
        self.config.partitions
    }

    /// The time of the next pending event on any shard, if any. Runs pending `on_start` handlers and control
    /// commands first, since they may schedule events.
    pub fn next_event_time(&mut self) -> Option<SimTime> {
        self.start_pending();
        self.shards
            .iter_mut()
            .filter_map(|s| s.core.queue.peek_key())
            .map(|k| k.at)
            .min()
    }

    /// Number of pending events.
    pub fn pending(&self) -> usize {
        self.shards.iter().map(|s| s.core.queue.len()).sum()
    }

    /// The lookahead: how far all shards can run in parallel without exchanging messages. It's the shortest delay
    /// of any route edge between shards (less the jitter allowance), or [`RuntimeConfig::min_remote_delay`] if that
    /// is shorter. `SimDuration::MAX` means nothing crosses shards.
    pub fn lookahead(&self) -> SimDuration {
        if self.shards.len() == 1 && self.placement.nodes() == 1 {
            return SimDuration::MAX;
        }
        let mut lookahead = self.config.min_remote_delay.unwrap_or(SimDuration::MAX);
        let node = self.placement.node;
        for (index, shard) in self.shards.iter().enumerate() {
            for route in shard.routes.values() {
                let reduction = route.jitter.max_reduction();
                for edge in &route.edges {
                    if self
                        .world
                        .dir
                        .home(edge.target)
                        .is_some_and(|home| home != (node, index as u16))
                    {
                        lookahead = lookahead.min(edge.delay().saturating_sub(reduction));
                    }
                }
            }
        }
        lookahead.max(SimDuration::NANOSECOND)
    }

    /// Counters, summed over shards.
    pub fn stats(&self) -> Stats {
        let mut total = Stats {
            dropped_unknown: self.dropped_unknown,
            ..Stats::default()
        };
        for shard in &self.shards {
            total.merge(&shard.core.stats);
        }
        total
    }

    /// How well paced runs kept to the clock.
    /// Wall time the shards have spent waiting for each other (and for other nodes) at window ends, summed over
    /// shards: with the run's wall time × shards, how much of the shards' time synchronization takes.
    pub fn sync_wait(&self) -> std::time::Duration {
        self.shards.iter().map(|s| s.sync_wait).sum()
    }

    pub fn pacing_stats(&self) -> &PacingStats {
        &self.shards[0].pacing
    }

    /// Returns and clears the delivery trace (empty unless [`RuntimeConfig::trace`] is on).
    pub fn take_trace(&mut self) -> Vec<TraceEntry> {
        let mut all = Vec::new();
        for shard in &mut self.shards {
            if let Some(trace) = &mut shard.trace {
                all.append(trace);
            }
        }
        all
    }

    /// The actor with this ID, if it exists and is of type `A`.
    pub fn actor<A: Actor<M, E>>(&self, id: ActorId) -> Option<&A> {
        let slot = self.world.locate(id)?;
        if self.world.kind_types[usize::from(slot.kind)] != TypeId::of::<A>() {
            return None;
        }
        let store = &self.shards[usize::from(slot.shard)].stores[slot.store as usize];
        store
            .as_any()
            .downcast_ref::<Arena<A>>()?
            .actors
            .get(slot.index as usize)
    }

    /// Mutable access to the actor with this ID, if it exists and is of type `A`. For setup and inspection between
    /// runs, e.g. giving an actor the route it should fan out on.
    pub fn actor_mut<A: Actor<M, E>>(&mut self, id: ActorId) -> Option<&mut A> {
        let slot = self.world.locate(id)?;
        self.shards[usize::from(slot.shard)].actor_mut(&self.world, id)
    }

    /// Releases unused capacity in actor storage. Runs automatically before the first run after spawning.
    pub fn shrink_to_fit(&mut self) {
        for shard in &mut self.shards {
            for store in &mut shard.stores {
                store.shrink_to_fit();
            }
        }
        Arc::make_mut(&mut self.world.dir).blocks.shrink_to_fit();
        self.needs_shrink = false;
    }

    /// Approximate memory held by actors, their framework state, the directory and routes, in bytes.
    /// Excludes pending events.
    pub fn memory_bytes(&self) -> usize {
        let actors: usize = self
            .shards
            .iter()
            .flat_map(|s| &s.stores)
            .map(|store| store.memory_bytes())
            .sum();
        let directory = self.world.dir.blocks.capacity() * size_of::<Option<BlockEntry>>();
        let routes: usize = self
            .shards
            .iter()
            .flat_map(|s| s.routes.values())
            .map(|r| size_of::<Route>() + r.edges.capacity() * size_of::<crate::route::Edge>())
            .sum();
        actors + directory + routes
    }

    // ----- running -----

    /// Delivers the single next event (the earliest on any shard) and returns its time, or `None` if nothing is
    /// pending. Runs on the calling thread.
    ///
    /// # Panics
    /// On a node of a cluster: nodes only run in step with each other.
    pub fn step(&mut self) -> Option<SimTime> {
        assert!(self.placement.nodes() == 1, "a node of a cluster can't step on its own");
        self.start_pending();
        let (i, key) = self
            .shards
            .iter_mut()
            .enumerate()
            .filter_map(|(i, s)| s.core.queue.peek_key().map(|k| (i, k)))
            .min_by_key(|&(_, k)| k)?;
        let shard = &mut self.shards[i];
        let (key, item) = shard.core.queue.pop().filter(|(k, _)| *k == key).expect("peeked event");
        shard.core.window_end = key.at;
        shard.deliver(&self.world, key, item);
        shard.publish_outputs(key.at, false);
        self.route_outbox(i);
        self.now = self.now.max(key.at);
        Some(key.at)
    }

    /// Delivers every event before `end`, then sets [`Self::now`] to `end`, pacing against the clock if it's in
    /// real-time mode. Returns early if the clock is stopped. Returns the number of events delivered.
    pub fn run_until(&mut self, end: SimTime) -> u64 {
        let token = self.clock.stop_token();
        self.run_to(end, token)
    }

    /// Like [`Self::run_until`], but any [`ClockHandle::stop`] issued after `token` was taken ends the run, even
    /// one issued before this call starts. Use it when the run starts on another thread: take the token before
    /// handing the runtime over, so a stop that races the thread's start isn't lost.
    pub fn run_until_unless_stopped(&mut self, end: SimTime, token: StopToken) -> u64 {
        self.run_to(end, token)
    }

    /// Applies the control commands waiting for a run (queries, updates, injections, route changes sent through a
    /// [`Control`] while no run was going, or as one ended), now, at the current time. Returns how many shards had
    /// any. Without this they wait for the next run.
    pub fn apply_control(&mut self) -> usize {
        let now = self.now;
        let mut applied = 0;
        for shard in &mut self.shards {
            if self.control.has_pending(shard.core.shard) {
                shard.apply_commands(&self.control, &self.world, now);
                applied += 1;
            }
        }
        applied
    }

    /// Runs until no events are pending, or the clock is stopped. In real-time mode the simulation keeps following
    /// the clock even when idle (live input may arrive), so it runs until stopped. Returns the number of events
    /// delivered.
    pub fn run(&mut self) -> u64 {
        let token = self.clock.stop_token();
        self.run_to(SimTime::MAX, token)
    }

    fn kind_of<A: Actor<M, E>>(&mut self) -> u16 {
        let type_id = TypeId::of::<A>();
        if let Some(&kind) = self.kind_ids.get(&type_id) {
            return kind;
        }
        let kind = u16::try_from(self.world.kind_types.len()).expect("too many actor kinds");
        self.world.kind_types.push(type_id);
        self.kind_ids.insert(type_id, kind);
        kind
    }

    /// Runs pending control commands and `on_start` handlers on the calling thread.
    fn start_pending(&mut self) {
        if self.needs_shrink {
            self.shrink_to_fit();
        }
        self.control.set_directory(Arc::clone(&self.world.dir));
        let now = self.now;
        for i in 0..self.shards.len() {
            self.shards[i].core.window_end = now;
            self.shards[i].apply_commands(&self.control, &self.world, now);
            self.shards[i].run_starts(&self.world, now);
            self.route_outbox(i);
        }
    }

    /// Moves shard `i`'s cross-shard messages straight into their destination queues (single-threaded stepping).
    fn route_outbox(&mut self, i: usize) {
        for (to, items) in self.shards[i].take_outbox().into_iter().enumerate() {
            for (key, item) in items {
                self.shards[to].core.queue.push(key, item);
            }
        }
    }

    fn run_to(&mut self, end: SimTime, token: StopToken) -> u64 {
        if self.needs_shrink {
            self.shrink_to_fit();
        }
        self.control.set_directory(Arc::clone(&self.world.dir));
        let delivered_before = self.stats().delivered;
        let n = self.shards.len();
        let plan = RunPlan {
            start: self.now,
            end,
            lookahead: self.lookahead(),
            max_paced_window: self.config.max_paced_window,
            spin_threshold: self.config.spin_threshold,
            keep_up_warning: self.config.keep_up_warning,
        };
        self.clock.start_run(self.now);
        let shared = RunShared {
            world: &self.world,
            mailboxes: (0..n * n).map(|_| Mutex::new(Vec::new())).collect(),
            barrier: WindowBarrier::new(n),
            clock: &self.clock,
            control: &self.control,
            stop_epoch: token.0,
            link: self.link.as_deref(),
        };
        assert!(
            shared.link.is_some() || self.placement.nodes() == 1,
            "a node of a cluster needs a link (Runtime::set_link) to run"
        );

        let outcomes: Vec<ShardOutcome> = if n == 1 {
            vec![self.shards[0].work(&shared, &plan, 1)]
        } else {
            let results: Vec<_> = std::thread::scope(|scope| {
                let handles: Vec<_> = self
                    .shards
                    .iter_mut()
                    .map(|shard| {
                        let shared = &shared;
                        let plan = &plan;
                        scope.spawn(move || {
                            let result = catch_unwind(AssertUnwindSafe(|| shard.work(shared, plan, n)));
                            if result.is_err() {
                                // Release the other shards from the barrier instead of leaving them waiting.
                                shared.barrier.poison();
                            }
                            result
                        })
                    })
                    .collect();
                handles
                    .into_iter()
                    .map(|h| h.join().expect("shard thread returned"))
                    .collect()
            });
            let mut outcomes = Vec::with_capacity(n);
            for result in results {
                match result {
                    Ok(outcome) => outcomes.push(outcome),
                    Err(panic) => resume_unwind(panic),
                }
            }
            outcomes
        };

        // Messages still in mailboxes (a stopped run can leave some) go back into their destination queues.
        for (index, mailbox) in shared.mailboxes.into_iter().enumerate() {
            let to = index % n;
            for (key, item) in mailbox.into_inner().unwrap_or_else(|p| p.into_inner()) {
                self.shards[to].core.queue.push(key, item);
            }
        }

        let stopped = outcomes.iter().any(|o| o.stopped);
        let processed = outcomes.iter().map(|o| o.processed).max().unwrap_or(self.now);
        let last_event = self.shards.iter().map(|s| s.last_event).max().unwrap_or(self.now);
        self.now = if !stopped && end != SimTime::MAX {
            self.now.max(end)
        } else if stopped {
            self.now.max(processed)
        } else {
            self.now.max(last_event)
        };
        self.stats().delivered - delivered_before
    }
}

/// Publishes system events on a runtime's bus from outside it (see [`Runtime::system_publisher`]).
#[derive(Clone, Debug)]
pub struct SystemPublisher {
    bus: Arc<SystemBus>,
}

impl SystemPublisher {
    /// Publishes `event`, at simulation time `at`.
    pub fn publish(&self, at: SimTime, event: crate::SystemEvent) {
        self.bus.publish(at, event);
    }
}

/// Publishes events to a runtime's subscribers from outside it (see [`Runtime::event_publisher`]).
pub struct EventPublisher<E> {
    subscribers: Arc<Subscribers<E>>,
}

impl<E> Clone for EventPublisher<E> {
    fn clone(&self) -> Self {
        Self {
            subscribers: Arc::clone(&self.subscribers),
        }
    }
}

impl<E: Clone> EventPublisher<E> {
    /// Hands a batch of events to every subscriber, without blocking.
    pub fn publish(&self, batch: Vec<Emitted<E>>) {
        self.subscribers.publish(batch);
    }
}

impl<E> fmt::Debug for EventPublisher<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EventPublisher").finish_non_exhaustive()
    }
}

impl<M, E> fmt::Debug for Runtime<M, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Runtime")
            .field("now", &self.now)
            .field("shards", &self.shards.len())
            .field("partitions", &self.config.partitions)
            .finish_non_exhaustive()
    }
}
