//! What nodes send each other, encoded with `postcard` (compact, varint-based). Messages and events are the
//! application's own types (serde); field updates use the codec registered for their field.

use std::any::Any;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tempo_core::{ActorId, OrderKey, SimDuration, SimTime};
use tempo_runtime::cluster::{BroadcastTarget, Remote, RemoteItem, WindowDecision, WindowReport};
use tempo_runtime::{EdgeIdx, Field, GroupId, RouteEdge, RouteId, SpatialBroadcast, TopologyOp};

/// Encodes and decodes the updates of one field (see [`field_codec`]).
#[derive(Copy, Clone)]
pub struct FieldCodec {
    encode: fn(&dyn Any) -> Vec<u8>,
    decode: fn(&[u8]) -> Option<Box<dyn Any + Send>>,
}

impl std::fmt::Debug for FieldCodec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FieldCodec").finish_non_exhaustive()
    }
}

/// The codec for field `F`'s updates. Register one per field, in the order the fields are added to the runtime.
pub fn field_codec<F: Field>() -> FieldCodec
where
    F::Update: Serialize + DeserializeOwned,
{
    FieldCodec {
        encode: |update| {
            let update = update
                .downcast_ref::<F::Update>()
                .expect("field update matches its codec");
            postcard::to_stdvec(update).expect("field update encodes")
        },
        decode: |bytes| {
            postcard::from_bytes::<F::Update>(bytes)
                .ok()
                .map(|u| Box::new(u) as Box<dyn Any + Send>)
        },
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct WireKey {
    at: u64,
    src: u64,
    seq: u64,
    sub: u32,
}

impl From<OrderKey> for WireKey {
    fn from(k: OrderKey) -> Self {
        Self {
            at: k.at.as_nanos(),
            src: k.src.raw(),
            seq: k.seq,
            sub: k.sub,
        }
    }
}

impl From<&WireKey> for OrderKey {
    fn from(k: &WireKey) -> Self {
        OrderKey {
            at: SimTime::from_nanos(k.at),
            src: ActorId::from_raw(k.src),
            seq: k.seq,
            sub: k.sub,
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) enum WireTarget {
    Group(u32),
    Region {
        center: [f64; 3],
        radius: f64,
        base_delay: u64,
        speed: f64,
    },
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) enum WireOp {
    Connect { target: u64, delay: u64, edge: u32 },
    Prune { target: u64, edge: u32 },
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(bound(serialize = "M: Serialize", deserialize = "M: DeserializeOwned"))]
pub(crate) enum WireItem<M> {
    Message { dst: u64, msg: M },
    Routed { dst: u64, edge: u32, msg: M },
    Broadcast { target: WireTarget, msg: M },
    Field { field: u16, update: Vec<u8> },
    Topology { route: u32, op: WireOp },
    Fanout { route: u32, base: u64, msg: M },
    Slice { route: u32, op: WireOp },
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(bound(serialize = "M: Serialize", deserialize = "M: DeserializeOwned"))]
pub(crate) struct WireEvent<M> {
    pub(crate) shard: u16,
    pub(crate) key: WireKey,
    pub(crate) item: WireItem<M>,
}

/// A frame between two nodes.
#[derive(Serialize, Deserialize, Debug)]
#[serde(bound(serialize = "M: Serialize", deserialize = "M: DeserializeOwned"))]
pub(crate) enum Frame<M> {
    /// Events for the receiver's actors.
    Events(Vec<WireEvent<M>>),
    /// A node's end-of-window report to the coordinator, with how many events it has sent to each node so far.
    Report {
        round: u64,
        next: u64,
        lookahead: u64,
        stop: bool,
        sent: Vec<u64>,
    },
    /// The coordinator's decision for a round, with how many events the receiver must have received from each node
    /// (in node order) before it starts the window. Per node: frames from one node arrive in order, but a fast node's
    /// events for the next window can overtake a slow node's for this one, so a total could be reached too soon.
    Go {
        round: u64,
        start: u64,
        end: u64,
        lookahead: u64,
        stop: bool,
        expected: Vec<u64>,
    },
    /// Application data (emitted events, control) for the receiver's application handler.
    App(Vec<u8>),
}

impl<M> Frame<M> {
    pub(crate) fn report(round: u64, r: WindowReport, sent: Vec<u64>) -> Self {
        Frame::Report {
            round,
            next: r.next.as_nanos(),
            lookahead: r.lookahead.as_nanos(),
            stop: r.stop,
            sent,
        }
    }

    pub(crate) fn go(round: u64, d: WindowDecision, expected: Vec<u64>) -> Self {
        Frame::Go {
            round,
            start: d.start.as_nanos(),
            end: d.end.as_nanos(),
            lookahead: d.lookahead.as_nanos(),
            stop: d.stop,
            expected,
        }
    }
}

pub(crate) fn encode_event<M: Serialize>(event: Remote<M>, fields: &[FieldCodec]) -> WireEvent<M> {
    let item = match event.item {
        RemoteItem::Message { dst, msg } => WireItem::Message { dst: dst.raw(), msg },
        RemoteItem::Routed { dst, edge, msg } => WireItem::Routed {
            dst: dst.raw(),
            edge: edge.0,
            msg,
        },
        RemoteItem::Broadcast { target, msg } => WireItem::Broadcast {
            target: match target {
                BroadcastTarget::Group(g) => WireTarget::Group(g.0),
                BroadcastTarget::Region(r) => WireTarget::Region {
                    center: r.center,
                    radius: r.radius,
                    base_delay: r.base_delay.as_nanos(),
                    speed: r.speed,
                },
            },
            msg,
        },
        RemoteItem::Field { field, update } => {
            let codec = fields
                .get(usize::from(field))
                .unwrap_or_else(|| panic!("no codec registered for field {field}"));
            WireItem::Field {
                field,
                update: (codec.encode)(update.as_ref()),
            }
        }
        RemoteItem::Topology { route, op } => WireItem::Topology {
            route: route.0,
            op: encode_op(op),
        },
        RemoteItem::Fanout { route, base, msg } => WireItem::Fanout {
            route: route.0,
            base: base.as_nanos(),
            msg,
        },
        RemoteItem::Slice { route, op } => WireItem::Slice {
            route: route.0,
            op: encode_op(op),
        },
    };
    WireEvent {
        shard: event.shard,
        key: event.key.into(),
        item,
    }
}

/// Decodes an event's item (once per receiving shard, for broadcasts and field updates).
pub(crate) fn decode_item<M: Clone>(item: &WireItem<M>, fields: &[FieldCodec]) -> Option<RemoteItem<M>> {
    Some(match item {
        WireItem::Message { dst, msg } => RemoteItem::Message {
            dst: ActorId::from_raw(*dst),
            msg: msg.clone(),
        },
        WireItem::Routed { dst, edge, msg } => RemoteItem::Routed {
            dst: ActorId::from_raw(*dst),
            edge: EdgeIdx(*edge),
            msg: msg.clone(),
        },
        WireItem::Broadcast { target, msg } => RemoteItem::Broadcast {
            target: match *target {
                WireTarget::Group(g) => BroadcastTarget::Group(GroupId(g)),
                WireTarget::Region {
                    center,
                    radius,
                    base_delay,
                    speed,
                } => BroadcastTarget::Region(SpatialBroadcast {
                    center,
                    radius,
                    base_delay: SimDuration::from_nanos(base_delay),
                    speed,
                }),
            },
            msg: msg.clone(),
        },
        WireItem::Field { field, update } => RemoteItem::Field {
            field: *field,
            update: (fields.get(usize::from(*field))?.decode)(update)?,
        },
        WireItem::Topology { route, op } => RemoteItem::Topology {
            route: RouteId(*route),
            op: decode_op(op),
        },
        WireItem::Fanout { route, base, msg } => RemoteItem::Fanout {
            route: RouteId(*route),
            base: SimTime::from_nanos(*base),
            msg: msg.clone(),
        },
        WireItem::Slice { route, op } => RemoteItem::Slice {
            route: RouteId(*route),
            op: decode_op(op),
        },
    })
}

fn encode_op(op: TopologyOp) -> WireOp {
    match op {
        TopologyOp::Connect(e) => WireOp::Connect {
            target: e.target.raw(),
            delay: e.delay.as_nanos(),
            edge: e.edge.0,
        },
        TopologyOp::Prune { target, edge } => WireOp::Prune {
            target: target.raw(),
            edge: edge.0,
        },
    }
}

fn decode_op(op: &WireOp) -> TopologyOp {
    match *op {
        WireOp::Connect { target, delay, edge } => TopologyOp::Connect(RouteEdge {
            target: ActorId::from_raw(target),
            delay: SimDuration::from_nanos(delay),
            edge: EdgeIdx(edge),
        }),
        WireOp::Prune { target, edge } => TopologyOp::Prune {
            target: ActorId::from_raw(target),
            edge: EdgeIdx(edge),
        },
    }
}
