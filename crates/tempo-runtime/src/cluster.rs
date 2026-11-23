//! Running one simulation over several processes or hosts ("nodes").
//!
//! Every node builds the same network in the same order, so every node has the same directory (which ID block lives
//! where), but each keeps only its own share of the actors: partitions are spread over nodes as contiguous ranges,
//! then over each node's shards (see [`crate::placement`]). A message to an actor on another node goes into a
//! per-node outbox and leaves at the end of the window, through a [`Link`]: the transport and the cross-node window
//! protocol, implemented outside this crate (`tempo-cluster`).
//!
//! With a link, the window barrier spans the cluster: each node's last shard to arrive reports to the link, which
//! returns the next window for everyone (in strict mode, the coordinator decides it from every node's report, and
//! a node only goes on once every message sent to it has arrived). Since event order never depends on where an
//! actor runs, a strict cluster run gives exactly the results of a single-process run.

use std::any::Any;

use tempo_core::{ActorId, OrderKey, SimDuration, SimTime};

use crate::{EdgeIdx, GroupId, RouteId, SpatialBroadcast, TopologyOp};

/// Where this runtime sits in a cluster: its node index and every node's shard count (in node order).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodePlacement {
    /// This node.
    pub node: u32,
    /// How many shards each node runs.
    pub shards_per_node: Vec<usize>,
}

impl NodePlacement {
    /// A single node with `shards` shards (no cluster).
    pub fn single(shards: usize) -> Self {
        Self {
            node: 0,
            shards_per_node: vec![shards],
        }
    }

    /// Number of nodes.
    pub fn nodes(&self) -> u32 {
        self.shards_per_node.len() as u32
    }

    /// The partitions node `node` runs: `[first, end)`.
    pub fn partitions_of(&self, node: u32, partitions: u32) -> (u32, u32) {
        let n = u64::from(self.nodes());
        let p = u64::from(partitions);
        let first = |k: u64| (k * p).div_ceil(n) as u32;
        (first(u64::from(node)), first(u64::from(node) + 1))
    }

    /// The node and shard that run `partition`.
    pub fn home_of(&self, partition: u32, partitions: u32) -> (u32, u16) {
        let node = (u64::from(partition) * u64::from(self.nodes()) / u64::from(partitions)) as u32;
        let (first, end) = self.partitions_of(node, partitions);
        let shards = self.shards_per_node[node as usize] as u64;
        let shard = (u64::from(partition - first) * shards / u64::from(end - first)) as u16;
        (node, shard)
    }
}

/// A broadcast's target, as sent to another node.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum BroadcastTarget {
    /// Every member of a group (see [`crate::Runtime::add_group`]).
    Group(GroupId),
    /// Every positioned actor within a region.
    Region(SpatialBroadcast),
}

/// An event for another node.
#[derive(Debug)]
pub enum RemoteItem<M> {
    /// A message for one actor.
    Message {
        /// The receiving actor.
        dst: ActorId,
        /// The message.
        msg: M,
    },
    /// A message through a route edge.
    Routed {
        /// The receiving actor.
        dst: ActorId,
        /// The receiver's index for the edge.
        edge: EdgeIdx,
        /// The message.
        msg: M,
    },
    /// A broadcast, expanded on the receiving node (for every shard: see [`ALL_SHARDS`]).
    Broadcast {
        /// Who receives it.
        target: BroadcastTarget,
        /// The message.
        msg: M,
    },
    /// An update to field number `field` (in the order fields were added), for every shard.
    Field {
        /// The field's index.
        field: u16,
        /// The update: the field's `Update` type, boxed. A link encodes it with the codec registered for the field.
        update: Box<dyn Any + Send>,
    },
    /// A change to a route owned by an actor on the receiving node.
    Topology {
        /// The route.
        route: RouteId,
        /// The change.
        op: TopologyOp,
    },
}

/// The `shard` of a [`Remote`] that every shard of the receiving node gets (broadcasts and field updates).
pub const ALL_SHARDS: u16 = u16::MAX;

/// An event for another node: which of its shards, the event's order key, and the event.
#[derive(Debug)]
pub struct Remote<M> {
    /// The receiving shard on the destination node, or [`ALL_SHARDS`].
    pub shard: u16,
    /// When, and in what order, it's delivered.
    pub key: OrderKey,
    /// The event.
    pub item: RemoteItem<M>,
}

/// One node's report at the end of a window: its earliest pending event, the lookahead it proposes, and whether it
/// wants the run to stop.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct WindowReport {
    /// The earliest event this node knows of (pending, or just sent elsewhere); `SimTime::MAX` for none.
    pub next: SimTime,
    /// The shortest delay of any edge leaving a shard on this node.
    pub lookahead: SimDuration,
    /// This node's clock asks to stop.
    pub stop: bool,
}

/// The next window, as agreed by every node.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct WindowDecision {
    /// Start of the window.
    pub start: SimTime,
    /// End of the window (exclusive).
    pub end: SimTime,
    /// The agreed lookahead.
    pub lookahead: SimDuration,
    /// End the run.
    pub stop: bool,
}

impl WindowDecision {
    /// Ends the run.
    pub const STOP: Self = Self {
        start: SimTime::MAX,
        end: SimTime::MAX,
        lookahead: SimDuration::MAX,
        stop: true,
    };
}

/// A runtime's connection to the other nodes of its cluster. See the module documentation.
///
/// Calls come from shard threads: `send` from any shard at the end of each window, `receive` from each shard after
/// each barrier, `decide` from one shard per window (the node's last to arrive) while the others wait.
pub trait Link<M>: Send + Sync {
    /// Sends events produced in the window just finished to `node`. They must reach it before `decide` lets the next
    /// window start there.
    fn send(&self, node: u32, events: Vec<Remote<M>>);

    /// Hands `shard` (a shard of this node) every event that has arrived for it, in any order.
    fn receive(&self, shard: u16, sink: &mut dyn FnMut(OrderKey, RemoteItem<M>));

    /// The window barrier across the cluster: given this node's report, returns the next window. On the node that
    /// decides (the coordinator), `decide` turns the cluster's reduced report into the decision, using that node's
    /// clock. While waiting, call `idle` regularly: it applies control commands, so a paused cluster can still be
    /// inspected. Return [`WindowDecision::STOP`] if the cluster has failed.
    fn decide(
        &self,
        report: WindowReport,
        decide: &dyn Fn(WindowReport) -> WindowDecision,
        idle: &mut dyn FnMut(),
    ) -> WindowDecision;
}
