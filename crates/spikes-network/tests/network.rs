//! Behaviour of networks built from `.boo` descriptions.

use std::path::Path;

use spikes_dna::NetworkDescription;
use spikes_model::Transition;
use spikes_network::{BuildOptions, Event, Network, NeuronMsg, PeriodicInput, PoissonInput, Runtime, build};
use tempo_runtime::{Emitted, RuntimeConfig, SimDuration, SimTime};

fn ms(v: f64) -> SimTime {
    SimTime::from_start(SimDuration::from_millis_f64(v))
}

fn v1(name: &str) -> NetworkDescription {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-data/networks")
        .join(name);
    spikes_dna::parse(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn runtime(shards: usize) -> Runtime {
    let mut rt = Runtime::new(RuntimeConfig {
        seed: 1,
        shards,
        min_remote_delay: Some(SimDuration::from_millis(1)),
        ..RuntimeConfig::default()
    });
    rt.buffer_events();
    rt
}

fn spikes(events: &[Emitted<Event>], net: &Network) -> Vec<(f64, String)> {
    events
        .iter()
        .filter_map(|e| match e.event {
            Event::Spike { at } => Some((at.since_start().as_millis_f64(), net.name(e.src).unwrap().to_string())),
            _ => None,
        })
        .collect()
}

#[test]
fn the_ring_fires_group_by_group() {
    let mut rt = runtime(1);
    let net = build(&mut rt, &v1("ring.boo"), &BuildOptions::default());
    for n in net.matching(|name| name.starts_with("hidden-10")) {
        rt.inject(n.id(), SimTime::ZERO, NeuronMsg::External { mv: 10.0 }.into());
    }
    rt.run_until(ms(200.0));
    let spikes = spikes(&rt.take_events(), &net);

    // Group 1 fires after its 2 ms rise time; each later group 0.65–1.0 ms after the previous (the 0.5 ms default
    // synaptic delay, µs conduction delays, and the rise from the fourth input crossing the 5 mV threshold).
    let first = |group: char| {
        spikes
            .iter()
            .filter(|(_, n)| n.chars().nth(7) == Some(group))
            .map(|(t, _)| *t)
            .fold(f64::INFINITY, f64::min)
    };
    assert_eq!(first('1'), 2.0);
    for (a, b) in [('1', '2'), ('2', '3'), ('3', '4'), ('4', '5')] {
        let hop = first(b) - first(a);
        assert!((0.65..1.0).contains(&hop), "group {a} → {b}: {hop} ms");
    }
    // One lap takes ~4 ms, inside group 1's 20 ms refractory period, so activity stops after 25 spikes. (v1's
    // ring kept circulating with ~50 ms hops, an artefact of its millisecond-rounded, wall-clock delivery.)
    assert_eq!(spikes.len(), 25);
}

#[test]
fn stdp_potentiates_the_early_input_and_depresses_the_late_one() {
    let mut rt = runtime(1);
    let description = v1("stdp-test.boo");
    let net = build(&mut rt, &description, &BuildOptions::default());
    for name in ["input-1", "input-2"] {
        rt.inject(
            net.neuron(name).unwrap().id(),
            SimTime::ZERO,
            NeuronMsg::External { mv: 10.0 }.into(),
        );
    }
    rt.run_until(ms(100.0));
    let events = rt.take_events();
    let weight = |pre: &str| {
        events
            .iter()
            .find_map(|e| match e.event {
                Event::Weight { pre: p, after, .. } if net.name(p).as_deref() == Some(pre) => Some(f64::from(after)),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no weight change for {pre}"))
    };
    // Input 1 is 10.05 mm away at 1 m/s; input 2 the same distance at 0.5 m/s, so it arrives 10.05 ms after the
    // output fired.
    let distance_mm = (1.0f64 + 100.0).sqrt();
    assert!((weight("input-1") - 1.015).abs() < 1e-5, "{}", weight("input-1"));
    let late_by = 2.0 * distance_mm - distance_mm;
    let expected = 1.0 - 0.035 * (-late_by / 40.0).exp();
    assert!(
        (weight("input-2") - expected).abs() < 1e-5,
        "{} vs {expected}",
        weight("input-2")
    );
}

const BISTABLE: &str = "(
synapse_types=[
    (name=fixed, decay=(time=1500 ms), release=()),
],
neurons=[
    (name=tonic, dynamics=bistable_integrator, tonic_on_threshold=15 mV, tonic_off_threshold=10 mV, tonic_rate=50 Hz, decay_time=250 ms, membrane_noise=0 mV, position=[0, 0, 0]),
    (name=listener, threshold=100 mV, membrane_noise=0 mV, position=[100, 0, 0]),
],
connections=[
    (from=tonic, to=listener, type=fixed, weight=1),
],
)";

#[test]
fn bistable_neurons_fire_tonically_through_runtime_wakeups() {
    let mut rt = runtime(2);
    let net = build(&mut rt, &spikes_dna::parse(BISTABLE).unwrap(), &BuildOptions::default());
    let tonic = net.neuron("tonic").unwrap();
    rt.inject(tonic.id(), SimTime::ZERO, NeuronMsg::External { mv: 20.0 }.into());
    rt.run_until(ms(500.0));
    let events = rt.take_events();
    let transitions: Vec<Transition> = events
        .iter()
        .filter_map(|e| match e.event {
            Event::Transition(t) => Some(t),
            _ => None,
        })
        .collect();
    assert_eq!(transitions, [Transition::StartedTonic, Transition::Rested]);
    // Potential 15 mV decaying with τ = 250 ms stays above 10 mV until t = 250·ln 1.5 ≈ 101 ms: tonic spikes every
    // 20 ms (50 Hz) at 20, 40, …, 100 ms, the last of which sees it below rest. Plus the initial spike.
    let tonic_spikes: Vec<f64> = spikes(&events, &net)
        .into_iter()
        .filter(|(_, n)| n == "tonic")
        .map(|(t, _)| t)
        .collect();
    assert_eq!(tonic_spikes.len(), 7, "{tonic_spikes:?}");
    assert_eq!(&tonic_spikes[1..], [20.0, 40.0, 60.0, 80.0, 100.0, 120.0]);
}

/// One neuron connected to 20 others with random weights in [0, 1).
fn random_weights_description() -> String {
    let targets: String = (1..=20)
        .map(|i| format!("(name=b{i}, position=[10, 0, 0]),\n"))
        .collect();
    format!(
        "(neurons=[(name=a, position=[0, 0, 0]),\n{targets}], connections=[(from=a, to=b{{1:20:1}}, weight=uniform(0, 1))])"
    )
}

#[test]
fn random_weights_are_reproducible_by_seed() {
    let weights = |seed| {
        let mut rt = runtime(1);
        let description = spikes_dna::parse(&random_weights_description()).unwrap();
        let net = build(
            &mut rt,
            &description,
            &BuildOptions {
                seed,
                ..BuildOptions::default()
            },
        );
        net.matching(|n| n.starts_with('b'))
            .iter()
            .map(|n| rt.actor::<spikes_network::NeuronActor>(n.id()).unwrap().synapses[0].weight)
            .collect::<Vec<f32>>()
    };
    let w1 = weights(1);
    assert_eq!(w1.len(), 20);
    assert!(w1.iter().all(|w| (0.0..1.0).contains(w)));
    assert_eq!(w1, weights(1));
    assert_ne!(w1, weights(2));
}

#[test]
fn neurons_are_found_by_name() {
    let mut rt = runtime(1);
    let net = build(&mut rt, &v1("simple-wta-2019.boo"), &BuildOptions::default());
    let out = net.neuron("out-1").unwrap();
    assert_eq!(net.name(out.id()).as_deref(), Some("out-1"));
    assert_eq!(net.matching(|n| n.starts_with("in-")).len(), 2);
    assert!(net.neuron("nope").is_none());
}

/// The line-sensor network driven by Poisson input on several shard counts: every run must be identical.
fn run_line_sensor(shards: usize) -> (Vec<(f64, String)>, usize) {
    let mut rt = runtime(shards);
    let description = v1("line-sensor.boo");
    let net = build(&mut rt, &description, &BuildOptions::default());
    let inputs = net.matching(|n| n.starts_with("input-"));
    rt.spawn(PoissonInput {
        rate_hz: 2_000.0,
        targets: inputs.clone(),
        mv: 6.0,
        delay: SimDuration::from_millis(1),
        start: SimTime::ZERO,
        until: ms(150.0),
    });
    rt.spawn(PeriodicInput {
        period: SimDuration::from_millis(25),
        targets: inputs[..7].to_vec(),
        mv: 3.0,
        delay: SimDuration::from_millis(1),
        start: ms(5.0),
        until: ms(150.0),
    });
    rt.run_until(ms(200.0));
    assert_eq!(rt.stats().lookahead_violations, 0);
    let events = rt.take_events();
    let weights = events
        .iter()
        .filter(|e| matches!(e.event, Event::Weight { .. }))
        .count();
    (spikes(&events, &net), weights)
}

#[test]
fn a_parsed_network_gives_identical_results_on_any_number_of_shards() {
    let (spikes, weights) = run_line_sensor(1);
    assert!(spikes.len() > 50, "network should be active: {} spikes", spikes.len());
    assert!(weights > 0, "learning should change weights");
    for shards in [2, 4] {
        assert_eq!(run_line_sensor(shards), (spikes.clone(), weights), "{shards} shards");
    }
}
