//! Snapshots: a restored runtime continues exactly as the original would have, on any number of shards.
#![cfg(feature = "serde")]

use std::sync::Arc;

use rand::RngExt;
use serde::{Deserialize, Serialize};
use tempo_runtime::{
    Actor, ActorId, Ctx, EdgeIdx, Field, Jitter, Persist, QueueKind, RouteEdge, RouteId, Runtime, RuntimeConfig,
    SimDuration, SimTime, Snapshot, SnapshotCodec, Timer, TimerToken, TopologyOp,
};

#[derive(Clone, Serialize, Deserialize)]
enum Msg {
    Pulse(u64),
    Tick(TimerToken),
}

/// A running total every actor adds to (a field).
#[derive(Clone, Default, Serialize, Deserialize)]
struct Total(u64);

impl Field for Total {
    type Update = u64;
    fn apply(&mut self, _at: SimTime, update: u64) {
        self.0 = self.0.wrapping_add(update);
    }
}

/// Accumulates what it receives, fans out at random, re-arms a timer, adds edges now and then, and reads the field.
struct Node {
    route: Option<RouteId>,
    peers: Arc<Vec<ActorId>>,
    value: u64,
    timer: Timer,
}

#[derive(Serialize, Deserialize)]
struct NodeState {
    value: u64,
    timer: Timer,
}

const MS: SimDuration = SimDuration::MILLISECOND;

impl Actor<Msg, (u64, u64)> for Node {
    fn on_start(&mut self, ctx: &mut Ctx<'_, Msg, (u64, u64)>) {
        let token = self.timer.arm();
        let delay = MS * ctx.rng().random_range(1..5);
        ctx.schedule_self(delay, Msg::Tick(token));
    }

    fn on_message(&mut self, ctx: &mut Ctx<'_, Msg, (u64, u64)>, _from: ActorId, msg: Msg) {
        let total = ctx.field::<Total>().map_or(0, |t| t.0);
        match msg {
            Msg::Pulse(v) => self.value = self.value.wrapping_mul(31).wrapping_add(v ^ total),
            Msg::Tick(token) => {
                if !self.timer.fire(token) {
                    return;
                }
                let token = self.timer.arm();
                let delay = MS * ctx.rng().random_range(1..5);
                ctx.schedule_self(delay, Msg::Tick(token));
            }
        }
        let route = self.route.expect("route");
        let roll: u32 = ctx.rng().random_range(0..100);
        if roll < 30 {
            ctx.fanout(route, Msg::Pulse(self.value));
        }
        if roll < 2 {
            let raw = ctx.rng().random_range(0..N);
            let edge = RouteEdge {
                target: self.peers[raw as usize],
                delay: MS * 3,
                edge: EdgeIdx(raw),
            };
            ctx.change_route(route, TopologyOp::Connect(edge));
        }
        if roll == 99 {
            ctx.update_field::<Total>(MS * 2, self.value & 0xff);
        }
        ctx.emit((ctx.now().as_nanos(), self.value));
    }
}

impl Persist<Msg, (u64, u64)> for Node {
    type State = NodeState;
    fn save(&self) -> NodeState {
        NodeState {
            value: self.value,
            timer: self.timer.clone(),
        }
    }
    fn restore(&mut self, state: NodeState) {
        self.value = state.value;
        self.timer = state.timer;
    }
}

const N: u32 = 60;

fn codec() -> SnapshotCodec<Msg, (u64, u64)> {
    SnapshotCodec::new().actor::<Node>("node").field::<Total>()
}

/// The same network, on `shards` shards.
fn build(shards: usize) -> Runtime<Msg, (u64, u64)> {
    let mut rt = Runtime::new(RuntimeConfig {
        seed: 11,
        shards,
        partitions: 6,
        queue: QueueKind::Wheel {
            bucket: SimDuration::MICROSECOND,
        },
        ..RuntimeConfig::default()
    });
    rt.add_field(Total::default());
    let ids: Vec<ActorId> = (0..N)
        .map(|_| {
            rt.spawn(Node {
                route: None,
                peers: Arc::default(),
                value: 0,
                timer: Timer::default(),
            })
        })
        .collect();
    let peers = Arc::new(ids.clone());
    for (i, &id) in ids.iter().enumerate() {
        let edges = (1..4)
            .map(|k| {
                let j = (i * 7 + k * 13) % N as usize;
                RouteEdge {
                    target: ids[j],
                    delay: MS * (1 + k as u64),
                    edge: EdgeIdx(i as u32),
                }
            })
            .collect();
        let route = rt.add_route(id, edges, Jitter::None);
        let node = rt.actor_mut::<Node>(id).unwrap();
        node.route = Some(route);
        node.peers = Arc::clone(&peers);
    }
    rt.buffer_events();
    rt
}

fn at(ms: u64) -> SimTime {
    SimTime::from_start(MS * ms)
}

/// Everything emitted, in a canonical order.
fn events(rt: &mut Runtime<Msg, (u64, u64)>) -> Vec<(u64, u64, u64)> {
    let mut e: Vec<_> = rt
        .take_events()
        .into_iter()
        .map(|e| (e.event.0, e.src.raw(), e.event.1))
        .collect();
    e.sort_unstable();
    e
}

fn values(rt: &Runtime<Msg, (u64, u64)>) -> Vec<u64> {
    let first = rt.actor::<Node>(ActorId::from_raw(0)).map(|n| Arc::clone(&n.peers));
    let peers = first.expect("the first actor");
    peers.iter().map(|&id| rt.actor::<Node>(id).unwrap().value).collect()
}

#[test]
fn a_restored_runtime_continues_exactly_on_any_number_of_shards() {
    // The reference: one uninterrupted run.
    let mut reference = build(1);
    reference.run_until(at(200));
    let snapshot: Snapshot = reference.snapshot(&codec()).expect("snapshot");
    let _ = events(&mut reference);
    reference.run_until(at(500));
    let expected = events(&mut reference);
    let expected_values = values(&reference);
    let expected_total = reference.field::<Total>().unwrap().0;
    assert!(
        expected.len() > 1000,
        "the test network is busy ({} events)",
        expected.len()
    );
    assert!(snapshot.pending() > 0);

    // A snapshot survives encoding.
    let bytes = postcard_roundtrip(&snapshot);
    for shards in [1, 2, 3] {
        let mut rt = build(shards);
        rt.restore(&codec(), &bytes).expect("restore");
        assert_eq!(rt.now(), at(200));
        rt.run_until(at(500));
        assert_eq!(events(&mut rt), expected, "{shards} shard(s)");
        assert_eq!(values(&rt), expected_values);
        assert_eq!(rt.field::<Total>().unwrap().0, expected_total);
    }
}

#[test]
fn snapshots_taken_on_several_shards_restore_on_one() {
    let mut two = build(3);
    two.run_until(at(150));
    let snapshot = two.snapshot(&codec()).expect("snapshot");
    two.run_until(at(300));
    let expected = values(&two);

    let mut one = build(1);
    one.restore(&codec(), &snapshot).expect("restore");
    one.run_until(at(300));
    assert_eq!(values(&one), expected);
}

#[test]
fn a_snapshot_doesnt_disturb_the_run() {
    let mut a = build(2);
    let mut b = build(2);
    a.run_until(at(100));
    b.run_until(at(100));
    let _ = a.snapshot(&codec()).expect("snapshot");
    a.run_until(at(250));
    b.run_until(at(250));
    assert_eq!(values(&a), values(&b));
    assert_eq!(events(&mut a), events(&mut b));
}

#[test]
fn restoring_into_a_different_network_fails() {
    let mut rt = build(1);
    rt.run_until(at(10));
    let mut snapshot = rt.snapshot(&codec()).expect("snapshot");
    snapshot.actors[0].kind = "other".into();
    let mut fresh = build(1);
    let err = fresh.restore(&codec(), &snapshot).unwrap_err();
    assert!(err.0.contains("snapshot has a other"), "{err}");
    // A codec that doesn't describe every field is refused.
    let bare = SnapshotCodec::<Msg, (u64, u64)>::new().actor::<Node>("node");
    assert!(rt.snapshot(&bare).is_err());
}

/// Encodes and decodes a snapshot (as saving it to a file would).
fn postcard_roundtrip(s: &Snapshot) -> Snapshot {
    let bytes = postcard::to_stdvec(s).expect("encodes");
    postcard::from_bytes(&bytes).expect("decodes")
}
