//! One shard: a single-threaded event loop owning a subset of the actors.
//!
//! Shards run in parallel using conservative time windows. With lookahead `L` (the minimum delay of any message
//! that crosses shards), once every shard has finished everything before `T`, no shard can still create a
//! cross-shard message earlier than `T + L`. So each window `[T, T + L)` is processed by all shards in parallel,
//! then they meet at the [`WindowBarrier`], exchange the cross-shard messages they produced, and agree on the next
//! `T`: the earliest pending event anywhere. Empty stretches of simulation time are skipped entirely.

use std::any::{Any, TypeId};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tempo_core::{ActorId, OrderKey, SimDuration, SimTime};
use tempo_queue::{HeapQueue, TimeQueue, WheelQueue};

use crate::barrier::WindowBarrier;
use crate::clock::{ClockHandle, ClockView, wait_for};
use crate::ctx::ActorMeta;
use crate::events::Events;
use crate::metrics::{PacingStats, Stats};
use crate::route::Route;
use crate::{Actor, Ctx, EdgeIdx};

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

/// Deliveries in transit between shards.
pub(crate) type Batch<M> = Vec<(OrderKey, Item<M>)>;

/// A queued delivery.
pub(crate) enum Item<M> {
    Message { dst: ActorId, msg: M },
    Routed { dst: ActorId, edge: EdgeIdx, msg: M },
}

impl<M> Item<M> {
    pub(crate) fn dst(&self) -> ActorId {
        match self {
            Self::Message { dst, .. } | Self::Routed { dst, .. } => *dst,
        }
    }
}

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

/// Everything shards share read-only while running: the directory and the routes.
pub(crate) struct World {
    pub(crate) blocks: Vec<Option<BlockEntry>>,
    pub(crate) routes: Vec<Route>,
    pub(crate) kind_types: Vec<TypeId>,
    pub(crate) seed: u64,
}

impl World {
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

/// A shard's mutable state that handlers reach through [`Ctx`].
pub(crate) struct Core<M, E> {
    pub(crate) shard: u16,
    pub(crate) queue: Queue<Item<M>>,
    pub(crate) events: Events<E>,
    pub(crate) stats: Stats,
    /// Cross-shard messages produced this window, per destination shard.
    pub(crate) outbox: Vec<Batch<M>>,
    /// Earliest time among this window's cross-shard messages.
    pub(crate) outbound_min: SimTime,
    /// End of the current window: cross-shard messages earlier than this would be late.
    pub(crate) window_end: SimTime,
}

impl<M, E> Core<M, E> {
    /// Queues a delivery locally, or for another shard.
    pub(crate) fn send(&mut self, world: &World, mut key: OrderKey, item: Item<M>) {
        let Some(slot) = world.locate(item.dst()) else {
            self.stats.dropped_unknown += 1;
            return;
        };
        if slot.shard == self.shard {
            self.queue.push(key, item);
            return;
        }
        if key.at < self.window_end {
            // Shorter than the lookahead: the receiver may already be past this time. Deliver at the next window
            // start instead (deterministically), and record the error.
            let error = self.window_end - key.at;
            self.stats.lookahead_violations += 1;
            self.stats.max_lookahead_violation = self.stats.max_lookahead_violation.max(error);
            key.at = self.window_end;
        }
        self.outbound_min = self.outbound_min.min(key.at);
        self.stats.remote_sends += 1;
        self.outbox[usize::from(slot.shard)].push((key, item));
    }
}

/// Type-erased storage for one ID block's actors (all of one Rust type). Dispatch is one virtual call; inside it
/// the actor's handlers are called statically.
pub(crate) trait KindStore<M, E>: Send {
    fn dispatch(&mut self, index: u32, core: &mut Core<M, E>, world: &World, key: OrderKey, item: Item<M>);
    fn start(&mut self, index: u32, core: &mut Core<M, E>, world: &World, now: SimTime, me: ActorId);
    fn shrink_to_fit(&mut self);
    fn memory_bytes(&self) -> usize;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

pub(crate) struct Arena<A> {
    pub(crate) actors: Vec<A>,
    pub(crate) meta: Vec<ActorMeta>,
}

impl<M: 'static, E: 'static, A: Actor<M, E>> KindStore<M, E> for Arena<A> {
    fn dispatch(&mut self, index: u32, core: &mut Core<M, E>, world: &World, key: OrderKey, item: Item<M>) {
        let i = index as usize;
        let mut ctx = Ctx {
            now: key.at,
            me: item.dst(),
            meta: &mut self.meta[i],
            core,
            world,
        };
        let actor = &mut self.actors[i];
        match item {
            Item::Message { msg, .. } => actor.on_message(&mut ctx, key.src, msg),
            Item::Routed { edge, msg, .. } => actor.on_route_message(&mut ctx, key.src, edge, msg),
        }
    }

    fn start(&mut self, index: u32, core: &mut Core<M, E>, world: &World, now: SimTime, me: ActorId) {
        let i = index as usize;
        let mut ctx = Ctx {
            now,
            me,
            meta: &mut self.meta[i],
            core,
            world,
        };
        self.actors[i].on_start(&mut ctx);
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

/// One shard.
pub(crate) struct Shard<M, E> {
    pub(crate) core: Core<M, E>,
    pub(crate) stores: Vec<Box<dyn KindStore<M, E>>>,
    pub(crate) pending_starts: Vec<ActorId>,
    pub(crate) trace: Option<Vec<TraceEntry>>,
    pub(crate) pacing: PacingStats,
    /// Time of the latest delivered event.
    pub(crate) last_event: SimTime,
    /// Reused buffer for draining mailboxes.
    spare: Batch<M>,
}

/// The shared state of one multi-shard run.
pub(crate) struct RunShared<'a, M> {
    pub(crate) world: &'a World,
    /// `mailboxes[from * n + to]`: cross-shard messages from one shard to another, handed over once per window.
    pub(crate) mailboxes: Vec<Mutex<Batch<M>>>,
    pub(crate) barrier: WindowBarrier,
    pub(crate) clock: &'a ClockHandle,
    pub(crate) stop_epoch: u64,
}

impl<M> RunShared<'_, M> {
    fn stop_requested(&self) -> bool {
        self.clock.stop_epoch() != self.stop_epoch
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
}

/// How a shard's part of a run ended.
#[derive(Copy, Clone, Debug)]
pub(crate) struct ShardOutcome {
    /// Everything before this time has been processed by this shard.
    pub(crate) processed: SimTime,
    pub(crate) stopped: bool,
}

impl<M: 'static, E: 'static> Shard<M, E> {
    pub(crate) fn new(index: u16, shards: usize, queue: Queue<Item<M>>, events: Events<E>, trace: bool) -> Self {
        Self {
            core: Core {
                shard: index,
                queue,
                events,
                stats: Stats::default(),
                outbox: (0..shards).map(|_| Vec::new()).collect(),
                outbound_min: SimTime::MAX,
                window_end: SimTime::ZERO,
            },
            stores: Vec::new(),
            pending_starts: Vec::new(),
            trace: trace.then(Vec::new),
            pacing: PacingStats::default(),
            last_event: SimTime::ZERO,
            spare: Vec::new(),
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
            self.stores[slot.store as usize].start(slot.index, &mut self.core, world, now, id);
            self.core.stats.started += 1;
        }
    }

    pub(crate) fn deliver(&mut self, world: &World, key: OrderKey, item: Item<M>) {
        self.last_event = key.at;
        let dst = item.dst();
        let Some(slot) = world.locate(dst) else {
            self.core.stats.dropped_unknown += 1;
            return;
        };
        debug_assert_eq!(slot.shard, self.core.shard, "delivered to the wrong shard");
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
        self.stores[slot.store as usize].dispatch(slot.index, &mut self.core, world, key, item);
        self.core.stats.delivered += 1;
        self.core.stats.routed_delivered += u64::from(routed);
    }

    /// The earliest time this shard knows of: its own next event, or a message it just sent elsewhere.
    fn local_next(&mut self) -> SimTime {
        let next = self.core.queue.peek_key().map_or(SimTime::MAX, |k| k.at);
        next.min(self.core.outbound_min)
    }

    /// Hands this window's cross-shard messages to their destinations' mailboxes.
    fn flush<'a>(&mut self, shared: &RunShared<'a, M>, n: usize) {
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
    fn drain<'a>(&mut self, shared: &RunShared<'a, M>, n: usize) {
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

    /// This shard's part of a run: the window loop. Every shard runs it, in parallel.
    pub(crate) fn work<'a>(&mut self, shared: &RunShared<'a, M>, plan: &RunPlan, n: usize) -> ShardOutcome {
        let world = shared.world;
        let leader = self.core.shard == 0;
        let mut clock = ClockView::new(shared.clock);
        let abort = || shared.stop_requested() || shared.barrier.is_poisoned();

        self.core.window_end = plan.start;
        self.run_starts(world, plan.start);
        self.flush(shared, n);
        let mut processed = plan.start;
        let (mut next, mut stop) = shared
            .barrier
            .arrive(self.local_next().as_nanos(), || shared.stop_requested());
        self.core.outbound_min = SimTime::MAX;

        loop {
            if stop {
                return ShardOutcome {
                    processed,
                    stopped: true,
                };
            }
            self.drain(shared, n);
            let t = SimTime::from_nanos(next);

            if t >= plan.end {
                // Nothing left before the end. A paced run still waits for the clock to reach the end.
                let reached = plan.end == SimTime::MAX || wait_for(&mut clock, plan.end, plan.spin_threshold, abort);
                return ShardOutcome {
                    processed: if reached && plan.end != SimTime::MAX {
                        plan.end
                    } else {
                        processed
                    },
                    stopped: !reached,
                };
            }

            // Wait until the clock allows time `t`. If the run is stopped meanwhile, skip this window (others may
            // have processed it; that is safe, since no shard goes beyond it before the next barrier).
            if wait_for(&mut clock, t, plan.spin_threshold, abort) {
                if leader && clock.is_paced() {
                    if let Some(due) = clock.state.wall_for(t) {
                        self.pacing.windows += 1;
                        self.pacing
                            .lateness
                            .record(Instant::now().saturating_duration_since(due));
                    }
                    shared.clock.publish_progress(t);
                } else if leader {
                    shared.clock.publish_progress(t);
                }

                let span = if clock.is_paced() {
                    plan.lookahead.min(plan.max_paced_window)
                } else {
                    plan.lookahead
                };
                let window_end = t.saturating_add(span.max(SimDuration::NANOSECOND)).min(plan.end);
                self.core.window_end = window_end;
                while let Some((key, item)) = self.core.queue.pop_before(window_end) {
                    self.deliver(world, key, item);
                }
                self.core.stats.windows += 1;
                processed = window_end;
            }

            self.flush(shared, n);
            (next, stop) = shared
                .barrier
                .arrive(self.local_next().as_nanos(), || shared.stop_requested());
            self.core.outbound_min = SimTime::MAX;
        }
    }

    /// Moves this shard's outboxes straight into other shards' queues (single-threaded stepping).
    pub(crate) fn take_outbox(&mut self) -> Vec<Batch<M>> {
        self.core.outbound_min = SimTime::MAX;
        let n = self.core.outbox.len();
        std::mem::replace(&mut self.core.outbox, (0..n).map(|_| Vec::new()).collect())
    }
}
