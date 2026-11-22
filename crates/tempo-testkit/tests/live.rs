//! Phase 3: the control plane, subscriptions, system events, dynamic topology, fields and broadcasts.

use std::time::{Duration, Instant};

use tempo_runtime::{
    Actor, ActorId, Ctx, EdgeIdx, Field, Jitter, Pacing, RouteEdge, RouteId, Runtime, RuntimeConfig, SimDuration,
    SimTime, SpatialBroadcast, SystemEvent, TopologyOp,
};
use tempo_testkit::models::lif;
use tempo_testkit::trace_hash;

fn ms(v: u64) -> SimTime {
    SimTime::from_start(SimDuration::from_millis(v))
}

fn us(v: u64) -> SimDuration {
    SimDuration::from_micros(v)
}

// ---------- control plane ----------

/// Counts messages; ticks itself every millisecond.
#[derive(Default)]
struct Counter {
    ticks: u64,
    pokes: Vec<(SimTime, u64)>,
}

#[derive(Clone, Debug, PartialEq)]
enum Msg {
    Tick,
    Poke(u64),
}

impl Actor<Msg, SimTime> for Counter {
    fn on_start(&mut self, ctx: &mut Ctx<'_, Msg, SimTime>) {
        ctx.schedule_self(SimDuration::from_millis(1), Msg::Tick);
    }
    fn on_message(&mut self, ctx: &mut Ctx<'_, Msg, SimTime>, _: ActorId, msg: Msg) {
        match msg {
            Msg::Tick => {
                self.ticks += 1;
                if ctx.is_observed() {
                    ctx.emit(ctx.now());
                }
                ctx.schedule_self(SimDuration::from_millis(1), Msg::Tick);
            }
            Msg::Poke(n) => self.pokes.push((ctx.now(), n)),
        }
    }
}

fn paced(shards: usize) -> RuntimeConfig {
    RuntimeConfig {
        shards,
        partitions: 4,
        pacing: Pacing::RealTime { time_factor: 1.0 },
        journal: true,
        ..RuntimeConfig::default()
    }
}

#[test]
fn query_update_observe_and_inject_while_running_and_paused() {
    let mut rt: Runtime<Msg, SimTime> = Runtime::new(paced(2));
    let counters: Vec<ActorId> = (0..4).map(|_| rt.spawn(Counter::default())).collect();
    let events = rt.subscribe(1024);
    let control = rt.control();
    let clock = rt.clock();
    std::thread::scope(|s| {
        let runner = s.spawn(|| {
            rt.run();
            rt
        });
        std::thread::sleep(Duration::from_millis(30));

        // Query a running actor.
        let ticks = control
            .query(counters[1], |c: &Counter| c.ticks)
            .wait()
            .unwrap()
            .unwrap();
        assert!((20..=40).contains(&ticks), "ticks after ~30 ms: {ticks}");
        // Wrong type or unknown ID: None.
        assert!(control.query(counters[1], |_: &Nothing| ()).wait().unwrap().is_none());
        assert!(
            control
                .query(ActorId::from_raw(1 << 40), |c: &Counter| c.ticks)
                .wait()
                .unwrap()
                .is_none()
        );

        // Observe one counter: it starts emitting.
        assert!(control.observe(counters[2], true).wait().unwrap());
        std::thread::sleep(Duration::from_millis(20));
        let seen: Vec<_> = events.drain();
        assert!(
            !seen.is_empty() && seen.iter().all(|e| e.src == counters[2]),
            "{seen:?}"
        );

        // Live input is delivered shortly after "now".
        let sent = Instant::now();
        let at = control.inject_live(counters[3], Msg::Poke(7)).wait().unwrap();
        std::thread::sleep(Duration::from_millis(10));
        let pokes = control
            .query(counters[3], |c: &Counter| c.pokes.clone())
            .wait()
            .unwrap()
            .unwrap();
        assert_eq!(pokes, [(at, 7)]);
        assert!(sent.elapsed() < Duration::from_millis(50));

        // While paused, queries and updates still work, and simulation time stands still.
        clock.pause();
        std::thread::sleep(Duration::from_millis(5));
        let before = control
            .query(counters[0], |c: &Counter| c.ticks)
            .wait()
            .unwrap()
            .unwrap();
        assert!(
            control
                .update(counters[0], |c: &mut Counter| c.ticks += 1_000)
                .wait()
                .unwrap()
        );
        std::thread::sleep(Duration::from_millis(20));
        let after = control
            .query(counters[0], |c: &Counter| c.ticks)
            .wait()
            .unwrap()
            .unwrap();
        assert_eq!(after, before + 1_000, "paused: only the update changed it");

        clock.stop();
        let mut rt = runner.join().unwrap();
        let journal = rt.take_journal();
        assert_eq!(journal.len(), 1);
        assert_eq!((journal[0].at, journal[0].to), (at, counters[3]));
    });
}

struct Nothing;
impl Actor<Msg, SimTime> for Nothing {
    fn on_message(&mut self, _: &mut Ctx<'_, Msg, SimTime>, _: ActorId, _: Msg) {}
}

#[test]
fn live_input_is_prompt_even_when_the_next_event_is_far_away() {
    // Nothing is scheduled for a whole second: unpaced shards would jump straight there. Paced, they follow the
    // clock, so a live poke is delivered within milliseconds.
    struct Sleeper(Vec<SimTime>);
    impl Actor<Msg, SimTime> for Sleeper {
        fn on_start(&mut self, ctx: &mut Ctx<'_, Msg, SimTime>) {
            ctx.schedule_self(SimDuration::from_secs(1), Msg::Tick);
        }
        fn on_message(&mut self, ctx: &mut Ctx<'_, Msg, SimTime>, _: ActorId, _: Msg) {
            self.0.push(ctx.now());
        }
    }
    let mut rt: Runtime<Msg, SimTime> = Runtime::new(paced(2));
    let sleeper = rt.spawn(Sleeper(Vec::new()));
    let control = rt.control();
    std::thread::scope(|s| {
        let runner = s.spawn(|| rt.run());
        std::thread::sleep(Duration::from_millis(20));
        let at = control.inject_live(sleeper, Msg::Poke(1)).wait().unwrap();
        assert!(at < ms(100), "stamped near the clock's time, got {at}");
        let start = Instant::now();
        loop {
            let got = control
                .query(sleeper, |s: &Sleeper| s.0.clone())
                .wait()
                .unwrap()
                .unwrap();
            if got == [at] {
                break;
            }
            assert!(
                start.elapsed() < Duration::from_millis(100),
                "poke not delivered: {got:?}"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        control.clock().stop();
        runner.join().unwrap();
    });
}

#[test]
fn batched_injection_delivers_each_message_at_its_time_and_counts_late_ones() {
    let mut rt: Runtime<Msg, SimTime> = Runtime::new(paced(2));
    let counters: Vec<ActorId> = (0..4).map(|_| rt.spawn(Counter::default())).collect();
    let control = rt.control();
    std::thread::scope(|s| {
        let runner = s.spawn(|| rt.run());
        std::thread::sleep(Duration::from_millis(30));
        // Two in the past (late), the rest 50 ms ahead at µs spacing, spread over both shards.
        let base = control.clock().sim_time() + SimDuration::from_millis(50);
        let mut batch = vec![
            (counters[0], ms(1), Msg::Poke(100)),
            (counters[1], ms(2), Msg::Poke(101)),
        ];
        for i in 0..40u64 {
            batch.push((counters[(i % 4) as usize], base + us(i), Msg::Poke(i)));
        }
        let late = control.inject_batch(batch).wait().unwrap();
        assert_eq!(late, 2);
        std::thread::sleep(Duration::from_millis(100));
        control.clock().stop();
        runner.join().unwrap();
    });
    let mut pokes: Vec<(SimTime, u64)> = counters
        .iter()
        .flat_map(|&c| rt.actor::<Counter>(c).unwrap().pokes.clone())
        .collect();
    pokes.retain(|p| p.1 < 100);
    pokes.sort_unstable();
    assert_eq!(pokes.len(), 40);
    for (i, w) in pokes.windows(2).enumerate() {
        assert_eq!(w[1].0 - w[0].0, us(1), "spacing kept exactly at {i}");
    }
}

#[test]
fn a_recorded_live_session_replays_identically() {
    let build = |config: RuntimeConfig| {
        let mut rt: Runtime<Msg, SimTime> = Runtime::new(RuntimeConfig { trace: true, ..config });
        let ids: Vec<ActorId> = (0..3).map(|_| rt.spawn(Counter::default())).collect();
        (rt, ids)
    };
    let end = ms(80);
    let (mut live, ids) = build(paced(2));
    let control = live.control();
    let (live_hash, journal) = std::thread::scope(|s| {
        let runner = s.spawn(|| {
            live.run_until(end);
            live
        });
        for i in 0..5 {
            std::thread::sleep(Duration::from_millis(8));
            control.inject_live(ids[i % 3], Msg::Poke(i as u64)).wait().unwrap();
        }
        let mut live = runner.join().unwrap();
        (trace_hash(&live.take_trace()), live.take_journal())
    });
    assert_eq!(journal.len(), 5);

    // Replay as fast as possible on a different number of shards.
    let (mut replay, ids2) = build(RuntimeConfig {
        shards: 3,
        partitions: 4,
        ..RuntimeConfig::default()
    });
    assert_eq!(ids, ids2);
    for entry in &journal {
        replay.inject(entry.to, entry.at, entry.msg.clone());
    }
    replay.run_until(end);
    assert_eq!(trace_hash(&replay.take_trace()), live_hash);
}

// ---------- subscriptions and system events ----------

#[test]
fn subscribers_never_slow_the_simulation_down() {
    let network = lif::LifConfig {
        neurons: 2_000,
        synapses_per_neuron: 50,
        input_until: ms(100),
        ..lif::LifConfig::default()
    };
    let mut rt = Runtime::new(RuntimeConfig {
        shards: 4,
        min_remote_delay: Some(network.input_delay),
        ..RuntimeConfig::default()
    });
    rt.buffer_events();
    lif::build(&mut rt, &network);
    let fast = rt.subscribe(100_000);
    let slow = rt.subscribe(1); // never read: it will fill up immediately
    let spikes_only = rt.subscribe_filtered(100_000, |e| e.src.raw() % 2 == 0);
    rt.run_until(ms(100));

    let all = rt.take_events();
    let mut streamed = fast.drain();
    streamed.sort_by_key(|e| (e.at, e.src));
    assert_eq!(streamed, all, "a subscriber that keeps up sees every event");
    assert_eq!(fast.dropped(), 0);
    assert!(
        slow.dropped() > 0,
        "the full subscriber loses events instead of blocking"
    );
    let even = spikes_only.drain();
    assert_eq!(even.len(), all.iter().filter(|e| e.src.raw() % 2 == 0).count());
}

#[test]
fn falling_behind_the_clock_raises_a_warning_but_keeps_the_speed() {
    // 2,000 neurons at 100× real time: far more than this machine can do, so it falls behind.
    let network = lif::LifConfig {
        neurons: 2_000,
        synapses_per_neuron: 50,
        input_until: SimTime::MAX,
        ..lif::LifConfig::default()
    };
    let mut rt = Runtime::new(RuntimeConfig {
        shards: 2,
        pacing: Pacing::RealTime { time_factor: 0.01 },
        min_remote_delay: Some(network.input_delay),
        ..RuntimeConfig::default()
    });
    lif::build(&mut rt, &network);
    let notices = rt.system_events();
    rt.run_until(ms(300));
    let warnings: Vec<_> = notices
        .drain()
        .into_iter()
        .filter(|n| matches!(n.event, SystemEvent::CannotKeepUp { .. }))
        .collect();
    assert_eq!(warnings.len(), 1, "warn once, not every window");
    assert!(matches!(warnings[0].event, SystemEvent::CannotKeepUp { time_factor, .. } if time_factor == 0.01));
    assert_eq!(rt.clock().time_factor(), Some(0.01), "the clock is not slowed down");
}

#[test]
fn panicking_actors_are_isolated_and_reported() {
    struct Fragile;
    impl Actor<Msg, SimTime> for Fragile {
        fn on_message(&mut self, _: &mut Ctx<'_, Msg, SimTime>, _: ActorId, msg: Msg) {
            if msg == Msg::Poke(13) {
                panic!("unlucky number");
            }
        }
    }
    let mut rt: Runtime<Msg, SimTime> = Runtime::new(RuntimeConfig {
        shards: 2,
        partitions: 2,
        isolate_panics: true,
        min_remote_delay: Some(us(10)),
        ..RuntimeConfig::default()
    });
    let fragile = rt.spawn(Fragile);
    let counter = rt.spawn(Counter::default());
    rt.inject(fragile, ms(1), Msg::Poke(13));
    rt.inject(fragile, ms(2), Msg::Poke(1));
    rt.run_until(ms(10));
    let failures: Vec<_> = rt
        .recent_system_events()
        .into_iter()
        .filter_map(|n| match n.event {
            SystemEvent::ActorFailed { id, message } => Some((id, message, n.sim_time)),
            _ => None,
        })
        .collect();
    assert_eq!(failures, [(fragile, "unlucky number".to_string(), ms(1))]);
    assert_eq!(rt.stats().dropped_failed, 1, "the message after the panic is dropped");
    assert_eq!(
        rt.actor::<Counter>(counter).unwrap().ticks,
        9,
        "other actors keep running"
    );
}

// ---------- dynamic topology ----------

/// On a `Poke`, fans out on its route. When it receives a routed message, records it; on the third one, prunes
/// the connection it came from (structural plasticity, decided by the receiver).
#[derive(Default)]
struct Plastic {
    out: Option<RouteId>,
    /// The route that feeds this actor, and this actor's edge index on it.
    input: Option<(RouteId, EdgeIdx)>,
    received: Vec<SimTime>,
}

impl Actor<Msg, SimTime> for Plastic {
    fn on_message(&mut self, ctx: &mut Ctx<'_, Msg, SimTime>, _: ActorId, _: Msg) {
        if let Some(route) = self.out {
            ctx.fanout(route, Msg::Tick);
        }
    }
    fn on_route_message(&mut self, ctx: &mut Ctx<'_, Msg, SimTime>, _: ActorId, edge: EdgeIdx, _: Msg) {
        self.received.push(ctx.now());
        if self.received.len() == 3
            && let Some((route, my_edge)) = self.input
        {
            assert_eq!(edge, my_edge);
            ctx.change_route(
                route,
                TopologyOp::Prune {
                    target: ctx.me(),
                    edge: my_edge,
                },
            );
        }
    }
}

fn run_plastic(shards: usize) -> (Vec<Vec<SimTime>>, u64) {
    let mut rt: Runtime<Msg, SimTime> = Runtime::new(RuntimeConfig {
        shards,
        partitions: 4,
        trace: true,
        ..RuntimeConfig::default()
    });
    let source = rt.spawn(Plastic::default());
    let targets: Vec<ActorId> = (0..3).map(|_| rt.spawn(Plastic::default())).collect();
    let edges = targets
        .iter()
        .enumerate()
        .map(|(i, &t)| RouteEdge {
            target: t,
            delay: us(100 + 10 * i as u64),
            edge: EdgeIdx(0),
        })
        .collect();
    let route = rt.add_route(source, edges, Jitter::None);
    rt.actor_mut::<Plastic>(source).unwrap().out = Some(route);
    // Only the first target prunes itself.
    rt.actor_mut::<Plastic>(targets[0]).unwrap().input = Some((route, EdgeIdx(0)));
    for i in 0..6 {
        rt.inject(source, ms(i), Msg::Poke(i));
    }
    rt.run_until(ms(10));
    let received = targets
        .iter()
        .map(|&t| rt.actor::<Plastic>(t).unwrap().received.clone())
        .collect();
    (received, trace_hash(&rt.take_trace()))
}

#[test]
fn actors_can_prune_their_inputs_and_results_stay_identical_across_shards() {
    let (received, hash) = run_plastic(1);
    assert_eq!(received[0].len(), 3, "pruned after the third spike");
    assert_eq!(received[1].len(), 6);
    assert_eq!(received[2].len(), 6);
    for shards in [2, 4] {
        assert_eq!(run_plastic(shards), (received.clone(), hash), "{shards} shards");
    }
}

#[test]
fn adding_a_shorter_connection_lowers_the_lookahead() {
    let mut rt: Runtime<Msg, SimTime> = Runtime::new(RuntimeConfig {
        shards: 2,
        partitions: 2,
        ..RuntimeConfig::default()
    });
    let a = rt.spawn(Plastic::default());
    let b = rt.spawn(Plastic::default());
    let route = rt.add_route(
        a,
        vec![RouteEdge {
            target: b,
            delay: us(500),
            edge: EdgeIdx(0),
        }],
        Jitter::None,
    );
    rt.actor_mut::<Plastic>(a).unwrap().out = Some(route);
    assert_eq!(rt.lookahead(), us(500));
    let control = rt.control();
    let added = control.change_route(
        route,
        TopologyOp::Connect(RouteEdge {
            target: b,
            delay: us(50),
            edge: EdgeIdx(1),
        }),
    );
    rt.inject(a, ms(1), Msg::Poke(0));
    rt.run_until(ms(5));
    assert!(added.wait().unwrap());
    assert_eq!(rt.lookahead(), us(50));
    let changes: Vec<_> = rt
        .recent_system_events()
        .into_iter()
        .filter_map(|n| match n.event {
            SystemEvent::LookaheadChanged { old, new } => Some((old, new)),
            _ => None,
        })
        .collect();
    assert_eq!(changes, [(us(500), us(50))]);
    assert_eq!(rt.stats().lookahead_violations, 0);
    let got = &rt.actor::<Plastic>(b).unwrap().received;
    assert_eq!(got, &[ms(1) + us(50), ms(1) + us(500)]);
}

// ---------- fields and broadcasts ----------

/// A tick-free modulator: a list of releases; the concentration is computed on demand.
#[derive(Clone, Default)]
struct Dopamine {
    releases: Vec<(SimTime, f64)>,
}

impl Field for Dopamine {
    type Update = f64;
    fn apply(&mut self, at: SimTime, amount: f64) {
        self.releases.push((at, amount));
    }
}

impl Dopamine {
    fn level(&self, now: SimTime) -> f64 {
        self.releases
            .iter()
            .filter(|(t, _)| *t <= now)
            .map(|(t, a)| a * (-(now - *t).as_millis_f64() / 10.0).exp())
            .sum()
    }
}

/// Releases dopamine when poked; otherwise reads the level whenever it ticks.
#[derive(Default)]
struct Sensor {
    readings: Vec<(SimTime, f64)>,
}

impl Actor<Msg, SimTime> for Sensor {
    fn on_message(&mut self, ctx: &mut Ctx<'_, Msg, SimTime>, _: ActorId, msg: Msg) {
        match msg {
            Msg::Poke(amount) => ctx.update_field::<Dopamine>(us(100), amount as f64),
            Msg::Tick => {
                let level = ctx.field::<Dopamine>().unwrap().level(ctx.now());
                self.readings.push((ctx.now(), level));
            }
        }
    }
}

fn run_dopamine(shards: usize) -> Vec<Vec<(SimTime, f64)>> {
    let mut rt: Runtime<Msg, SimTime> = Runtime::new(RuntimeConfig {
        shards,
        partitions: 8,
        min_remote_delay: Some(us(100)),
        ..RuntimeConfig::default()
    });
    rt.add_field(Dopamine::default());
    let sensors: Vec<ActorId> = (0..8).map(|_| rt.spawn(Sensor::default())).collect();
    rt.inject(sensors[0], ms(1), Msg::Poke(2));
    for &s in &sensors {
        for t in [1, 2, 5, 20] {
            rt.inject(s, ms(t), Msg::Tick);
        }
    }
    rt.run_until(ms(30));
    sensors
        .iter()
        .map(|&s| rt.actor::<Sensor>(s).unwrap().readings.clone())
        .collect()
}

#[test]
fn fields_are_read_lazily_and_agree_on_every_shard() {
    let readings = run_dopamine(1);
    let release = ms(1) + us(100);
    for sensor in &readings {
        assert_eq!(sensor.len(), 4);
        assert_eq!(sensor[0].1, 0.0, "before the release arrives");
        for &(t, level) in &sensor[1..] {
            let expected = 2.0 * (-(t - release).as_millis_f64() / 10.0).exp();
            assert!((level - expected).abs() < 1e-12, "{t}: {level} vs {expected}");
        }
    }
    for shards in [2, 4, 8] {
        assert_eq!(run_dopamine(shards), readings, "{shards} shards");
    }
}

/// Records when broadcasts reach it.
#[derive(Default)]
struct Listener(Vec<SimTime>);

impl Actor<Msg, SimTime> for Listener {
    fn on_message(&mut self, ctx: &mut Ctx<'_, Msg, SimTime>, _: ActorId, msg: Msg) {
        match msg {
            Msg::Poke(1) => ctx.broadcast_spatial(
                SpatialBroadcast {
                    center: [0.0, 0.0, 0.0],
                    radius: 25.0,
                    base_delay: us(100),
                    speed: 1e6, // 1 unit per µs
                },
                Msg::Tick,
            ),
            Msg::Poke(2) => {
                let group = tempo_runtime::GroupId(0);
                ctx.multicast(group, us(200), Msg::Tick);
            }
            _ => self.0.push(ctx.now()),
        }
    }
}

fn run_broadcasts(shards: usize) -> Vec<Vec<SimTime>> {
    let mut rt: Runtime<Msg, SimTime> = Runtime::new(RuntimeConfig {
        shards,
        partitions: 8,
        min_remote_delay: Some(us(100)),
        ..RuntimeConfig::default()
    });
    // A line of listeners at x = 0, 10, 20, ..., 50.
    let listeners: Vec<ActorId> = (0..6).map(|_| rt.spawn(Listener::default())).collect();
    for (i, &l) in listeners.iter().enumerate() {
        rt.set_position(l, [10.0 * i as f64, 0.0, 0.0]);
    }
    let group = rt.add_group(&[listeners[1], listeners[4]]);
    assert_eq!(group.0, 0);
    rt.inject(listeners[0], ms(1), Msg::Poke(1));
    rt.inject(listeners[0], ms(3), Msg::Poke(2));
    rt.run_until(ms(10));
    listeners
        .iter()
        .map(|&l| rt.actor::<Listener>(l).unwrap().0.clone())
        .collect()
}

#[test]
fn spatial_and_group_broadcasts_reach_the_right_actors_at_the_right_times() {
    let got = run_broadcasts(1);
    let spatial = |x: u64| ms(1) + us(100) + us(x);
    let group = ms(3) + us(200);
    assert_eq!(got[0], [spatial(0)]);
    assert_eq!(got[1], [spatial(10), group]);
    assert_eq!(got[2], [spatial(20)]);
    assert_eq!(got[3], Vec::<SimTime>::new(), "x = 30 is outside the radius");
    assert_eq!(got[4], [group]);
    assert_eq!(got[5], Vec::<SimTime>::new());
    for shards in [2, 3, 6] {
        assert_eq!(run_broadcasts(shards), got, "{shards} shards");
    }
}
