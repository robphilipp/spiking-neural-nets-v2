//! The control plane: change or inspect a simulation while it runs, from any thread.
//!
//! [`Control`] is a cheap, cloneable handle. Each request becomes a command for the shard that owns the target
//! (an actor or a route). Shards apply their commands at window boundaries, and also while they are idle waiting
//! for the clock (so a paused simulation can still be inspected and changed). Every request returns a [`Reply`]
//! that can be waited on or `.await`ed.
//!
//! Between runs, nothing applies commands, so replies wait until the next run starts. To inspect or change a
//! runtime that isn't running, use the [`crate::Runtime`] methods directly.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use tempo_core::{ActorId, OrderKey, SimDuration, SimTime};
use tempo_queue::TimeQueue;

use crate::reply::{Reply, reply};
use crate::shard::{Directory, Item, Shard, World};
use crate::{Actor, ClockHandle, EdgeIdx, RouteEdge, RouteId};

/// Work for a shard: runs on the shard's thread with the start time of its current window.
pub(crate) type Command<M, E> = Box<dyn FnOnce(&mut Shard<M, E>, &World, SimTime) + Send>;

/// A change to a fan-out route.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TopologyOp {
    /// Add an edge.
    Connect(RouteEdge),
    /// Remove the first edge to `target` with receiver index `edge`.
    Prune {
        /// The receiving actor.
        target: ActorId,
        /// The receiver's index for the connection.
        edge: EdgeIdx,
    },
}

/// A message injected from outside, as recorded in the input journal.
#[derive(Clone, Debug, PartialEq)]
pub struct Injected<M> {
    /// When it was delivered.
    pub at: SimTime,
    /// The recipient.
    pub to: ActorId,
    /// The message.
    pub msg: M,
}

pub(crate) struct ControlShared<M, E> {
    inboxes: Vec<Mutex<Vec<Command<M, E>>>>,
    pending: Vec<AtomicBool>,
    directory: Mutex<Arc<Directory>>,
    pub(crate) world_seq: AtomicU64,
    pub(crate) clock: ClockHandle,
    ingress_delay: SimDuration,
    pub(crate) journal: Option<Mutex<Vec<Injected<M>>>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl<M, E> ControlShared<M, E> {
    pub(crate) fn new(
        shards: usize,
        directory: Arc<Directory>,
        clock: ClockHandle,
        ingress_delay: SimDuration,
        journal: bool,
    ) -> Self {
        Self {
            inboxes: (0..shards).map(|_| Mutex::new(Vec::new())).collect(),
            pending: (0..shards).map(|_| AtomicBool::new(false)).collect(),
            directory: Mutex::new(directory),
            world_seq: AtomicU64::new(0),
            clock,
            ingress_delay,
            journal: journal.then(|| Mutex::new(Vec::new())),
        }
    }

    pub(crate) fn set_directory(&self, directory: Arc<Directory>) {
        *lock(&self.directory) = directory;
    }

    fn submit(&self, shard: u16, command: Command<M, E>) {
        lock(&self.inboxes[usize::from(shard)]).push(command);
        self.pending[usize::from(shard)].store(true, Ordering::Release);
    }

    pub(crate) fn has_pending(&self, shard: u16) -> bool {
        self.pending[usize::from(shard)].load(Ordering::Acquire)
    }

    pub(crate) fn take(&self, shard: u16) -> Vec<Command<M, E>> {
        if !self.pending[usize::from(shard)].swap(false, Ordering::AcqRel) {
            return Vec::new();
        }
        std::mem::take(&mut *lock(&self.inboxes[usize::from(shard)]))
    }

    fn shard_of(&self, id: ActorId) -> Option<u16> {
        lock(&self.directory).locate(id).map(|slot| slot.shard)
    }

    fn shard_of_route(&self, route: RouteId) -> Option<u16> {
        lock(&self.directory).route_shard(route)
    }

    /// `true` if the actor lives on another node of the cluster.
    fn is_remote(&self, id: ActorId) -> bool {
        let dir = lock(&self.directory);
        dir.home(id).is_some_and(|(node, _)| node != dir.node)
    }

    /// Sends injected messages for actors on other nodes from shard 0: they leave with its next window's messages.
    fn inject_remote(self: &Arc<Self>, messages: Vec<(ActorId, SimTime, M)>) -> Reply<SimTime>
    where
        M: Send + 'static,
        E: 'static,
    {
        let (tx, rx) = reply();
        let control = Arc::clone(self);
        self.submit(
            0,
            Box::new(move |shard, world, window_start| {
                let mut last = window_start;
                for (to, at, msg) in messages {
                    let key = OrderKey {
                        at: at.max(window_start),
                        src: ActorId::WORLD,
                        seq: control.world_seq.fetch_add(1, Ordering::AcqRel),
                        sub: 0,
                    };
                    last = key.at;
                    shard.core.send(world, key, Item::Message { dst: to, msg });
                }
                tx.send(last);
            }),
        );
        rx
    }
}

/// A handle for controlling a running simulation from any thread. See the module documentation.
pub struct Control<M, E> {
    pub(crate) shared: Arc<ControlShared<M, E>>,
}

impl<M, E> Clone for Control<M, E> {
    fn clone(&self) -> Self {
        Self {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl<M, E> std::fmt::Debug for Control<M, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Control").finish_non_exhaustive()
    }
}

impl<M: Clone + Send + 'static, E: Clone + Send + 'static> Control<M, E> {
    /// The clock: change speed, pause, step or stop.
    pub fn clock(&self) -> ClockHandle {
        self.shared.clock.clone()
    }

    /// Reads from an actor: runs `f` on it (on its shard's thread) and replies with the result, or `None` if there
    /// is no actor of type `A` with that ID.
    pub fn query<A, R>(&self, id: ActorId, f: impl FnOnce(&A) -> R + Send + 'static) -> Reply<Option<R>>
    where
        A: Actor<M, E>,
        R: Send + 'static,
    {
        let (tx, rx) = reply();
        match self.shared.shard_of(id) {
            Some(shard) => self.shared.submit(
                shard,
                Box::new(move |shard, world, _| tx.send(shard.actor_mut::<A>(world, id).map(|a| f(a)))),
            ),
            None => tx.send(None),
        }
        rx
    }

    /// Changes an actor: runs `f` on it (on its shard's thread, between events). Replies `false` if there is no
    /// actor of type `A` with that ID.
    pub fn update<A>(&self, id: ActorId, f: impl FnOnce(&mut A) + Send + 'static) -> Reply<bool>
    where
        A: Actor<M, E>,
    {
        let (tx, rx) = reply();
        match self.shared.shard_of(id) {
            Some(shard) => self.shared.submit(
                shard,
                Box::new(move |shard, world, _| tx.send(shard.actor_mut::<A>(world, id).map(f).is_some())),
            ),
            None => tx.send(false),
        }
        rx
    }

    /// Marks an actor as observed (or not). Actors check [`crate::Ctx::is_observed`] to decide whether to emit
    /// detailed events, such as every membrane-potential update. Replies `false` if the actor doesn't exist.
    pub fn observe(&self, id: ActorId, observed: bool) -> Reply<bool> {
        let (tx, rx) = reply();
        match self.shared.shard_of(id) {
            Some(shard) => self.shared.submit(
                shard,
                Box::new(move |shard, world, _| tx.send(shard.set_observed(world, id, observed))),
            ),
            None => tx.send(false),
        }
        rx
    }

    /// Delivers `msg` to `to` at `at` (or at the start of the window being processed, if `at` is already past).
    /// Replies with the delivery time used.
    pub fn inject(&self, to: ActorId, at: SimTime, msg: M) -> Reply<SimTime> {
        if self.shared.is_remote(to) {
            return self.shared.inject_remote(vec![(to, at, msg)]);
        }
        let (tx, rx) = reply();
        let Some(shard) = self.shared.shard_of(to) else {
            drop(tx);
            return rx;
        };
        let control = Arc::clone(&self.shared);
        self.shared.submit(
            shard,
            Box::new(move |shard, _world, window_start| {
                let at = at.max(window_start);
                if let Some(journal) = &control.journal {
                    lock(journal).push(Injected {
                        at,
                        to,
                        msg: msg.clone(),
                    });
                }
                let key = OrderKey {
                    at,
                    src: ActorId::WORLD,
                    seq: control.world_seq.fetch_add(1, Ordering::AcqRel),
                    sub: 0,
                };
                shard.core.queue.push(key, Item::Message { dst: to, msg });
                tx.send(at);
            }),
        );
        rx
    }

    /// Delivers many messages, each at its own time (or at the start of the window being processed, if that time
    /// is already past), with one submission per shard instead of one per message. Messages to unknown actors are
    /// dropped. Replies with how many were delivered late (later than asked).
    pub fn inject_batch(&self, messages: Vec<(ActorId, SimTime, M)>) -> Reply<usize> {
        let (tx, rx) = reply();
        let mut per_shard: std::collections::BTreeMap<u16, Vec<(ActorId, SimTime, M)>> = Default::default();
        let mut remote = Vec::new();
        for (to, at, msg) in messages {
            if let Some(shard) = self.shared.shard_of(to) {
                per_shard.entry(shard).or_default().push((to, at, msg));
            } else if self.shared.is_remote(to) {
                remote.push((to, at, msg));
            }
        }
        if !remote.is_empty() {
            drop(self.shared.inject_remote(remote));
        }
        let late = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let remaining = Arc::new(std::sync::atomic::AtomicUsize::new(per_shard.len()));
        let tx = Arc::new(Mutex::new(Some(tx)));
        if per_shard.is_empty()
            && let Some(tx) = lock(&tx).take()
        {
            tx.send(0);
        }
        for (shard, messages) in per_shard {
            let control = Arc::clone(&self.shared);
            let (late, remaining, tx) = (Arc::clone(&late), Arc::clone(&remaining), Arc::clone(&tx));
            self.shared.submit(
                shard,
                Box::new(move |shard, _world, window_start| {
                    for (to, at, msg) in messages {
                        let delivered = at.max(window_start);
                        if delivered > at {
                            late.fetch_add(1, Ordering::Relaxed);
                        }
                        if let Some(journal) = &control.journal {
                            lock(journal).push(Injected {
                                at: delivered,
                                to,
                                msg: msg.clone(),
                            });
                        }
                        let key = OrderKey {
                            at: delivered,
                            src: ActorId::WORLD,
                            seq: control.world_seq.fetch_add(1, Ordering::AcqRel),
                            sub: 0,
                        };
                        shard.core.queue.push(key, Item::Message { dst: to, msg });
                    }
                    if remaining.fetch_sub(1, Ordering::AcqRel) == 1
                        && let Some(tx) = lock(&tx).take()
                    {
                        tx.send(late.load(Ordering::Relaxed));
                    }
                }),
            );
        }
        rx
    }

    /// Delivers `msg` to `to` "now": at the clock's current simulation time plus the ingress delay
    /// ([`crate::RuntimeConfig::ingress_delay`]). This is how live input (a UI sensor, hardware) enters a paced
    /// simulation. Replies with the delivery time used.
    pub fn inject_live(&self, to: ActorId, msg: M) -> Reply<SimTime> {
        let clock = &self.shared.clock;
        let now = clock.horizon().unwrap_or_else(|| clock.sim_time());
        self.inject(to, now.saturating_add(self.shared.ingress_delay), msg)
    }

    /// Changes a route: adds or prunes an edge. Noted at the next window boundary on the route owner's shard, and
    /// made where the edge's target lives (at once if that's the owner's shard, else with that window's messages).
    /// Replies `true` if the route is owned on this node and the target exists (and, when the target is on the
    /// owner's shard: the change was made; for a prune, the edge was found).
    pub fn change_route(&self, route: RouteId, op: TopologyOp) -> Reply<bool> {
        let (tx, rx) = reply();
        match self.shared.shard_of_route(route) {
            Some(shard) => self.shared.submit(
                shard,
                Box::new(move |shard, world, start| {
                    // (Forwarded to the slice's shard at once, or with the window's messages.)
                    let key = tempo_core::OrderKey {
                        at: start,
                        src: ActorId::WORLD,
                        seq: 0,
                        sub: 0,
                    };
                    tx.send(shard.change_route_now(world, key, route, op));
                }),
            ),
            None => tx.send(false),
        }
        rx
    }
}

#[cfg(feature = "serde")]
impl<M, E> Control<M, E>
where
    M: Clone + Send + serde::Serialize + serde::de::DeserializeOwned + 'static,
    E: Clone + Send + 'static,
{
    /// The run's state, taken during a run: every shard saves its part at the start of a window, and the parts are
    /// one consistent state if every shard did so at the same window. While the clock is paused they always do
    /// (every shard waits at the same window start); while it runs they may not, and the reply is then an error
    /// (try again, or pause first). Not on a cluster (its nodes don't wait at the same point): stop the run instead.
    pub fn snapshot(
        &self,
        codec: Arc<crate::SnapshotCodec<M, E>>,
    ) -> Reply<Result<crate::Snapshot, crate::SnapshotError>> {
        type Part = (u16, SimTime, Result<crate::Snapshot, crate::SnapshotError>);
        let (tx, rx) = reply();
        let shards = self.shared.inboxes.len();
        let parts: Arc<Mutex<Vec<Part>>> = Arc::new(Mutex::new(Vec::with_capacity(shards)));
        let tx = Arc::new(Mutex::new(Some(tx)));
        for shard in 0..shards {
            let (parts, tx, codec, control) = (
                Arc::clone(&parts),
                Arc::clone(&tx),
                Arc::clone(&codec),
                Arc::clone(&self.shared),
            );
            self.shared.submit(
                shard as u16,
                Box::new(move |shard, world, window_start| {
                    let part = if world.nodes > 1 {
                        Err(crate::SnapshotError(
                            "a cluster's run can't be saved while it runs or is paused: stop it first".into(),
                        ))
                    } else {
                        shard.snapshot_part(world, &codec)
                    };
                    let mut all = lock(&parts);
                    all.push((shard.core.shard, window_start, part));
                    if all.len() < shards {
                        return;
                    }
                    let mut all = std::mem::take(&mut *all);
                    all.sort_by_key(|p| p.0);
                    let start = all[0].1;
                    let result = if all.iter().any(|p| p.1 != start) {
                        Err(crate::SnapshotError(
                            "the shards were at different points of the run: pause it, then save".into(),
                        ))
                    } else {
                        all.into_iter()
                            .map(|p| p.2)
                            .collect::<Result<Vec<_>, _>>()
                            .map(|parts| {
                                let mut snapshot = crate::Snapshot::merge(parts);
                                snapshot.now = start.as_nanos();
                                snapshot.world_seq = control.world_seq.load(Ordering::Acquire);
                                snapshot
                            })
                    };
                    if let Some(tx) = lock(&tx).take() {
                        tx.send(result);
                    }
                }),
            );
        }
        rx
    }
}
