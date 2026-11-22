//! The runtime: owns the shards, builds the network, and runs it.

use std::any::TypeId;
use std::collections::HashMap;
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rand_core::SeedableRng;
use tempo_core::{ActorId, BlockId, OrderKey, SeqCounter, SimDuration, SimTime};
use tempo_queue::{HeapQueue, TimeQueue, WheelQueue};

use crate::barrier::WindowBarrier;
use crate::clock::{ClockHandle, Pacing};
use crate::ctx::ActorMeta;
use crate::events::{Emitted, Events};
use crate::metrics::{PacingStats, Stats};
use crate::placement::{Partition, shard_of};
use crate::route::{Route, splitmix};
use crate::shard::{Arena, BlockEntry, Item, Queue, RunPlan, RunShared, Shard, ShardOutcome, TraceEntry, World};
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
    /// The shortest delay of a *direct* message (`send`, `send_after`, `send_at`) between actors on different
    /// shards, if any are sent. Route delays are known in advance; direct sends are not, so declare their minimum
    /// here. It bounds the lookahead, and therefore the window length.
    pub min_remote_delay: Option<SimDuration>,
    /// While paced, windows are at most this long, so no shard gets further than this ahead of the clock.
    pub max_paced_window: SimDuration,
    /// While waiting for the clock, spin (rather than sleep) for the last this-much wall time.
    pub spin_threshold: Duration,
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
        }
    }
}

/// A runtime of timed actors, spread over one or more shards (threads).
///
/// Events are processed in `(time, sender, sequence, sub)` order for each actor. As long as cross-shard messages
/// respect the lookahead (see [`Stats::lookahead_violations`]), results are identical for any number of shards.
pub struct Runtime<M, E> {
    config: RuntimeConfig,
    world: World,
    shards: Vec<Shard<M, E>>,
    kind_ids: HashMap<TypeId, u16>,
    /// The block currently being filled for each (partition, kind), and the next index in it.
    open_blocks: HashMap<(u32, u16), (BlockId, u32)>,
    next_partition: u32,
    /// Everything before this time has been processed.
    now: SimTime,
    world_seq: SeqCounter,
    clock: ClockHandle,
    dropped_unknown: u64,
    needs_shrink: bool,
}

impl<M: Send + 'static, E: Send + 'static> Runtime<M, E> {
    /// A runtime with the given settings. Emitted events are discarded until you choose an output with
    /// [`Self::buffer_events`] or [`Self::on_event`].
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
        let shards = (0..config.shards)
            .map(|i| {
                let queue = match config.queue {
                    QueueKind::Heap => Queue::Heap(HeapQueue::new()),
                    QueueKind::Wheel { bucket } => Queue::Wheel(WheelQueue::with_bucket(bucket)),
                };
                Shard::new(i as u16, config.shards, queue, Events::Discard, config.trace)
            })
            .collect();
        Self {
            world: World {
                blocks: Vec::new(),
                routes: Vec::new(),
                kind_types: Vec::new(),
                seed: config.seed,
            },
            shards,
            kind_ids: HashMap::new(),
            open_blocks: HashMap::new(),
            next_partition: 0,
            now: SimTime::ZERO,
            world_seq: SeqCounter::new(),
            clock: ClockHandle::new(config.pacing),
            dropped_unknown: 0,
            needs_shrink: false,
            config,
        }
    }

    /// Collects emitted events in memory; read them with [`Self::take_events`].
    pub fn buffer_events(&mut self) {
        self.set_events(&Events::Buffer(Vec::new()));
    }

    /// Passes each emitted event to `callback`. With several shards, it is called from several threads.
    pub fn on_event(&mut self, callback: impl Fn(Emitted<E>) + Send + Sync + 'static) {
        self.set_events(&Events::Callback(Arc::new(callback)));
    }

    /// Discards emitted events (the default).
    pub fn discard_events(&mut self) {
        self.set_events(&Events::Discard);
    }

    fn set_events(&mut self, events: &Events<E>) {
        for shard in &mut self.shards {
            shard.core.events = events.fresh();
        }
    }

    /// Returns and clears the buffered events, ordered by time and then emitting actor (each actor's own events
    /// stay in the order it emitted them). Empty unless [`Self::buffer_events`] is on.
    pub fn take_events(&mut self) -> Vec<Emitted<E>> {
        let mut all = Vec::new();
        for shard in &mut self.shards {
            if let Events::Buffer(buffer) = &mut shard.core.events {
                all.append(buffer);
            }
        }
        all.sort_by_key(|e| (e.at, e.src));
        all
    }

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
        let shard = shard_of(partition, self.config.partitions, self.config.shards);

        let (block, index) = match self.open_blocks.get(&(partition.0, kind)) {
            Some(&(block, next)) if u64::from(next) < ActorId::BLOCK_SIZE => (block, next),
            _ => {
                let block = BlockId::new(self.world.blocks.len() as u64);
                assert!(!block.is_reserved(), "actor IDs exhausted");
                let stores = &mut self.shards[usize::from(shard)].stores;
                self.world.blocks.push(Some(BlockEntry {
                    shard,
                    kind,
                    store: u32::try_from(stores.len()).expect("too many blocks on one shard"),
                    count: 0,
                }));
                stores.push(Box::new(Arena::<A> {
                    actors: Vec::new(),
                    meta: Vec::new(),
                }));
                (block, 0)
            }
        };
        self.open_blocks.insert((partition.0, kind), (block, index + 1));
        let entry = self.world.blocks[block.raw() as usize]
            .as_mut()
            .expect("open block is in the directory");
        entry.count += 1;
        let store = entry.store as usize;

        let id = ActorId::from_parts(block, index as u16);
        let rng = ActorRng::seed_from_u64(splitmix(self.config.seed) ^ id.raw());
        let shard = &mut self.shards[usize::from(shard)];
        let arena = shard.stores[store]
            .as_any_mut()
            .downcast_mut::<Arena<A>>()
            .expect("block store matches its kind");
        arena.actors.push(actor);
        arena.meta.push(ActorMeta {
            seq: SeqCounter::new(),
            rng,
        });
        shard.pending_starts.push(id);
        self.needs_shrink = true;
        id
    }

    /// Adds a fan-out route owned by `owner`, which can then send along it with [`crate::Ctx::fanout`].
    pub fn add_route(&mut self, owner: ActorId, edges: Vec<RouteEdge>, jitter: Jitter) -> RouteId {
        let id = RouteId(u32::try_from(self.world.routes.len()).expect("too many routes"));
        self.world.routes.push(Route { owner, edges, jitter });
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
        match self.world.locate(to) {
            Some(slot) => self.shards[usize::from(slot.shard)]
                .core
                .queue
                .push(key, Item::Message { dst: to, msg }),
            None => self.dropped_unknown += 1,
        }
    }

    /// A handle to the clock, for changing speed, pausing, stepping or stopping, from any thread, while running.
    pub fn clock(&self) -> ClockHandle {
        self.clock.clone()
    }

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

    /// The time of the next pending event on any shard, if any. Runs pending `on_start` handlers first, since
    /// they may schedule events.
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
        let mut lookahead = self.config.min_remote_delay.unwrap_or(SimDuration::MAX);
        if self.shards.len() == 1 {
            return SimDuration::MAX;
        }
        for route in &self.world.routes {
            let Some(owner) = self.world.locate(route.owner) else {
                continue;
            };
            let reduction = route.jitter.max_reduction();
            for edge in &route.edges {
                match self.world.locate(edge.target) {
                    Some(target) if target.shard != owner.shard => {
                        lookahead = lookahead.min(edge.delay.saturating_sub(reduction));
                    }
                    _ => {}
                }
            }
        }
        lookahead.max(SimDuration::NANOSECOND)
    }

    /// Delivers the single next event (the earliest on any shard) and returns its time, or `None` if nothing is
    /// pending. Runs on the calling thread.
    pub fn step(&mut self) -> Option<SimTime> {
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
        self.route_outbox(i);
        self.now = self.now.max(key.at);
        Some(key.at)
    }

    /// Delivers every event before `end`, then sets [`Self::now`] to `end`, pacing against the clock if it's in
    /// real-time mode. Returns early if the clock is stopped. Returns the number of events delivered.
    pub fn run_until(&mut self, end: SimTime) -> u64 {
        self.run_to(end)
    }

    /// Delivers events until none are pending (pacing against the clock if it's in real-time mode), or the clock
    /// is stopped. Returns the number of events delivered.
    pub fn run(&mut self) -> u64 {
        self.run_to(SimTime::MAX)
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
        if self.world.kind_types[usize::from(slot.kind)] != TypeId::of::<A>() {
            return None;
        }
        let store = &mut self.shards[usize::from(slot.shard)].stores[slot.store as usize];
        store
            .as_any_mut()
            .downcast_mut::<Arena<A>>()?
            .actors
            .get_mut(slot.index as usize)
    }

    /// Releases unused capacity in actor storage. Runs automatically before the first run after spawning.
    pub fn shrink_to_fit(&mut self) {
        for shard in &mut self.shards {
            for store in &mut shard.stores {
                store.shrink_to_fit();
            }
        }
        self.world.blocks.shrink_to_fit();
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
        let directory = self.world.blocks.capacity() * size_of::<Option<BlockEntry>>();
        let routes: usize = self
            .world
            .routes
            .iter()
            .map(|r| size_of::<Route>() + r.edges.capacity() * size_of::<RouteEdge>())
            .sum();
        actors + directory + routes
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

    fn prepare(&mut self) {
        if self.needs_shrink {
            self.shrink_to_fit();
        }
    }

    /// Runs pending `on_start` handlers on the calling thread.
    fn start_pending(&mut self) {
        self.prepare();
        let now = self.now;
        for i in 0..self.shards.len() {
            if self.shards[i].pending_starts.is_empty() {
                continue;
            }
            self.shards[i].core.window_end = now;
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

    fn run_to(&mut self, end: SimTime) -> u64 {
        self.prepare();
        let delivered_before = self.stats().delivered;
        let n = self.shards.len();
        let plan = RunPlan {
            start: self.now,
            end,
            lookahead: self.lookahead(),
            max_paced_window: self.config.max_paced_window,
            spin_threshold: self.config.spin_threshold,
        };
        self.clock.start_run(self.now);
        let shared = RunShared {
            world: &self.world,
            mailboxes: (0..n * n).map(|_| Mutex::new(Vec::new())).collect(),
            barrier: WindowBarrier::new(n),
            clock: &self.clock,
            stop_epoch: self.clock.stop_epoch(),
        };

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

impl<M, E> fmt::Debug for Runtime<M, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Runtime")
            .field("now", &self.now)
            .field("shards", &self.shards.len())
            .field("partitions", &self.config.partitions)
            .finish_non_exhaustive()
    }
}
