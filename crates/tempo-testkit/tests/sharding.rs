//! Phase 2: parallel shards and the live clock.

use std::time::{Duration, Instant};

use tempo_runtime::{Actor, ActorId, Ctx, Jitter, Pacing, Runtime, RuntimeConfig, SimDuration, SimTime};
use tempo_testkit::models::{lif, ring};
use tempo_testkit::trace_hash;

const SHARD_COUNTS: [usize; 6] = [1, 2, 3, 4, 8, 16];

fn ms(v: u64) -> SimTime {
    SimTime::from_start(SimDuration::from_millis(v))
}

fn lif_config(jitter: Jitter) -> lif::LifConfig {
    lif::LifConfig {
        neurons: 400,
        synapses_per_neuron: 30,
        input_until: ms(40),
        jitter,
        ..lif::LifConfig::default()
    }
}

/// Runs the LIF network on `shards` shards; returns (trace hash, spikes, lookahead violations).
fn run_lif(shards: usize, jitter: Jitter) -> (u64, Vec<(SimTime, ActorId)>, u64) {
    let network = lif_config(jitter);
    let mut rt = Runtime::new(RuntimeConfig {
        seed: 11,
        trace: true,
        shards,
        min_remote_delay: Some(network.input_delay),
        ..RuntimeConfig::default()
    });
    rt.buffer_events();
    lif::build(&mut rt, &network);
    rt.run_until(ms(50));
    let spikes = rt.take_events().into_iter().map(|e| (e.at, e.src)).collect();
    (trace_hash(&rt.take_trace()), spikes, rt.stats().lookahead_violations)
}

#[test]
fn lif_results_are_identical_for_any_shard_count() {
    let (hash, spikes, violations) = run_lif(1, Jitter::None);
    assert!(spikes.len() > 100, "network should be active: {} spikes", spikes.len());
    assert_eq!(violations, 0);
    for shards in &SHARD_COUNTS[1..] {
        let (h, s, v) = run_lif(*shards, Jitter::None);
        assert_eq!(v, 0, "{shards} shards: lookahead violations");
        assert_eq!(s, spikes, "{shards} shards: spikes differ");
        assert_eq!(h, hash, "{shards} shards: trace differs");
    }
}

#[test]
fn bounded_jitter_keeps_results_identical() {
    let jitter = Jitter::Uniform {
        half_width: SimDuration::from_micros(30),
    };
    let reference = run_lif(1, jitter);
    for shards in [2, 4, 8] {
        assert_eq!(run_lif(shards, jitter), reference, "{shards} shards");
    }
}

#[test]
fn ring_timing_is_exact_across_shards() {
    let delays: Vec<SimDuration> = (0..37).map(|i| SimDuration::from_nanos(1_000 + i * 337)).collect();
    let run = |shards| {
        let mut rt = Runtime::new(RuntimeConfig {
            trace: true,
            shards,
            min_remote_delay: Some(SimDuration::from_micros(1)),
            ..RuntimeConfig::default()
        });
        rt.buffer_events();
        ring::build(&mut rt, &delays, 5, 400, true);
        rt.run();
        assert_eq!(rt.stats().lookahead_violations, 0);
        let events: Vec<_> = rt.take_events().into_iter().map(|e| (e.at, e.src, e.event)).collect();
        (trace_hash(&rt.take_trace()), events)
    };
    let reference = run(1);
    for shards in [2, 5, 16] {
        assert_eq!(run(shards), reference, "{shards} shards");
    }
}

/// Forwards to `next` after `delay`, `hops` times.
struct Relay {
    next: ActorId,
    delay: SimDuration,
}

impl Actor<u32, SimTime> for Relay {
    fn on_message(&mut self, ctx: &mut Ctx<'_, u32, SimTime>, _from: ActorId, hops: u32) {
        ctx.emit(ctx.now());
        if hops > 0 {
            ctx.send_after(self.next, self.delay, hops - 1);
        }
    }
}

#[test]
fn messages_shorter_than_the_lookahead_are_delivered_late_and_counted() {
    // Two relays on different shards; the declared minimum (50 µs) is longer than the real delay (10 µs).
    let mut rt = Runtime::new(RuntimeConfig {
        shards: 2,
        partitions: 2,
        min_remote_delay: Some(SimDuration::from_micros(50)),
        ..RuntimeConfig::default()
    });
    rt.buffer_events();
    let relay = |rt: &mut Runtime<u32, SimTime>| {
        rt.spawn(Relay {
            next: ActorId::WORLD,
            delay: SimDuration::from_micros(10),
        })
    };
    let a = relay(&mut rt);
    let b = relay(&mut rt);
    rt.actor_mut::<Relay>(a).unwrap().next = b;
    rt.actor_mut::<Relay>(b).unwrap().next = a;
    rt.inject(a, SimTime::ZERO, 4);
    rt.run();
    let stats = rt.stats();
    assert_eq!(stats.lookahead_violations, 4);
    assert_eq!(stats.max_lookahead_violation, SimDuration::from_micros(40));
    // Each hop lands at the start of the next 50 µs window instead of 10 µs later.
    let times: Vec<u64> = rt.take_events().iter().map(|e| e.event.as_nanos() / 1_000).collect();
    assert_eq!(times, [0, 50, 100, 150, 200]);
}

#[test]
fn single_stepping_matches_a_full_run() {
    let network = lif_config(Jitter::None);
    let config = RuntimeConfig {
        seed: 3,
        trace: true,
        shards: 4,
        min_remote_delay: Some(network.input_delay),
        ..RuntimeConfig::default()
    };
    let mut stepped = Runtime::new(config.clone());
    lif::build(&mut stepped, &network);
    while stepped.next_event_time().is_some_and(|t| t < ms(20)) {
        stepped.step();
    }
    let mut full = Runtime::new(config);
    lif::build(&mut full, &network);
    full.run_until(ms(20));
    assert_eq!(trace_hash(&stepped.take_trace()), trace_hash(&full.take_trace()));
}

#[test]
#[should_panic(expected = "belongs to")]
fn a_panic_in_one_shard_propagates_instead_of_hanging() {
    #[derive(Default)]
    struct Thief(Option<tempo_runtime::RouteId>);
    impl Actor<u32, SimTime> for Thief {
        fn on_message(&mut self, ctx: &mut Ctx<'_, u32, SimTime>, _: ActorId, _: u32) {
            ctx.fanout(self.0.unwrap(), 0);
        }
    }
    let mut rt: Runtime<u32, SimTime> = Runtime::new(RuntimeConfig {
        shards: 4,
        ..RuntimeConfig::default()
    });
    let owner = rt.spawn(Thief::default());
    let thief = rt.spawn(Thief::default());
    let route = rt.add_route(owner, vec![], Jitter::None);
    rt.actor_mut::<Thief>(thief).unwrap().0 = Some(route);
    rt.inject(thief, ms(1), 0);
    rt.run();
}

/// A runtime with one actor that ticks every millisecond, paced in real time.
fn ticking_runtime(time_factor: f64) -> Runtime<u32, SimTime> {
    struct Ticker;
    impl Actor<u32, SimTime> for Ticker {
        fn on_start(&mut self, ctx: &mut Ctx<'_, u32, SimTime>) {
            ctx.schedule_self(SimDuration::from_millis(1), 0);
        }
        fn on_message(&mut self, ctx: &mut Ctx<'_, u32, SimTime>, _: ActorId, _: u32) {
            ctx.emit(ctx.now());
            ctx.schedule_self(SimDuration::from_millis(1), 0);
        }
    }
    let mut rt = Runtime::new(RuntimeConfig {
        shards: 2,
        pacing: Pacing::RealTime { time_factor },
        ..RuntimeConfig::default()
    });
    rt.buffer_events();
    rt.spawn(Ticker);
    rt
}

#[test]
fn real_time_pacing_tracks_the_wall_clock() {
    let mut rt = ticking_runtime(1.0);
    let start = Instant::now();
    rt.run_until(ms(60));
    let wall = start.elapsed();
    assert!(wall >= Duration::from_millis(59), "finished too early: {wall:?}");
    assert!(wall < Duration::from_millis(150), "finished too late: {wall:?}");
    assert_eq!(rt.take_events().len(), 59);
    let pacing = rt.pacing_stats();
    assert!(pacing.windows >= 59);
    assert!(pacing.lateness.quantile(0.5) < Duration::from_millis(2), "{pacing:?}");
}

#[test]
fn the_clock_can_be_slowed_paused_stepped_and_stopped_while_running() {
    let mut rt = ticking_runtime(1.0);
    let clock = rt.clock();
    std::thread::scope(|s| {
        let runner = s.spawn(|| {
            rt.run_until(SimTime::MAX);
            rt
        });
        std::thread::sleep(Duration::from_millis(30));

        // Slow down 10×: simulation time continues from where it was, but advances 10× slower.
        let before = clock.horizon().unwrap();
        clock.set_time_factor(10.0);
        std::thread::sleep(Duration::from_millis(50));
        let advanced = clock.horizon().unwrap() - before;
        assert!(
            advanced < SimDuration::from_millis(12),
            "advanced {advanced} in 50 ms at 10×"
        );

        // Pause: simulation time stops.
        clock.pause();
        let paused_at = clock.horizon().unwrap();
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(clock.horizon().unwrap(), paused_at);

        // Step 5 ms while paused, then stop.
        clock.step(SimDuration::from_millis(5));
        std::thread::sleep(Duration::from_millis(20));
        assert!(clock.sim_time() >= paused_at, "progress should follow the step");
        clock.stop();
        let mut rt = runner.join().unwrap();

        let ticks: Vec<SimTime> = rt.take_events().into_iter().map(|e| e.event).collect();
        let last = *ticks.last().unwrap();
        assert!(
            last <= paused_at + SimDuration::from_millis(5),
            "ran past the step: {last}"
        );
        assert!(last >= paused_at, "didn't run the step: {last} < {paused_at}");
        // Ticks stayed regular through all the speed changes: no gaps, no duplicates.
        for (i, t) in ticks.iter().enumerate() {
            assert_eq!(*t, ms(i as u64 + 1));
        }
        // The runtime continues from where it stopped.
        let now = rt.now();
        clock.set_as_fast_as_possible();
        rt.run_until(now + SimDuration::from_millis(10));
        assert_eq!(rt.take_events().len(), 10);
    });
}
