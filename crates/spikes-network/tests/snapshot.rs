//! A saved network continues exactly as it would have: spikes, weights (learning), modulators and inputs, on any
//! number of shards.

use spikes_network::{BuildOptions, Event, Network, NeuronActor, PoissonInput, Runtime, build, snapshot_codec};
use tempo_runtime::{RuntimeConfig, SimDuration, SimTime, Snapshot};

const NET: &str = "(
synapse_types=[
    (name=stdp_hard, decay=(time=1500 ms), release=(), learning=stdp_hard),
    (name=fixed, decay=(time=1500 ms), release=()),
],
populations=[
    (name=exc, conduction_speed=1 m/s, count=200, random=(box=(corner=[0, 0, 0], size=[400 µm, 400 µm, 400 µm]))),
    (name=inh, inhibitory=true, releases=(modulator=ach, amount=1), count=50, random=(box=(corner=[0, 0, 0], size=[400 µm, 400 µm, 400 µm]))),
],
connections=[
    (from=exc, to=exc, pattern=fan_in, count=20, type=stdp_hard, weight=uniform(0.5, 1.5)),
    (from=inh, to=exc, pattern=random, probability=0.2, type=fixed, weight=1),
    (from=exc, to=inh, pattern=fan_in, count=10, within=120 µm, type=fixed, weight=1),
],
learning_rules=[
    (name=stdp_hard, rule=stdp_hard, depression=0.01, depression_time=10 ms, potentiation=0.02, potentiation_time=10 ms),
],
modulators=[
    (name=ach, clearance_time=50 ms, reach=300 µm, speed=1 m/s, latency=1 ms),
],
)";

fn ms(v: u64) -> SimTime {
    SimTime::from_start(SimDuration::from_millis(v))
}

fn network(shards: usize) -> (Runtime, Network) {
    let mut rt = Runtime::new(RuntimeConfig {
        seed: 3,
        shards,
        partitions: 4,
        min_remote_delay: Some(SimDuration::from_millis(1)),
        ..RuntimeConfig::default()
    });
    rt.buffer_events();
    let description = spikes_dna::parse(NET).expect("parses");
    let net = build(&mut rt, &description, &BuildOptions::default());
    let targets = net.neurons.iter().take(200).copied().collect();
    rt.spawn(PoissonInput {
        rate_hz: 4000.0,
        targets,
        mv: 12.0,
        delay: SimDuration::from_millis(1),
        start: SimTime::ZERO,
        until: ms(10_000),
    });
    (rt, net)
}

/// Spikes, in a canonical order: (time ns, neuron).
fn spikes(rt: &mut Runtime) -> Vec<(u64, u64)> {
    let mut s: Vec<_> = rt
        .take_events()
        .into_iter()
        .filter_map(|e| match e.event {
            Event::Spike { at } => Some((at.as_nanos(), e.src.raw())),
            _ => None,
        })
        .collect();
    s.sort_unstable();
    s
}

/// Every weight's bits.
fn weights(rt: &Runtime, net: &Network) -> Vec<u32> {
    net.neurons
        .iter()
        .flat_map(|n| {
            rt.actor::<NeuronActor>(n.id())
                .unwrap()
                .synapses
                .iter()
                .map(|s| s.weight.to_bits())
        })
        .collect()
}

#[test]
fn a_saved_network_continues_exactly() {
    let (mut reference, net) = network(1);
    reference.run_until(ms(300));
    let snapshot: Snapshot = reference.snapshot(&snapshot_codec()).expect("snapshot");
    let before = weights(&reference, &net);
    let _ = spikes(&mut reference);
    reference.run_until(ms(600));
    let expected = spikes(&mut reference);
    let expected_weights = weights(&reference, &net);
    assert!(
        expected.len() > 500,
        "the network is active ({} spikes)",
        expected.len()
    );
    assert_ne!(before, expected_weights, "learning changes weights after the snapshot");

    let bytes = postcard::to_stdvec(&snapshot).expect("encodes");
    let snapshot: Snapshot = postcard::from_bytes(&bytes).expect("decodes");
    for shards in [1, 3] {
        let (mut rt, net) = network(shards);
        rt.restore(&snapshot_codec(), &snapshot).expect("restore");
        assert_eq!(weights(&rt, &net), before, "{shards} shard(s): weights restored");
        rt.run_until(ms(600));
        assert_eq!(rt.stats().lookahead_violations, 0);
        assert_eq!(spikes(&mut rt), expected, "{shards} shard(s)");
        assert_eq!(weights(&rt, &net), expected_weights, "{shards} shard(s)");
    }
}
