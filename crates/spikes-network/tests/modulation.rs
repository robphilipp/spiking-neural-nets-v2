//! Neuromodulation in running networks.

use spikes_network::{BuildOptions, Event, ModulatorMsg, NeuronActor, NeuronMsg, Runtime, build};
use tempo_runtime::{RuntimeConfig, SimDuration, SimTime};

fn ms(v: f64) -> SimTime {
    SimTime::from_start(SimDuration::from_millis_f64(v))
}

fn runtime(shards: usize) -> Runtime {
    let mut rt = Runtime::new(RuntimeConfig {
        seed: 1,
        shards,
        partitions: 4,
        min_remote_delay: Some(SimDuration::from_millis(1)),
        ..RuntimeConfig::default()
    });
    rt.buffer_events();
    rt
}

const REWARD: &str = "(
synapse_types=[
    (name=stdp_hard, release=(), learning=stdp_hard),
],
neurons=[
    (name=pre, threshold=5 mV, membrane_noise=0 mV, conduction_speed=10 m/s, position=[0, 0, 0]),
    (name=post, threshold=5 mV, membrane_noise=0 mV, receptors=[(modulator=dopamine, eligibility_time=1 s, learning_rate=1)], position=[100, 0, 0]),
],
connections=[
    (from=pre, to=post, type=stdp_hard, weight=1),
],
learning_rules=[
    (name=stdp_hard, rule=stdp_hard, depression=0.01, depression_time=10 ms, potentiation=0.02, potentiation_time=10 ms),
],
modulators=[
    (name=dopamine, clearance_time=200 ms, latency=1 ms),
],
)";

/// Pairs pre before post, optionally releases dopamine at 100 ms, then fires pre again at 600 ms so the synapse
/// catches up. Returns the final weight.
fn reward_run(shards: usize, dopamine: bool) -> f32 {
    let mut rt = runtime(shards);
    let description = spikes_dna::parse(REWARD).unwrap();
    let net = build(&mut rt, &description, &BuildOptions::default());
    let (pre, post) = (net.neuron("pre").unwrap(), net.neuron("post").unwrap());
    let site = net.add_release_site(&mut rt, [50.0, 0.0, 0.0]);
    let da = net.modulator("dopamine").unwrap();
    rt.inject(pre.id(), ms(0.0), NeuronMsg::External { mv: 10.0 }.into()); // pre spikes at 2 ms (rise)
    rt.inject(post.id(), ms(3.0), NeuronMsg::External { mv: 10.0 }.into()); // post fires just after
    if dopamine {
        rt.inject(
            site.id(),
            ms(100.0),
            ModulatorMsg::Release { kind: da, amount: 1.0 }.into(),
        );
    }
    rt.inject(pre.id(), ms(600.0), NeuronMsg::External { mv: 10.0 }.into());
    rt.run_until(ms(700.0));
    assert_eq!(rt.stats().lookahead_violations, 0);
    rt.actor::<NeuronActor>(post.id()).unwrap().synapses[0].weight
}

#[test]
fn dopamine_turns_a_marked_synapse_into_learning() {
    assert_eq!(reward_run(1, false), 1.0, "STDP alone only marks the synapse");
    let rewarded = reward_run(1, true);
    assert!(rewarded > 1.0, "dopamine after the pairing strengthens it: {rewarded}");
    assert_eq!(reward_run(2, true), rewarded, "same result on two shards");
}

const DIRECT: &str = "(
neuron_types=[
    (name=neuron, threshold=5 mV, membrane_noise=0 mV, receptors=[(modulator=ach, direct_potential=6 mV)]),
],
neurons=[
    (name=near, type=neuron, position=[50, 0, 0]),
    (name=far, type=neuron, position=[500, 0, 0]),
    (name=deaf, threshold=5 mV, membrane_noise=0 mV, position=[60, 0, 0]),
],
modulators=[
    (name=ach, clearance_time=50 ms, reach=300 µm, speed=1 m/s, latency=1 ms),
],
)";

fn spike_times(rt: &mut Runtime, net: &spikes_network::Network) -> Vec<(String, f64)> {
    rt.take_events()
        .into_iter()
        .filter_map(|e| match e.event {
            Event::Spike { at } => Some((net.name(e.src)?.to_string(), at.since_start().as_millis_f64())),
            _ => None,
        })
        .collect()
}

#[test]
fn a_direct_modulator_fires_neurons_it_reaches_on_arrival() {
    for shards in [1, 2] {
        let mut rt = runtime(shards);
        let net = build(&mut rt, &spikes_dna::parse(DIRECT).unwrap(), &BuildOptions::default());
        let site = net.add_release_site(&mut rt, [0.0, 0.0, 0.0]);
        let ach = net.modulator("ach").unwrap();
        rt.inject(
            site.id(),
            ms(10.0),
            ModulatorMsg::Release { kind: ach, amount: 1.0 }.into(),
        );
        rt.run_until(ms(100.0));
        // Released at 10 ms, latency 1 ms, 50 µm at 1 m/s = 50 µs: arrives at 11.05 ms, 6 mV ≥ 5 mV. "far" is beyond
        // the 300 µm reach; "deaf" has no receptor.
        let spikes = spike_times(&mut rt, &net);
        assert_eq!(spikes.len(), 1, "{shards} shards: {spikes:?}");
        assert_eq!(spikes[0].0, "near");
        assert!((spikes[0].1 - 11.05).abs() < 1e-9, "{}", spikes[0].1);
    }
}

const RELEASING_NEURON: &str = "(
neurons=[
    (name=cholinergic, threshold=5 mV, membrane_noise=0 mV, releases=(modulator=ach, amount=1), position=[0, 0, 0]),
    (name=target, threshold=5 mV, membrane_noise=0 mV, receptors=[(modulator=ach, direct_potential=6 mV)], position=[100, 0, 0]),
],
modulators=[
    (name=ach, clearance_time=50 ms, reach=300 µm, speed=1 m/s, latency=1 ms),
],
)";

#[test]
fn a_neuron_can_release_a_modulator_when_it_spikes() {
    let mut rt = runtime(1);
    let net = build(
        &mut rt,
        &spikes_dna::parse(RELEASING_NEURON).unwrap(),
        &BuildOptions::default(),
    );
    rt.inject(
        net.neuron("cholinergic").unwrap().id(),
        ms(0.0),
        NeuronMsg::External { mv: 10.0 }.into(),
    );
    rt.run_until(ms(50.0));
    let mut spikes = spike_times(&mut rt, &net);
    spikes.sort_by(|a, b| a.1.total_cmp(&b.1));
    // The cholinergic neuron spikes at 2 ms (its rise time) and releases then: arrival at 2 ms + 1 ms latency +
    // 100 µm at 1 m/s = 3.1 ms.
    assert_eq!(spikes.len(), 2, "{spikes:?}");
    assert_eq!(spikes[0], ("cholinergic".to_string(), 2.0));
    assert_eq!(spikes[1].0, "target");
    assert!((spikes[1].1 - 3.1).abs() < 1e-9, "{}", spikes[1].1);
}
