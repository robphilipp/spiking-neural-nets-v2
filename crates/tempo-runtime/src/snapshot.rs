//! Snapshots: a runtime's whole changing state, saved between runs and restored into a runtime built the same way.
//!
//! A snapshot is keyed by actor ID, not by where actors live: IDs depend only on how the network was built (its
//! partitions), so a snapshot taken on 2 shards restores onto 8, or onto the nodes of a cluster (each node takes the
//! actors and events it holds). What it holds:
//!
//! - every actor's state (through [`Persist`]) and its framework state (random stream, sequence counter, flags);
//! - every pending event (messages, spikes in flight, timers, broadcasts, field updates, route changes);
//! - every route's edges (connections change while running) and every field's state;
//! - the simulation time and the control plane's sequence counter.
//!
//! Restoring needs a runtime with the same actors: build the same network (same description and seed, spawning the
//! same actors in the same order), then [`Runtime::restore`]. Actors' `on_start` handlers don't run again: their
//! scheduled events are in the snapshot.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::marker::PhantomData;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tempo_core::{ActorId, OrderKey, SimTime};
use tempo_queue::TimeQueue;

use crate::broadcast::Target;
use crate::field::Field;
use crate::shard::{Arena, Item, Shard, World};
use crate::{Actor, ActorRng, EdgeIdx, RouteEdge, RouteId, Runtime, TopologyOp};

/// An actor whose state can be saved and restored.
pub trait Persist<M, E>: Actor<M, E> {
    /// What changes about it as it runs.
    type State: Serialize + DeserializeOwned;
    /// Its state now.
    fn save(&self) -> Self::State;
    /// Takes back a saved state (into an actor built the same way).
    fn restore(&mut self, state: Self::State);
}

/// A field whose state can be saved and restored (like [`Persist`] for actors). Its updates are saved as they
/// are, so they must be serializable too.
pub trait PersistField: Field<Update: Serialize + DeserializeOwned> {
    /// What changes about it as it runs.
    type State: Serialize + DeserializeOwned;
    /// Its state now.
    fn save(&self) -> Self::State;
    /// Takes back a saved state (into a field built the same way).
    fn restore(&mut self, state: Self::State);
}

/// Something went wrong taking or restoring a snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotError(pub String);

impl std::fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SnapshotError {}

fn err(message: impl Into<String>) -> SnapshotError {
    SnapshotError(message.into())
}

/// One actor, as saved.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActorRecord {
    /// Its ID.
    pub id: u64,
    /// Its kind's name (as registered in the [`SnapshotCodec`]).
    pub kind: String,
    /// Its state ([`Persist::State`], encoded).
    pub state: Vec<u8>,
    seq: u64,
    rng: ActorRng,
    observed: bool,
    failed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
enum EventItem {
    Message { dst: u64, msg: Vec<u8> },
    Routed { dst: u64, edge: u32, msg: Vec<u8> },
    Broadcast { target: Vec<u8>, msg: Vec<u8> },
    Field { field: u16, update: Vec<u8> },
    Topology { route: u32, op: Vec<u8> },
}

/// One pending event, as saved.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EventRecord {
    at: u64,
    src: u64,
    seq: u64,
    sub: u32,
    item: EventItem,
}

/// A route edge, as saved: (target, delay in ns, receiver's edge index).
type SavedEdge = (u64, u64, u32);

/// A runtime's state (see the module documentation). Several snapshots of a cluster's nodes merge into one with
/// [`Snapshot::merge`].
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Snapshot {
    /// Simulation time: everything before it was processed.
    pub now: u64,
    pub(crate) world_seq: u64,
    /// Every actor.
    pub actors: Vec<ActorRecord>,
    /// Every pending event.
    pub events: Vec<EventRecord>,
    /// Every route's edges.
    routes: Vec<(u32, Vec<SavedEdge>)>,
    /// Every field's state, in the order fields were added.
    fields: Vec<Vec<u8>>,
}

impl Snapshot {
    /// The parts of one snapshot of a cluster's nodes, merged (each node saves the actors, events and routes it
    /// holds; fields, broadcasts and the time are the same on every node, so they're taken once).
    pub fn merge(parts: Vec<Snapshot>) -> Snapshot {
        let mut parts = parts.into_iter();
        let Some(mut all) = parts.next() else {
            return Snapshot::default();
        };
        for p in parts {
            all.actors.extend(p.actors);
            all.events.extend(
                p.events
                    .into_iter()
                    .filter(|e| !matches!(e.item, EventItem::Broadcast { .. } | EventItem::Field { .. })),
            );
            all.routes.extend(p.routes);
            all.world_seq = all.world_seq.max(p.world_seq);
        }
        all.actors.sort_by_key(|a| a.id);
        all.routes.sort_by_key(|r| r.0);
        all
    }

    /// Events pending, for a summary.
    pub fn pending(&self) -> usize {
        self.events.len()
    }
}

type SaveFn = fn(&dyn Any, u32) -> Option<Vec<u8>>;
type LoadFn = fn(&mut dyn Any, u32, &[u8]) -> Result<(), SnapshotError>;
type FieldSaveFn = fn(&dyn Any) -> Vec<u8>;
type FieldLoadFn = fn(&mut dyn Any, &[u8]) -> Result<(), SnapshotError>;
type UpdateEncodeFn = fn(&dyn Any) -> Vec<u8>;
type UpdateDecodeFn = fn(&[u8]) -> Option<Box<dyn Any + Send>>;

struct KindCodec {
    name: String,
    save: SaveFn,
    load: LoadFn,
}

struct FieldCodec {
    save: FieldSaveFn,
    load: FieldLoadFn,
    encode_update: UpdateEncodeFn,
    decode_update: UpdateDecodeFn,
}

/// How each kind of actor and each field is saved: register every kind the runtime has ([`Self::actor`], or
/// [`Self::stateless`] for actors with nothing to save) and every type of field it may have.
pub struct SnapshotCodec<M, E> {
    kinds: HashMap<TypeId, KindCodec>,
    fields: HashMap<TypeId, FieldCodec>,
    _types: PhantomData<fn(M, E)>,
}

impl<M, E> Default for SnapshotCodec<M, E> {
    fn default() -> Self {
        Self {
            kinds: HashMap::new(),
            fields: HashMap::new(),
            _types: PhantomData,
        }
    }
}

impl<M, E> std::fmt::Debug for SnapshotCodec<M, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SnapshotCodec")
            .field("kinds", &self.kinds.values().map(|k| &k.name).collect::<Vec<_>>())
            .field("fields", &self.fields.len())
            .finish()
    }
}

impl<M: 'static, E: 'static> SnapshotCodec<M, E> {
    /// An empty codec.
    pub fn new() -> Self {
        Self::default()
    }

    /// Actors of type `A` are saved with [`Persist`], under `name` (stable across versions of the program).
    pub fn actor<A: Persist<M, E>>(mut self, name: &str) -> Self {
        self.kinds.insert(
            TypeId::of::<A>(),
            KindCodec {
                name: name.to_string(),
                save: |store, index| {
                    let arena = store.downcast_ref::<Arena<A>>()?;
                    let actor = arena.actors.get(index as usize)?;
                    Some(postcard::to_stdvec(&actor.save()).expect("actor state encodes"))
                },
                load: |store, index, bytes| {
                    let arena = store
                        .downcast_mut::<Arena<A>>()
                        .ok_or_else(|| err("actor kind mismatch"))?;
                    let actor = arena
                        .actors
                        .get_mut(index as usize)
                        .ok_or_else(|| err("no such actor"))?;
                    let state = postcard::from_bytes(bytes).map_err(|e| err(format!("actor state: {e}")))?;
                    actor.restore(state);
                    Ok(())
                },
            },
        );
        self
    }

    /// Actors of type `A` have no state of their own (only the framework's: random stream, counters).
    pub fn stateless<A: Actor<M, E>>(mut self, name: &str) -> Self {
        self.kinds.insert(
            TypeId::of::<A>(),
            KindCodec {
                name: name.to_string(),
                save: |_, _| Some(Vec::new()),
                load: |_, _, _| Ok(()),
            },
        );
        self
    }

    /// Fields of type `F` are saved with [`PersistField`] (if the runtime has one).
    pub fn field<F: PersistField>(mut self) -> Self {
        self.fields.insert(
            TypeId::of::<F>(),
            FieldCodec {
                save: |f| {
                    postcard::to_stdvec(&f.downcast_ref::<F>().expect("field type").save()).expect("field encodes")
                },
                load: |f, bytes| {
                    let field = f.downcast_mut::<F>().ok_or_else(|| err("field type mismatch"))?;
                    field.restore(postcard::from_bytes(bytes).map_err(|e| err(format!("field state: {e}")))?);
                    Ok(())
                },
                encode_update: |u| {
                    postcard::to_stdvec(u.downcast_ref::<F::Update>().expect("update type")).expect("encodes")
                },
                decode_update: |bytes| {
                    postcard::from_bytes::<F::Update>(bytes)
                        .ok()
                        .map(|u| Box::new(u) as Box<dyn Any + Send>)
                },
            },
        );
        self
    }
}

/// The codec of each of the runtime's fields, in its order.
fn field_codecs<'c, M, E>(world: &World, codec: &'c SnapshotCodec<M, E>) -> Result<Vec<&'c FieldCodec>, SnapshotError> {
    world
        .field_types
        .iter()
        .enumerate()
        .map(|(i, t)| {
            codec
                .fields
                .get(t)
                .ok_or_else(|| err(format!("field #{i} has no snapshot codec")))
        })
        .collect()
}

impl<M, E> Runtime<M, E> {
    fn field_codecs<'c>(&self, codec: &'c SnapshotCodec<M, E>) -> Result<Vec<&'c FieldCodec>, SnapshotError> {
        field_codecs(&self.world, codec)
    }
}

impl<M, E> Shard<M, E>
where
    M: Clone + Send + Serialize + DeserializeOwned + 'static,
    E: Clone + Send + 'static,
{
    /// This shard's part of a snapshot: its actors, routes and pending events, and (for the node's first shard)
    /// the fields. Broadcasts and field updates are in every shard's queue (and every node's): the first shard of
    /// the first node keeps them. Every other shard must be at the same point (between runs, or waiting at the
    /// same window start).
    pub(crate) fn snapshot_part(
        &mut self,
        world: &World,
        codec: &SnapshotCodec<M, E>,
    ) -> Result<Snapshot, SnapshotError> {
        let fields = field_codecs(world, codec)?;
        let node = world.dir.node;
        let me = self.core.shard;
        let first = me == 0;
        let mut part = Snapshot::default();
        // Actors, block by block.
        for (block, entry) in world.dir.blocks.iter().enumerate() {
            let Some(entry) = entry else { continue };
            if entry.node != node || entry.shard != me {
                continue;
            }
            let type_id = world.kind_types[usize::from(entry.kind)];
            let kind = codec
                .kinds
                .get(&type_id)
                .ok_or_else(|| err(format!("actor kind #{} has no snapshot codec", entry.kind)))?;
            for index in 0..entry.count {
                let id = ActorId::from_parts(
                    tempo_core::BlockId::new(block as u64),
                    u16::try_from(index).expect("block index"),
                );
                let store = &self.stores[entry.store as usize];
                let state = (kind.save)(store.as_any(), index).ok_or_else(|| err(format!("actor {id} is missing")))?;
                let meta = self.stores[entry.store as usize]
                    .meta_mut(index)
                    .ok_or_else(|| err(format!("actor {id} has no metadata")))?;
                part.actors.push(ActorRecord {
                    id: id.raw(),
                    kind: kind.name.clone(),
                    state,
                    seq: meta.seq.peek(),
                    rng: meta.rng.clone(),
                    observed: meta.observed,
                    failed: meta.failed,
                });
            }
        }
        for (route, r) in &self.routes {
            part.routes.push((
                route.0,
                r.edges
                    .iter()
                    .map(|e| (e.target.raw(), e.delay().as_nanos(), e.edge.0))
                    .collect(),
            ));
        }
        if first {
            for (field, c) in self.core.fields.iter().zip(&fields) {
                part.fields.push((c.save)(field.as_any()));
            }
        }
        // Pending events: taken out and put back (all of them, even if one can't be saved).
        let once = first && node == 0;
        let mut items = Vec::with_capacity(self.core.queue.len());
        let mut failure = None;
        while let Some((key, item)) = self.core.queue.pop() {
            let saved = match &item {
                Item::Message { dst, msg } => Some(EventItem::Message {
                    dst: dst.raw(),
                    msg: encode(msg),
                }),
                Item::Routed { dst, edge, msg } => Some(EventItem::Routed {
                    dst: dst.raw(),
                    edge: edge.0,
                    msg: encode(msg),
                }),
                Item::Broadcast { target, msg } if once => Some(EventItem::Broadcast {
                    target: encode_target(target),
                    msg: encode(msg),
                }),
                Item::Field { field, update } if once => match fields.get(usize::from(*field)) {
                    Some(c) => Some(EventItem::Field {
                        field: *field,
                        update: (c.encode_update)(update.as_ref()),
                    }),
                    None => {
                        failure = Some(err(format!("field {field} has no codec")));
                        None
                    }
                },
                Item::Topology { route, op } => Some(EventItem::Topology {
                    route: route.0,
                    op: encode_op(op),
                }),
                Item::Broadcast { .. } | Item::Field { .. } => None,
            };
            if let Some(item) = saved {
                part.events.push(EventRecord {
                    at: key.at.as_nanos(),
                    src: key.src.raw(),
                    seq: key.seq,
                    sub: key.sub,
                    item,
                });
            }
            items.push((key, item));
        }
        for (key, item) in items {
            self.core.queue.push(key, item);
        }
        match failure {
            Some(e) => Err(e),
            None => Ok(part),
        }
    }
}

impl<M, E> Runtime<M, E>
where
    M: Clone + Send + Serialize + DeserializeOwned + 'static,
    E: Clone + Send + 'static,
{
    /// The runtime's state (between runs; while paused, see [`crate::Control::snapshot`]). On a node of a cluster,
    /// the part it holds: merge every node's with [`Snapshot::merge`].
    ///
    /// # Errors
    /// If an actor kind or field has no codec.
    pub fn snapshot(&mut self, codec: &SnapshotCodec<M, E>) -> Result<Snapshot, SnapshotError> {
        let parts = self
            .shards
            .iter_mut()
            .map(|shard| shard.snapshot_part(&self.world, codec))
            .collect::<Result<Vec<_>, _>>()?;
        let mut snapshot = Snapshot::merge(parts);
        snapshot.now = self.now.as_nanos();
        snapshot.world_seq = self.control.world_seq.load(std::sync::atomic::Ordering::Acquire);
        Ok(snapshot)
    }

    /// Takes back a snapshot (between runs), into a runtime built the same way it was: the same actors spawned in
    /// the same order. Any number of shards, or nodes: each takes the actors and events it holds.
    ///
    /// # Errors
    /// If the runtime has different actors, or a kind or field has no codec, or the snapshot is damaged.
    pub fn restore(&mut self, codec: &SnapshotCodec<M, E>, snapshot: &Snapshot) -> Result<(), SnapshotError> {
        let fields = self.field_codecs(codec)?;
        if snapshot.fields.len() != fields.len() {
            return Err(err("the snapshot's fields don't match the runtime's"));
        }
        let node = self.world.dir.node;
        // What the build scheduled is replaced by what the snapshot had pending; nothing starts again.
        for shard in &mut self.shards {
            shard.pending_starts.clear();
            while shard.core.queue.pop().is_some() {}
            shard.core.outbox.iter_mut().for_each(Vec::clear);
            shard.core.remote.iter_mut().for_each(Vec::clear);
        }
        for a in &snapshot.actors {
            let id = ActorId::from_raw(a.id);
            let Some((home, _)) = self.world.dir.home(id) else {
                return Err(err(format!(
                    "actor {id} doesn't exist here: was the network built the same way?"
                )));
            };
            if home != node {
                continue;
            }
            let slot = self.world.locate(id).expect("local actor");
            let type_id = self.world.kind_types[usize::from(slot.kind)];
            let kind = codec
                .kinds
                .get(&type_id)
                .ok_or_else(|| err(format!("actor kind #{} has no snapshot codec", slot.kind)))?;
            if kind.name != a.kind {
                return Err(err(format!(
                    "actor {id} is a {}, the snapshot has a {}",
                    kind.name, a.kind
                )));
            }
            let shard = &mut self.shards[usize::from(slot.shard)];
            (kind.load)(shard.stores[slot.store as usize].as_any_mut(), slot.index, &a.state)?;
            let meta = shard.stores[slot.store as usize]
                .meta_mut(slot.index)
                .ok_or_else(|| err(format!("actor {id} has no metadata")))?;
            meta.seq = tempo_core::SeqCounter::starting_at(a.seq);
            meta.rng = a.rng.clone();
            meta.observed = a.observed;
            meta.failed = a.failed;
        }
        for (route, edges) in &snapshot.routes {
            if let Some(shard) = self.world.dir.route_shard(RouteId(*route))
                && let Some(r) = self.shards[usize::from(shard)].routes.get_mut(&RouteId(*route))
            {
                r.edges = edges
                    .iter()
                    .map(|&(target, delay, edge)| {
                        crate::route::Edge::new(RouteEdge {
                            target: ActorId::from_raw(target),
                            delay: tempo_core::SimDuration::from_nanos(delay),
                            edge: EdgeIdx(edge),
                        })
                    })
                    .collect();
            }
        }
        for shard in &mut self.shards {
            for ((field, c), bytes) in shard.core.fields.iter_mut().zip(&fields).zip(&snapshot.fields) {
                (c.load)(field.as_any_mut(), bytes)?;
            }
        }
        for e in &snapshot.events {
            let key = OrderKey {
                at: SimTime::from_nanos(e.at),
                src: ActorId::from_raw(e.src),
                seq: e.seq,
                sub: e.sub,
            };
            match &e.item {
                EventItem::Message { dst, msg } => {
                    let dst = ActorId::from_raw(*dst);
                    if let Some(slot) = self.world.locate(dst) {
                        let msg = decode(msg)?;
                        self.shards[usize::from(slot.shard)]
                            .core
                            .queue
                            .push(key, Item::Message { dst, msg });
                    }
                }
                EventItem::Routed { dst, edge, msg } => {
                    let dst = ActorId::from_raw(*dst);
                    if let Some(slot) = self.world.locate(dst) {
                        let msg = decode(msg)?;
                        self.shards[usize::from(slot.shard)].core.queue.push(
                            key,
                            Item::Routed {
                                dst,
                                edge: EdgeIdx(*edge),
                                msg,
                            },
                        );
                    }
                }
                EventItem::Broadcast { target, msg } => {
                    let target = decode_target(target)?;
                    let msg: M = decode(msg)?;
                    for shard in &mut self.shards {
                        shard.core.queue.push(
                            key,
                            Item::Broadcast {
                                target: Box::new(target),
                                msg: msg.clone(),
                            },
                        );
                    }
                }
                EventItem::Field { field, update } => {
                    let c = fields
                        .get(usize::from(*field))
                        .ok_or_else(|| err(format!("field {field} has no codec")))?;
                    for shard in &mut self.shards {
                        let update = (c.decode_update)(update).ok_or_else(|| err("bad field update"))?;
                        shard.core.queue.push(key, Item::Field { field: *field, update });
                    }
                }
                EventItem::Topology { route, op } => {
                    if let Some(shard) = self.world.dir.route_shard(RouteId(*route)) {
                        let op = decode_op(op)?;
                        self.shards[usize::from(shard)].core.queue.push(
                            key,
                            Item::Topology {
                                route: RouteId(*route),
                                op: Box::new(op),
                            },
                        );
                    }
                }
            }
        }
        self.now = SimTime::from_nanos(snapshot.now);
        for shard in &mut self.shards {
            shard.core.window_end = self.now;
            shard.last_event = self.now;
        }
        self.control
            .world_seq
            .fetch_max(snapshot.world_seq, std::sync::atomic::Ordering::AcqRel);
        self.needs_shrink = true;
        Ok(())
    }
}

fn encode<T: Serialize>(value: &T) -> Vec<u8> {
    postcard::to_stdvec(value).expect("message encodes")
}

fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, SnapshotError> {
    postcard::from_bytes(bytes).map_err(|e| err(format!("event: {e}")))
}

#[derive(Serialize, Deserialize)]
enum WireTarget {
    Group(u32),
    Region {
        center: [f64; 3],
        radius: f64,
        base_delay: u64,
        speed: f64,
    },
}

fn encode_target(target: &Target) -> Vec<u8> {
    encode(&match *target {
        Target::Group(g) => WireTarget::Group(g.0),
        Target::Region(r) => WireTarget::Region {
            center: r.center,
            radius: r.radius,
            base_delay: r.base_delay.as_nanos(),
            speed: r.speed,
        },
    })
}

fn decode_target(bytes: &[u8]) -> Result<Target, SnapshotError> {
    Ok(match decode::<WireTarget>(bytes)? {
        WireTarget::Group(g) => Target::Group(crate::GroupId(g)),
        WireTarget::Region {
            center,
            radius,
            base_delay,
            speed,
        } => Target::Region(crate::SpatialBroadcast {
            center,
            radius,
            base_delay: tempo_core::SimDuration::from_nanos(base_delay),
            speed,
        }),
    })
}

#[derive(Serialize, Deserialize)]
enum WireOp {
    Connect { target: u64, delay: u64, edge: u32 },
    Prune { target: u64, edge: u32 },
}

fn encode_op(op: &TopologyOp) -> Vec<u8> {
    encode(&match *op {
        TopologyOp::Connect(e) => WireOp::Connect {
            target: e.target.raw(),
            delay: e.delay.as_nanos(),
            edge: e.edge.0,
        },
        TopologyOp::Prune { target, edge } => WireOp::Prune {
            target: target.raw(),
            edge: edge.0,
        },
    })
}

fn decode_op(bytes: &[u8]) -> Result<TopologyOp, SnapshotError> {
    Ok(match decode::<WireOp>(bytes)? {
        WireOp::Connect { target, delay, edge } => TopologyOp::Connect(RouteEdge {
            target: ActorId::from_raw(target),
            delay: tempo_core::SimDuration::from_nanos(delay),
            edge: EdgeIdx(edge),
        }),
        WireOp::Prune { target, edge } => TopologyOp::Prune {
            target: ActorId::from_raw(target),
            edge: EdgeIdx(edge),
        },
    })
}
