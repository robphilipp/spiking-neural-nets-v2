//! A saved network continues exactly as it would have: spikes, weights (learning), modulators and inputs, on any
//! number of shards.

use spikes_network::{BuildOptions, Event, Network, NeuronActor, PoissonInput, Runtime, build, snapshot_codec};
use tempo_runtime::{RuntimeConfig, SimDuration, SimTime, Snapshot};

const NET: &str = "(
MOD=[(nam=ach, clr=50 ms, rad=300 µm, spd=1 m/s, lat=1 ms)],
POP=[
    (nam=exc, cnt=200, csp=1 m/s, REG=(cst=bx, px1=0, px2=0, px3=0, sx1=400 µm, sx2=400 µm, sx3=400 µm)),
    (nam=inh, cnt=50, inh=t, REG=(cst=bx, px1=0, px2=0, px3=0, sx1=400 µm, sx2=400 µm, sx3=400 µm),
        REL=(mod=ach, amt=1)),
],
RUL=[
    (prp=exc, psp=exc, cnt=20, cnw=r0.5:1.5, lrn=stdp_hard),
    (prp=inh, psp=exc, prb=0.2, cnw=1),
    (prp=exc, psp=inh, cnt=10, rad=120 µm, cnw=1),
],
LRN=[(fnc=stdp_hard, ina=0.01, inp=10 ms, exa=0.02, exp=10 ms)],
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
