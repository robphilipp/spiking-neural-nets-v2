//! Behavioural tests for the single-threaded runtime.

use rand::RngExt;
use tempo_runtime::{
    Actor, ActorId, Ctx, DeliveryKind, EdgeIdx, Jitter, QueueKind, RouteEdge, RouteId, Runtime, RuntimeConfig,
    SimDuration, SimTime, Timer, TimerToken,
};

fn config(queue: QueueKind) -> RuntimeConfig {
    RuntimeConfig {
        seed: 7,
        queue,
        trace: true,
    }
}

const QUEUES: [QueueKind; 2] = [
    QueueKind::Heap,
    QueueKind::Wheel {
        bucket: SimDuration::MICROSECOND,
    },
];

/// Records `(now, from, message)` for everything it receives.
#[derive(Default)]
struct Recorder {
    seen: Vec<(SimTime, ActorId, u64)>,
    routed: Vec<(SimTime, EdgeIdx, u64)>,
}

impl Actor<u64, ()> for Recorder {
    fn on_message(&mut self, ctx: &mut Ctx<'_, u64, ()>, from: ActorId, msg: u64) {
        self.seen.push((ctx.now(), from, msg));
    }
    fn on_route_message(&mut self, ctx: &mut Ctx<'_, u64, ()>, _from: ActorId, edge: EdgeIdx, msg: u64) {
        self.routed.push((ctx.now(), edge, msg));
    }
}

/// On receiving a delay in nanoseconds, sends it to `target` after exactly that delay.
struct Delayer {
    target: ActorId,
}

impl Actor<u64, ()> for Delayer {
    fn on_message(&mut self, ctx: &mut Ctx<'_, u64, ()>, _from: ActorId, delay_ns: u64) {
        ctx.send_after(self.target, SimDuration::from_nanos(delay_ns), delay_ns);
    }
}

#[test]
fn delays_are_delivered_exactly() {
    for queue in QUEUES {
        let mut rt = Runtime::new(config(queue));
        let rec = rt.spawn(Recorder::default());
        let delayer = rt.spawn(Delayer { target: rec });
        // Delays from 1 ns to 1 hour, including the 1 µs and 10 µs synaptic delays v1 couldn't represent.
        let delays: Vec<u64> = [1, 999, 1_000, 10_000, 12_345, 1_000_000, 3_600_000_000_000]
            .into_iter()
            .chain((0..200).map(|i| (i * 7_919_u64).pow(2) % 3_600_000_000_000))
            .collect();
        let sent_at = SimTime::from_nanos(5_000);
        for &d in &delays {
            rt.inject(delayer, sent_at, d);
        }
        rt.run();
        let got = &rt.actor::<Recorder>(rec).unwrap().seen;
        assert_eq!(got.len(), delays.len());
        for &(at, from, d) in got {
            assert_eq!(from, delayer);
            assert_eq!(at, sent_at + SimDuration::from_nanos(d), "{queue:?}");
        }
    }
}

/// Sends `count` numbered messages to `target`, all at the same time.
struct Burst {
    target: ActorId,
    count: u64,
}

impl Actor<u64, ()> for Burst {
    fn on_message(&mut self, ctx: &mut Ctx<'_, u64, ()>, _from: ActorId, base: u64) {
        for i in 0..self.count {
            ctx.send_after(self.target, SimDuration::from_micros(1), base + i);
        }
    }
}

#[test]
fn ties_are_broken_by_sender_then_sequence() {
    for queue in QUEUES {
        let mut rt = Runtime::new(config(queue));
        let rec = rt.spawn(Recorder::default());
        let a = rt.spawn(Burst { target: rec, count: 3 });
        let b = rt.spawn(Burst { target: rec, count: 3 });
        // Trigger b before a: the arrival order must not matter.
        rt.inject(b, SimTime::ZERO, 100);
        rt.inject(a, SimTime::ZERO, 0);
        rt.run();
        let got: Vec<_> = rt.actor::<Recorder>(rec).unwrap().seen.iter().map(|s| s.2).collect();
        assert!(a < b);
        assert_eq!(got, [0, 1, 2, 100, 101, 102], "{queue:?}");
    }
}

#[derive(Default)]
struct Spiker {
    route: Option<RouteId>,
}

impl Actor<u64, ()> for Spiker {
    fn on_message(&mut self, ctx: &mut Ctx<'_, u64, ()>, _from: ActorId, msg: u64) {
        ctx.fanout(self.route.expect("route set"), msg);
    }
}

#[test]
fn fanout_delivers_each_edge_after_its_own_delay_in_edge_order() {
    for queue in QUEUES {
        let mut rt = Runtime::new(config(queue));
        let x = rt.spawn(Recorder::default());
        let y = rt.spawn(Recorder::default());
        let spiker = rt.spawn(Spiker::default());
        let us = SimDuration::from_micros;
        // Two synapses onto x with the same delay: they must arrive in edge order.
        let edges = vec![
            RouteEdge {
                target: x,
                delay: us(20),
                edge: EdgeIdx(5),
            },
            RouteEdge {
                target: y,
                delay: us(10),
                edge: EdgeIdx(0),
            },
            RouteEdge {
                target: x,
                delay: us(20),
                edge: EdgeIdx(3),
            },
        ];
        let route = rt.add_route(spiker, edges, Jitter::None);
        rt.actor_mut::<Spiker>(spiker).unwrap().route = Some(route);
        rt.inject(spiker, SimTime::ZERO, 42);
        rt.run();
        let t = |us| SimTime::from_start(SimDuration::from_micros(us));
        assert_eq!(rt.actor::<Recorder>(y).unwrap().routed, [(t(10), EdgeIdx(0), 42)]);
        assert_eq!(
            rt.actor::<Recorder>(x).unwrap().routed,
            [(t(20), EdgeIdx(5), 42), (t(20), EdgeIdx(3), 42)]
        );
        assert_eq!(rt.stats().routed_sends, 3);
        assert_eq!(rt.stats().routed_delivered, 3);
    }
}

#[test]
#[should_panic(expected = "belongs to")]
fn only_the_owner_can_use_a_route() {
    let mut rt: Runtime<u64, ()> = Runtime::new(config(QUEUES[1]));
    let owner = rt.spawn(Recorder::default());
    let thief = rt.spawn(Spiker::default());
    let route = rt.add_route(owner, vec![], Jitter::None);
    rt.actor_mut::<Spiker>(thief).unwrap().route = Some(route);
    rt.inject(thief, SimTime::ZERO, 1);
    rt.run();
}

#[derive(Clone)]
enum TickMsg {
    Arm(u64),
    Cancel,
    Tick(TimerToken),
}

/// A re-armable timer: emits the time each live tick fires.
#[derive(Default)]
struct Ticker {
    timer: Timer,
}

impl Actor<TickMsg, SimTime> for Ticker {
    fn on_message(&mut self, ctx: &mut Ctx<'_, TickMsg, SimTime>, _from: ActorId, msg: TickMsg) {
        match msg {
            TickMsg::Arm(delay_us) => {
                let token = self.timer.arm();
                ctx.schedule_self(SimDuration::from_micros(delay_us), TickMsg::Tick(token));
            }
            TickMsg::Cancel => self.timer.cancel(),
            TickMsg::Tick(token) => {
                if self.timer.fire(token) {
                    ctx.emit(ctx.now());
                }
            }
        }
    }
}

#[test]
fn rearming_and_cancelling_timers_suppresses_stale_ticks() {
    let mut rt = Runtime::new(config(QUEUES[1]));
    rt.buffer_events();
    let t = rt.spawn(Ticker::default());
    let at = |us| SimTime::from_start(SimDuration::from_micros(us));
    rt.inject(t, at(0), TickMsg::Arm(100)); // would fire at 100, but is re-armed...
    rt.inject(t, at(10), TickMsg::Arm(30)); // ...to fire at 40
    rt.inject(t, at(200), TickMsg::Arm(50)); // would fire at 250, but is cancelled
    rt.inject(t, at(220), TickMsg::Cancel);
    rt.run();
    let fired: Vec<_> = rt.take_events().into_iter().map(|e| e.event).collect();
    assert_eq!(fired, [at(40)]);
}

/// Emits a few random numbers when poked.
struct Dice;

impl Actor<(), u64> for Dice {
    fn on_message(&mut self, ctx: &mut Ctx<'_, (), u64>, _from: ActorId, _msg: ()) {
        for _ in 0..3 {
            let roll = ctx.rng().random_range(0..1_000_000);
            ctx.emit(roll);
        }
    }
}

fn dice_rolls(seed: u64) -> Vec<(ActorId, u64)> {
    let mut rt = Runtime::new(RuntimeConfig {
        seed,
        ..RuntimeConfig::default()
    });
    rt.buffer_events();
    let dice: Vec<_> = (0..3).map(|_| rt.spawn(Dice)).collect();
    for &d in &dice {
        rt.inject(d, SimTime::ZERO, ());
    }
    rt.run();
    rt.take_events().into_iter().map(|e| (e.src, e.event)).collect()
}

#[test]
fn random_streams_are_per_actor_and_reproducible() {
    let a = dice_rolls(1);
    assert_eq!(a, dice_rolls(1), "same seed, same numbers");
    assert_ne!(a, dice_rolls(2), "different seed, different numbers");
    let first_of = |id: u64| a.iter().find(|(src, _)| src.raw() == id).unwrap().1;
    assert_ne!(first_of(0), first_of(1), "actors get different streams");
}

/// Sends a greeting to `peer` when it starts.
struct Greeter {
    peer: Option<ActorId>,
}

impl Actor<u64, ()> for Greeter {
    fn on_start(&mut self, ctx: &mut Ctx<'_, u64, ()>) {
        if let Some(peer) = self.peer {
            ctx.send(peer, 1);
        }
    }
    fn on_message(&mut self, _: &mut Ctx<'_, u64, ()>, _: ActorId, _: u64) {}
}

#[test]
fn on_start_runs_before_deliveries_and_after_setup() {
    let mut rt = Runtime::new(config(QUEUES[1]));
    let rec = rt.spawn(Recorder::default());
    let greeter = rt.spawn(Greeter { peer: None });
    // Setup after spawning, before running, is visible to on_start.
    rt.actor_mut::<Greeter>(greeter).unwrap().peer = Some(rec);
    rt.run();
    assert_eq!(rt.actor::<Recorder>(rec).unwrap().seen, [(SimTime::ZERO, greeter, 1)]);
    let trace = rt.take_trace();
    assert_eq!(trace[0].kind, DeliveryKind::Start);
    assert_eq!(trace[1].kind, DeliveryKind::Start);
    assert_eq!(trace[2].kind, DeliveryKind::Message);
    assert_eq!(rt.stats().started, 2);
}

#[test]
fn messages_to_unknown_actors_are_counted_and_dropped() {
    let mut rt: Runtime<u64, ()> = Runtime::new(config(QUEUES[1]));
    rt.spawn(Recorder::default());
    rt.inject(ActorId::from_raw(999), SimTime::ZERO, 1);
    rt.inject(ActorId::from_raw(1 << 40), SimTime::ZERO, 1);
    rt.run();
    assert_eq!(rt.stats().dropped_unknown, 2);
    assert_eq!(rt.stats().delivered, 0);
}

#[test]
fn actor_lookup_checks_the_type() {
    let mut rt: Runtime<u64, ()> = Runtime::new(config(QUEUES[1]));
    let rec = rt.spawn(Recorder::default());
    assert!(rt.actor::<Recorder>(rec).is_some());
    assert!(rt.actor::<Spiker>(rec).is_none());
}

#[test]
fn run_until_stops_at_the_window_end() {
    let mut rt = Runtime::new(config(QUEUES[1]));
    let rec = rt.spawn(Recorder::default());
    let at = |us| SimTime::from_start(SimDuration::from_micros(us));
    rt.inject(rec, at(5), 1);
    rt.inject(rec, at(15), 2);
    assert_eq!(rt.run_until(at(10)), 1);
    assert_eq!(rt.now(), at(10));
    assert_eq!(rt.next_event_time(), Some(at(15)));
    rt.inject(rec, at(10), 3); // still allowed: time 10 hasn't been processed
    assert_eq!(rt.run(), 2);
    let got: Vec<_> = rt.actor::<Recorder>(rec).unwrap().seen.iter().map(|s| s.2).collect();
    assert_eq!(got, [1, 3, 2]);
}

#[test]
#[should_panic(expected = "cannot inject into the past")]
fn injecting_into_processed_time_panics() {
    let mut rt: Runtime<u64, ()> = Runtime::new(config(QUEUES[1]));
    let rec = rt.spawn(Recorder::default());
    rt.run_until(SimTime::from_nanos(100));
    rt.inject(rec, SimTime::from_nanos(50), 1);
}
