//! The strict (exact) cross-node window protocol.
//!
//! Each window, every node's last shard to reach its barrier calls [`Link::decide`]. A node sends the coordinator
//! (node 0) its report (earliest pending event, proposed lookahead, stop) together with how many events it has sent
//! to each node so far. The coordinator reduces the reports, decides the next window with its own clock, and tells
//! every node the decision and how many events it should have received by then. A node starts the window only once
//! they have all arrived. So every node processes the same windows, no event is ever late, and a run gives exactly
//! the results of a single process.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::time::Duration;

use serde::Serialize;
use serde::de::DeserializeOwned;
use tempo_core::{OrderKey, SimDuration, SimTime};
use tempo_runtime::Link;
use tempo_runtime::cluster::{ALL_SHARDS, Remote, RemoteItem, WindowDecision, WindowReport};

use crate::transport::{FrameSink, Transport};
use crate::wire::{FieldCodec, Frame, decode_item, encode_event};

/// Called with application frames ([`StrictLink::send_app`]) from another node: `(from, bytes)`.
pub type AppHandler = Arc<dyn Fn(u32, &[u8]) + Send + Sync>;

/// A node's connection to its cluster, for [`tempo_runtime::Runtime::set_link`]. See the module documentation.
pub struct StrictLink<M> {
    inner: Arc<Inner<M>>,
}

impl<M> Clone for StrictLink<M> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<M> std::fmt::Debug for StrictLink<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StrictLink")
            .field("node", &self.inner.node)
            .field("nodes", &self.inner.nodes)
            .finish_non_exhaustive()
    }
}

/// Timeliness of the events from one other node.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Arrivals {
    /// Events received from it.
    pub events: u64,
    /// Of those, events that arrived after their time and were delivered late.
    pub late: u64,
    /// The largest lateness (simulation time).
    pub max_late: Duration,
    /// The 99th percentile of lateness over late events.
    pub p99_late: Duration,
}

/// How a link is doing: traffic and waiting, for a timing panel.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LinkStats {
    /// Events sent to each node (this node's entry is 0).
    pub sent: Vec<u64>,
    /// Events received from all nodes.
    pub received: u64,
    /// Bytes sent to each node.
    pub bytes_sent: Vec<u64>,
    /// Windows agreed with the cluster.
    pub rounds: u64,
    /// Total wall time spent waiting at the cluster barrier (for the other nodes, and for events in flight).
    pub waited: Duration,
    /// Why the cluster failed, if it did.
    pub failure: Option<String>,
    /// How timely the events from each node were (this node's entry is empty).
    pub arrivals: Vec<Arrivals>,
    /// Windows wait for every node (exact), rather than following the clock (best effort).
    pub lockstep: bool,
}

/// Lateness of one node's events.
#[derive(Default)]
struct Timeliness {
    events: u64,
    late: u64,
    lateness: tempo_runtime::LatencyHistogram,
}

/// Events waiting for one shard, with the node each came from.
type Inbound<M> = Mutex<Vec<(u32, OrderKey, RemoteItem<M>)>>;
/// A round's reports, per node: the report and that node's sent counts.
type Reports = Vec<Option<(WindowReport, Vec<u64>)>>;

struct Inner<M> {
    node: u32,
    nodes: u32,
    shards: usize,
    fields: Vec<FieldCodec>,
    transport: OnceLock<Arc<dyn Transport>>,
    app: OnceLock<AppHandler>,
    /// Events that arrived for each local shard.
    inbound: Vec<Inbound<M>>,
    sent: Vec<AtomicU64>,
    bytes_sent: Vec<AtomicU64>,
    received: AtomicU64,
    round: AtomicU64,
    waited_ns: AtomicU64,
    closing: AtomicBool,
    /// Best effort: windows follow this node's clock and don't wait for the others.
    best_effort: AtomicBool,
    timeliness: Mutex<Vec<Timeliness>>,
    state: Mutex<State>,
    changed: Condvar,
}

#[derive(Default)]
struct State {
    /// Coordinator: reports per round, per node (with each node's sent counts).
    reports: HashMap<u64, Reports>,
    /// Other nodes: the coordinator's decision per round, with the events expected by then.
    gos: HashMap<u64, (WindowDecision, u64)>,
    failure: Option<String>,
}

impl<M> Inner<M> {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn fail(&self, error: String) {
        let mut state = self.lock();
        if state.failure.is_none() && !self.closing.load(Ordering::Acquire) {
            state.failure = Some(error);
        }
        drop(state);
        self.changed.notify_all();
    }
}

impl<M: Clone + Send + Serialize + DeserializeOwned + 'static> StrictLink<M> {
    /// The link for node `node` of `nodes`, which runs `shards` shards. `fields` has a codec for every field the
    /// runtime has, in the order they were added (see [`crate::field_codec`]). Connect it to a transport (built
    /// with [`Self::sink`]) with [`Self::attach`].
    pub fn new(node: u32, nodes: u32, shards: usize, fields: Vec<FieldCodec>) -> Self {
        Self {
            inner: Arc::new(Inner {
                node,
                nodes,
                shards,
                fields,
                transport: OnceLock::new(),
                app: OnceLock::new(),
                inbound: (0..shards).map(|_| Mutex::new(Vec::new())).collect(),
                sent: (0..nodes).map(|_| AtomicU64::new(0)).collect(),
                bytes_sent: (0..nodes).map(|_| AtomicU64::new(0)).collect(),
                received: AtomicU64::new(0),
                round: AtomicU64::new(0),
                waited_ns: AtomicU64::new(0),
                closing: AtomicBool::new(false),
                best_effort: AtomicBool::new(false),
                timeliness: Mutex::new((0..nodes).map(|_| Timeliness::default()).collect()),
                state: Mutex::new(State::default()),
                changed: Condvar::new(),
            }),
        }
    }

    /// Where the transport should deliver this node's frames.
    pub fn sink(&self) -> Arc<dyn FrameSink> {
        Arc::new(Sink {
            inner: Arc::clone(&self.inner),
        })
    }

    /// Connects the link to its transport.
    ///
    /// # Panics
    /// If it already has one.
    pub fn attach(&self, transport: Arc<dyn Transport>) {
        assert!(self.inner.transport.set(transport).is_ok(), "link already attached");
    }

    /// Handles application frames from other nodes (see [`Self::send_app`]). Set it before frames arrive.
    pub fn on_app(&self, handler: AppHandler) {
        let _ = self.inner.app.set(handler);
    }

    /// Sends application data (emitted events, control) to node `to`, in order with this node's other frames to it.
    pub fn send_app(&self, to: u32, bytes: Vec<u8>) {
        self.send_frame(to, &Frame::<M>::App(bytes));
    }

    /// This node.
    pub fn node(&self) -> u32 {
        self.inner.node
    }

    /// Number of nodes.
    pub fn nodes(&self) -> u32 {
        self.inner.nodes
    }

    /// Traffic and waiting so far.
    pub fn stats(&self) -> LinkStats {
        let i = &self.inner;
        LinkStats {
            sent: i.sent.iter().map(|c| c.load(Ordering::Relaxed)).collect(),
            received: i.received.load(Ordering::Relaxed),
            bytes_sent: i.bytes_sent.iter().map(|c| c.load(Ordering::Relaxed)).collect(),
            rounds: i.round.load(Ordering::Relaxed),
            waited: Duration::from_nanos(i.waited_ns.load(Ordering::Relaxed)),
            failure: i.lock().failure.clone(),
            arrivals: i
                .timeliness
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .iter()
                .map(|t| Arrivals {
                    events: t.events,
                    late: t.late,
                    max_late: t.lateness.max(),
                    p99_late: if t.late > 0 {
                        t.lateness.quantile(0.99)
                    } else {
                        Duration::ZERO
                    },
                })
                .collect(),
            lockstep: !i.best_effort.load(Ordering::Relaxed),
        }
    }

    /// Chooses how windows are agreed, for the runs from now on (every node must choose the same, between runs):
    /// lockstep (exact: every window waits for every node) or best effort (each node follows its own copy of the
    /// coordinator's clock; events that arrive too late are delivered late and counted).
    pub fn set_lockstep(&self, lockstep: bool) {
        self.inner.best_effort.store(!lockstep, Ordering::Release);
    }

    /// The cluster has failed (a connection broke): why.
    pub fn failure(&self) -> Option<String> {
        self.inner.lock().failure.clone()
    }

    /// The node is leaving the cluster: a run waiting for the others ends, and connections closing from now on
    /// aren't reported as failures.
    pub fn close(&self) {
        self.inner.closing.store(true, Ordering::Release);
        self.inner
            .lock()
            .failure
            .get_or_insert_with(|| "this node left the cluster".into());
        self.inner.changed.notify_all();
    }

    fn send_frame(&self, to: u32, frame: &Frame<M>) {
        let bytes = postcard::to_stdvec(frame).expect("frame encodes");
        self.inner.bytes_sent[to as usize].fetch_add(bytes.len() as u64, Ordering::Relaxed);
        match self.inner.transport.get() {
            Some(t) => t.send(to, bytes),
            None => self.inner.fail("the link has no transport".into()),
        }
    }

    /// Waits (calling `idle`) until `ready` holds or the cluster fails. Returns `false` on failure.
    fn wait(&self, idle: &mut dyn FnMut(), mut ready: impl FnMut(&mut State) -> bool) -> bool {
        let started = std::time::Instant::now();
        let mut state = self.inner.lock();
        let ok = loop {
            if ready(&mut state) {
                break true;
            }
            if state.failure.is_some() {
                break false;
            }
            let (s, _) = self
                .inner
                .changed
                .wait_timeout(state, Duration::from_millis(1))
                .unwrap_or_else(|p| p.into_inner());
            drop(s);
            idle();
            state = self.inner.lock();
        };
        drop(state);
        self.inner
            .waited_ns
            .fetch_add(started.elapsed().as_nanos() as u64, Ordering::Relaxed);
        ok
    }

    /// Waits until every event this node should have by now has arrived.
    fn wait_for_events(&self, expected: u64, idle: &mut dyn FnMut()) -> bool {
        let received = &self.inner.received;
        self.wait(idle, |_| received.load(Ordering::Acquire) >= expected)
    }
}

impl<M: Clone + Send + Serialize + DeserializeOwned + 'static> Link<M> for StrictLink<M> {
    fn send(&self, node: u32, events: Vec<Remote<M>>) {
        if events.is_empty() {
            return;
        }
        let count = events.len() as u64;
        let wire = events
            .into_iter()
            .map(|e| encode_event(e, &self.inner.fields))
            .collect();
        // Count before sending, so a report never claims fewer events than are on their way.
        self.inner.sent[node as usize].fetch_add(count, Ordering::AcqRel);
        self.send_frame(node, &Frame::Events(wire));
    }

    fn receive(&self, shard: u16, sink: &mut dyn FnMut(OrderKey, RemoteItem<M>) -> SimDuration) {
        let events = std::mem::take(
            &mut *self.inner.inbound[usize::from(shard)]
                .lock()
                .unwrap_or_else(|p| p.into_inner()),
        );
        if events.is_empty() {
            return;
        }
        let mut timeliness = self.inner.timeliness.lock().unwrap_or_else(|p| p.into_inner());
        for (from, key, item) in events {
            let late = sink(key, item);
            if let Some(t) = timeliness.get_mut(from as usize) {
                t.events += 1;
                if !late.is_zero() {
                    t.late += 1;
                    t.lateness.record(Duration::from_nanos(late.as_nanos()));
                }
            }
        }
    }

    fn decide(
        &self,
        report: WindowReport,
        decide: &dyn Fn(WindowReport) -> WindowDecision,
        idle: &mut dyn FnMut(),
    ) -> WindowDecision {
        let inner = &self.inner;
        if inner.best_effort.load(Ordering::Acquire) {
            // Each node follows its own copy of the clock; nothing to agree.
            return if inner.lock().failure.is_some() {
                WindowDecision::STOP
            } else {
                decide(report)
            };
        }
        let round = inner.round.fetch_add(1, Ordering::AcqRel);
        let sent: Vec<u64> = inner.sent.iter().map(|c| c.load(Ordering::Acquire)).collect();
        let nodes = inner.nodes as usize;

        if inner.node != 0 {
            self.send_frame(0, &Frame::report(round, report, sent));
            let mut go = None;
            if !self.wait(idle, |state| {
                go = state.gos.remove(&round);
                go.is_some()
            }) {
                return WindowDecision::STOP;
            }
            let (decision, expected) = go.expect("waited for it");
            if !self.wait_for_events(expected, idle) {
                return WindowDecision::STOP;
            }
            return decision;
        }

        // The coordinator: gather every node's report for this round.
        inner.lock().reports.entry(round).or_insert_with(|| vec![None; nodes])[0] = Some((report, sent));
        let mut all = Vec::new();
        if !self.wait(idle, |state| {
            let ready = state.reports.get(&round).is_some_and(|r| r.iter().all(Option::is_some));
            if ready {
                all = state
                    .reports
                    .remove(&round)
                    .expect("present")
                    .into_iter()
                    .flatten()
                    .collect();
            }
            ready
        }) {
            // Tell whoever is still there to stop too.
            for node in 1..inner.nodes {
                self.send_frame(node, &Frame::go(round, WindowDecision::STOP, 0));
            }
            return WindowDecision::STOP;
        }
        let reduced = all.iter().fold(
            WindowReport {
                next: SimTime::MAX,
                lookahead: SimDuration::MAX,
                stop: false,
            },
            |acc, (r, _)| WindowReport {
                next: acc.next.min(r.next),
                lookahead: acc.lookahead.min(r.lookahead),
                stop: acc.stop || r.stop,
            },
        );
        let decision = decide(reduced);
        let expected = |to: usize| all.iter().map(|(_, sent)| sent[to]).sum::<u64>();
        for node in 1..nodes {
            self.send_frame(node as u32, &Frame::go(round, decision, expected(node)));
        }
        if !self.wait_for_events(expected(0), idle) {
            return WindowDecision::STOP;
        }
        decision
    }
}

struct Sink<M> {
    inner: Arc<Inner<M>>,
}

impl<M: Clone + Send + DeserializeOwned + 'static> FrameSink for Sink<M> {
    fn on_frame(&self, from: u32, bytes: &[u8]) {
        let inner = &self.inner;
        let frame: Frame<M> = match postcard::from_bytes(bytes) {
            Ok(f) => f,
            Err(e) => return inner.fail(format!("undecodable frame from node {from}: {e}")),
        };
        match frame {
            Frame::Events(events) => {
                let count = events.len() as u64;
                let mut per_shard: Vec<Vec<(u32, OrderKey, RemoteItem<M>)>> =
                    (0..inner.shards).map(|_| Vec::new()).collect();
                for event in &events {
                    let key = OrderKey::from(&event.key);
                    let shards = if event.shard == ALL_SHARDS {
                        0..inner.shards
                    } else {
                        let s = usize::from(event.shard);
                        s..s + 1
                    };
                    for shard in shards {
                        match (decode_item(&event.item, &inner.fields), per_shard.get_mut(shard)) {
                            (Some(item), Some(list)) => list.push((from, key, item)),
                            _ => return inner.fail(format!("bad event from node {from} for shard {shard}")),
                        }
                    }
                }
                for (shard, list) in per_shard.into_iter().enumerate() {
                    if !list.is_empty() {
                        inner.inbound[shard]
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .extend(list);
                    }
                }
                // Under the lock, so a waiter can't miss the change between checking the count and waiting.
                let state = inner.lock();
                inner.received.fetch_add(count, Ordering::AcqRel);
                drop(state);
                inner.changed.notify_all();
            }
            Frame::Report {
                round,
                next,
                lookahead,
                stop,
                sent,
            } => {
                let nodes = inner.nodes as usize;
                if sent.len() != nodes || from as usize >= nodes {
                    return inner.fail(format!("bad report from node {from}"));
                }
                let report = WindowReport {
                    next: SimTime::from_nanos(next),
                    lookahead: SimDuration::from_nanos(lookahead),
                    stop,
                };
                inner.lock().reports.entry(round).or_insert_with(|| vec![None; nodes])[from as usize] =
                    Some((report, sent));
                inner.changed.notify_all();
            }
            Frame::Go {
                round,
                start,
                end,
                lookahead,
                stop,
                expected,
            } => {
                let decision = WindowDecision {
                    start: SimTime::from_nanos(start),
                    end: SimTime::from_nanos(end),
                    lookahead: SimDuration::from_nanos(lookahead),
                    stop,
                };
                inner.lock().gos.insert(round, (decision, expected));
                inner.changed.notify_all();
            }
            Frame::App(bytes) => {
                if let Some(app) = inner.app.get() {
                    app(from, &bytes);
                }
            }
        }
    }

    fn on_failure(&self, peer: u32, error: String) {
        let _ = peer;
        self.inner.fail(error);
    }
}
