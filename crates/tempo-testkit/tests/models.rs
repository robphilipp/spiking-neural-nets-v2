//! Correctness and determinism checks on the toy models.

use tempo_runtime::{Jitter, QueueKind, Runtime, RuntimeConfig, SimDuration, SimTime};
use tempo_testkit::models::{lif, pingpong, ring};
use tempo_testkit::trace_hash;

fn config(seed: u64, queue: QueueKind) -> RuntimeConfig {
    RuntimeConfig {
        seed,
        queue,
        trace: true,
    }
}

const WHEEL: QueueKind = QueueKind::Wheel {
    bucket: SimDuration::MICROSECOND,
};

#[test]
fn pingpong_delivers_every_hit() {
    let mut rt = Runtime::new(config(0, WHEEL));
    let (a, b) = pingpong::build(&mut rt, SimDuration::from_micros(3), 1_000);
    assert_eq!(rt.run(), 1_001);
    let hits = |id| rt.actor::<pingpong::Player>(id).unwrap().received;
    assert_eq!(hits(a) + hits(b), 1_001);
    assert_eq!(rt.now(), SimTime::from_start(SimDuration::from_micros(3_000)));
}

#[test]
fn ring_arrival_times_are_exact() {
    // Irregular microsecond and sub-microsecond hop delays.
    let delays: Vec<SimDuration> = (0..17).map(|i| SimDuration::from_nanos(1_000 + i * 337)).collect();
    let laps = 50;
    let mut rt = Runtime::new(config(0, WHEEL));
    rt.buffer_events();
    let nodes = ring::build(&mut rt, &delays, 1, laps * delays.len() as u64, true);
    rt.run();
    let events = rt.take_events();
    assert_eq!(events.len() as u64, laps * delays.len() as u64 + 1);
    let mut expected = SimTime::ZERO;
    for (i, e) in events.iter().enumerate() {
        assert_eq!(e.at, expected, "hop {i}");
        assert_eq!(e.src, nodes[i % nodes.len()]);
        expected += delays[i % delays.len()];
    }
}

fn run_lif(seed: u64, queue: QueueKind, jitter: Jitter) -> (u64, usize) {
    let mut rt = Runtime::new(config(seed, queue));
    rt.buffer_events();
    lif::build(
        &mut rt,
        &lif::LifConfig {
            neurons: 300,
            synapses_per_neuron: 30,
            input_until: SimTime::from_start(SimDuration::from_millis(50)),
            jitter,
            ..lif::LifConfig::default()
        },
    );
    rt.run_until(SimTime::from_start(SimDuration::from_millis(60)));
    let spikes = rt.take_events().len();
    (trace_hash(&rt.take_trace()), spikes)
}

#[test]
fn lif_network_is_active_and_deterministic() {
    let (hash, spikes) = run_lif(1, WHEEL, Jitter::None);
    assert!(spikes > 100, "network should be active, got {spikes} spikes");
    assert_eq!(run_lif(1, WHEEL, Jitter::None), (hash, spikes), "same seed, same run");
    assert_ne!(run_lif(2, WHEEL, Jitter::None).0, hash, "different seed, different run");
}

#[test]
fn queue_implementation_does_not_change_results() {
    assert_eq!(
        run_lif(3, WHEEL, Jitter::None),
        run_lif(3, QueueKind::Heap, Jitter::None)
    );
    let coarse = QueueKind::Wheel {
        bucket: SimDuration::from_micros(37),
    };
    assert_eq!(run_lif(3, WHEEL, Jitter::None), run_lif(3, coarse, Jitter::None));
}

#[test]
fn jitter_is_reproducible_and_changes_the_run() {
    let jitter = Jitter::Normal {
        sigma: SimDuration::from_micros(20),
    };
    let jittered = run_lif(4, WHEEL, jitter);
    assert_eq!(run_lif(4, WHEEL, jitter), jittered);
    assert_ne!(run_lif(4, WHEEL, Jitter::None).0, jittered.0);
}
