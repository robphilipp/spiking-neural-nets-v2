//! The description language: the test networks, types and overrides, placements, sets, connections, synapse
//! types, nodes, and error reporting (see docs/plans/dna/README.md).

use std::path::Path;

use spikes_dna::{Pattern, WeightSpec, parse};
use spikes_model::{LearningRule, NeuronKind, Sign, Stickiness, WeightDecay, WeightLimit};

fn network_file(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data").join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn every_test_network_and_example_parses() {
    let mut failures = Vec::new();
    let mut count = 0;
    for dir in ["networks", "examples"] {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data").join(dir);
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == "boo"))
            .collect();
        files.sort();
        for file in &files {
            count += 1;
            if let Err(errors) = parse(&std::fs::read_to_string(file).unwrap()) {
                failures.push(format!(
                    "{}:\n  {}",
                    file.display(),
                    errors.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n  ")
                ));
            }
        }
    }
    assert!(count >= 28, "{count} files");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn the_ring_has_its_types_positions_and_connections() {
    let d = parse(&network_file("networks/ring.boo")).unwrap();
    assert_eq!(d.neurons.len(), 25);
    let n = d.neuron("hidden-205").unwrap();
    assert_eq!(n.params().threshold_mv, 5.0);
    let [x, y, z] = n.location_um();
    assert!(
        close(x, 40.0 * 2.1f64.cos()) && close(y, 40.0 * 2.1f64.sin()) && close(z, 40.0),
        "{x} {y} {z}"
    );
    assert_eq!(d.neuron("hidden-401").unwrap().params().decay_ms, 2400.0);
    // Five all-to-all connections of 5 × 5.
    assert_eq!(d.connections.len(), 5);
    assert!(
        d.connections
            .iter()
            .all(|c| c.pattern == Pattern::AllToAll && c.from.len() == 5 && c.to.len() == 5)
    );
    let s = &d.connections[0].synapse;
    assert_eq!(s.weight, WeightSpec::Fixed(1.5));
    assert_eq!(s.limit, WeightLimit::Bounded { lower: 0.0, upper: 5.0 });
    assert_eq!(s.decay, WeightDecay::Exponential { tau_ms: 10_000.0 });
    assert!(
        s.release.is_some(),
        "migrated from the old form, which released stochastically by default"
    );
    assert_eq!(s.learning, "stdp");
    assert!(matches!(d.learning["stdp"], LearningRule::HardLimit(_)));
    assert_eq!(d.stickiness["sig"], Stickiness::Sigmoid { tau_ms: 10_000.0 });
}

const TYPES: &str = "(
neuron_types=[
    (name=base, threshold=5 mV, decay_time=20 ms, conduction_speed=1 m/s),
    (name=inhibitory_cell, extends=base, inhibitory=true, threshold=8 mV),
    (name=burster, dynamics=bistable_integrator, tonic_on_threshold=12 mV, tonic_rate=50 Hz),
],
neurons=[
    (name=a, type=base, position=[0, 0, 0]),
    (name=b, type=inhibitory_cell, position=(r=10, phi=90 deg, z=5), decay_time=50 ms),
    (name=c, type=burster, position=(r=10, theta=90 deg, phi=0)),
    (name=d, position=[1 mm, 0, 0]),
],
)";

#[test]
fn types_extend_and_are_overridden() {
    let d = parse(TYPES).unwrap();
    let a = d.neuron("a").unwrap();
    assert_eq!(a.params().threshold_mv, 5.0);
    assert!(!a.inhibitory());
    let b = d.neuron("b").unwrap();
    assert_eq!(b.params().threshold_mv, 8.0, "its type's own setting over the base's");
    assert_eq!(b.params().decay_ms, 50.0, "its own setting over its type's");
    assert_eq!(b.conduction_m_per_s(), 1.0, "from the base type");
    assert!(b.inhibitory());
    let [x, y, z] = b.location_um();
    assert!(close(x, 0.0) && close(y, 10.0) && close(z, 5.0), "{x} {y} {z}");
    let c = d.neuron("c").unwrap();
    assert!(matches!(c.params().kind, NeuronKind::BistableIntegrator { tonic_rate_hz, .. } if tonic_rate_hz == 50.0));
    assert_eq!(c.params().threshold_mv, 12.0);
    let [x, _, z] = c.location_um();
    assert!(close(x, 10.0) && close(z, 0.0));
    let n = d.neuron("d").unwrap();
    assert_eq!(n.location_um(), [1000.0, 0.0, 0.0]);
    assert_eq!(n.params().threshold_mv, 10.0, "the default");
}

const POPULATIONS: &str = "(
neuron_types=[(name=cell, threshold=5 mV)],
populations=[
    (name=row, type=cell, count=5, line=(from=[0, 0, 0], to=[40, 0, 0])),
    (name=circle, type=cell, count=4, ring=(center=[0, 0, 10], radius=20)),
    (name=sheet, type=cell, grid=(origin=[0, 0, 0], spacing=[10, 10, 1], shape=[3, 2, 1])),
    (name=cloud, type=cell, count=50, random=(ball=(center=[0, 0, 0], radius=100)), seed=3),
    (name=slab, count=40, random=(box=(corner=[0, 0, 0], size=[100, 100, 10]))),
    (name=named, positions=[(name=left, at=[-5, 0, 0]), (name=right, at=[5, 0, 0])]),
    (name=points, positions=[[1, 2, 3], [4, 5, 6]]),
],
sets=[(name=edges, members=[row-0, row-4, left])],
)";

#[test]
fn populations_are_placed_and_named() {
    let d = parse(POPULATIONS).unwrap();
    assert_eq!(d.neurons.len(), 5 + 4 + 6 + 50 + 40 + 2 + 2);
    assert_eq!(d.neuron("row-2").unwrap().location_um(), [20.0, 0.0, 0.0]);
    let q = d.neuron("circle-1").unwrap().location_um();
    assert!(close(q[0], 0.0) && close(q[1], 20.0) && close(q[2], 10.0), "{q:?}");
    assert_eq!(
        d.neuron("sheet-4").unwrap().location_um(),
        [10.0, 10.0, 0.0],
        "x fastest"
    );
    for i in 0..50 {
        let p = d.neuron(&format!("cloud-{i}")).unwrap().location_um();
        assert!(p.iter().map(|v| v * v).sum::<f64>().sqrt() <= 100.0);
    }
    assert_eq!(d.neuron("right").unwrap().location_um(), [5.0, 0.0, 0.0]);
    assert!(d.neuron("named-0").is_none(), "named neurons go by their names");
    assert_eq!(d.neuron("points-1").unwrap().location_um(), [4.0, 5.0, 6.0]);
    assert_eq!(d.neuron("row-0").unwrap().params().threshold_mv, 5.0);
    assert_eq!(d.members("sheet").unwrap().len(), 6);
    let edges = d.members("edges").unwrap();
    assert_eq!(edges.len(), 3);
    // The same file places them the same way.
    assert_eq!(parse(POPULATIONS).unwrap().neurons, d.neurons);
}

const CONNECTIONS: &str = "(
learning_rules=[(name=stdp, rule=stdp_hard, depression=0.01, depression_time=20 ms, potentiation=0.02, potentiation_time=20 ms)],
stickiness=[(name=sticky, time=1 s)],
synapse_types=[
    (name=plastic, weight=uniform(0.2, 0.6), learning=stdp, limits=(min=0, max=2), decay=(time=1 s), release=()),
    (name=strong, extends=plastic, weight=2, stickiness=sticky),
    (name=inhibitory_synapse, weight=normal(1, 0.1), sign=inhibitory, delay=1 ms),
],
populations=[
    (name=a, count=10, line=(from=[0, 0, 0], to=[90, 0, 0])),
    (name=b, count=10, line=(from=[0, 10, 0], to=[90, 10, 0])),
],
sets=[(name=firsts, members=[a-0, b-0])],
connections=[
    (from=a, to=b, type=plastic),
    (from=a, to=b, pattern=one_to_one, type=strong, limits=(min=0, max=9)),
    (from=a, to=b, pattern=fan_in, count=3, within=50 µm, type=inhibitory_synapse, self=true),
    (from=[a, b], to=firsts, pattern=random, probability=0.5, weight=1),
],
)";

#[test]
fn connections_take_patterns_types_and_overrides() {
    let d = parse(CONNECTIONS).unwrap();
    let [all, one, fan, random] = &d.connections[..] else {
        panic!("{:?}", d.connections.len())
    };
    assert_eq!(all.pattern, Pattern::AllToAll);
    assert_eq!((all.from.len(), all.to.len()), (10, 10));
    assert_eq!(all.synapse.weight, WeightSpec::Uniform { low: 0.2, high: 0.6 });
    assert_eq!(all.synapse.learning, "stdp");
    assert_eq!(all.synapse.delay_ms, 0.5, "the default synaptic delay");
    assert!(all.synapse.sign.is_none(), "from the sender");

    assert_eq!(one.pattern, Pattern::OneToOne);
    assert_eq!(
        one.synapse.weight,
        WeightSpec::Fixed(2.0),
        "the type's own over its base's"
    );
    assert_eq!(one.synapse.stickiness, "sticky");
    assert_eq!(one.synapse.learning, "stdp", "from the base type");
    assert_eq!(
        one.synapse.limit,
        WeightLimit::Bounded { lower: 0.0, upper: 9.0 },
        "the connection's own limits replace the type's whole"
    );
    assert_eq!(one.synapse.decay, WeightDecay::Exponential { tau_ms: 1000.0 });

    assert_eq!(fan.pattern, Pattern::FanIn(3));
    assert_eq!(fan.within_um, Some(50.0));
    assert!(fan.autapses);
    assert_eq!(fan.synapse.sign, Some(Sign::Inhibitory));
    assert_eq!(fan.synapse.weight, WeightSpec::Normal { mean: 1.0, sd: 0.1 });
    assert_eq!(fan.synapse.delay_ms, 1.0);
    assert!(fan.synapse.release.is_none(), "release is off unless asked for");

    assert_eq!(random.pattern, Pattern::Random(0.5));
    assert_eq!(random.from.len(), 20);
    assert_eq!(random.to.len(), 2);
    assert_eq!(
        random.synapse.decay,
        WeightDecay::None,
        "a connection without a type: plain"
    );
}

const NODES: &str = "(
nodes=[(name=camera), (name=cortex)],
neurons=[(name=eye, position=[0, 0, 0], node=camera), (name=free, position=[1, 0, 0])],
populations=[
    (name=v1, count=4, line=(from=[0, 0, 0], to=[30, 0, 0]), node=cortex),
    (name=rest, count=3, line=(from=[0, 10, 0], to=[30, 10, 0])),
],
sets=[(name=pinned, members=[rest-0, free], node=main)],
)";

#[test]
fn neurons_populations_and_sets_can_be_pinned_to_nodes() {
    let d = parse(NODES).unwrap();
    assert_eq!(d.nodes, ["camera", "cortex"]);
    let nodes = d.pinned_nodes();
    let at = |name: &str| nodes[d.neurons.names().index(name).unwrap() as usize].clone();
    assert_eq!(at("eye").as_deref(), Some("camera"));
    assert_eq!(at("v1-3").as_deref(), Some("cortex"));
    assert_eq!(at("rest-0").as_deref(), Some("main"));
    assert_eq!(at("free").as_deref(), Some("main"));
    assert_eq!(at("rest-1"), None, "placed automatically");
}

// ---------- errors ----------

#[test]
fn all_problems_are_reported_with_positions() {
    let text = "(\n\
        neurons=[\n\
        (name=a, threshold=5 ms, position=[0, 0, 0]),\n\
        (name=a, xyz=1, position=[0, 0, 0]),\n\
        (name=b, dynamics=monostable_resonator, position=[0, 0, 0], node=nowhere),\n\
        ],\n\
        connections=[(from=a, to=zz, weight=1, learning=nope, type=missing)],\n\
        )";
    let errors = parse(text).unwrap_err();
    let messages: Vec<String> = errors.iter().map(ToString::to_string).collect();
    let has = |line: u32, text: &str| {
        assert!(
            errors.iter().any(|e| e.span.line == line && e.message.contains(text)),
            "no error on line {line} containing {text:?}; got:\n{}",
            messages.join("\n")
        );
    };
    has(3, "'ms' is not a unit for 'threshold'");
    has(4, "unknown key 'xyz'");
    has(4, "neuron 'a' is already defined at 3:1");
    has(5, "needs 'resonance'");
    has(5, "node 'nowhere' isn't defined");
    has(7, "learning rule 'nope' isn't defined");
    has(7, "'zz' isn't a neuron");
    has(7, "synapse type 'missing' isn't defined");
}

#[test]
fn syntax_errors_point_at_the_spot() {
    let errors = parse("(\nneurons=[\n  (name=a position=[0, 0, 0])\n])").unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!((errors[0].span.line, errors[0].span.column), (3, 11));
    assert!(errors[0].message.contains("expected ','"), "{}", errors[0]);
}

#[test]
fn mistakes_are_explained() {
    for (bad, says) in [
        ("populations=[(name=a, count=10)]", "needs a placement"),
        (
            "populations=[(name=a, count=1.5, line=(from=[0, 0, 0], to=[1, 0, 0]))]",
            "whole number",
        ),
        (
            "populations=[(name=a, count=2, line=(from=[0,0,0], to=[1,0,0]), ring=(radius=1))]",
            "one placement",
        ),
        (
            "populations=[(name=a, count=2, grid=(spacing=[1, 1, 1], shape=[2, 1, 1]))]",
            "comes from the grid",
        ),
        (
            "populations=[(name=a, count=2, line=(from=[0,0,0], to=[1,0,0]))], connections=[(from=a, to=a, pattern=random, probability=2, weight=1)]",
            "from 0 to 1",
        ),
        (
            "populations=[(name=a, count=2, line=(from=[0,0,0], to=[1,0,0]))], connections=[(from=a, to=a, pattern=one_to_one, weight=1, count=3)]",
            "doesn't apply",
        ),
        (
            "populations=[(name=a, count=2, line=(from=[0,0,0], to=[1,0,0]))], connections=[(from=a, to=a-0, pattern=one_to_one, weight=1)]",
            "as many senders as receivers",
        ),
        (
            "populations=[(name=a, count=2, line=(from=[0,0,0], to=[1,0,0]))], connections=[(from=a, to=a)]",
            "needs a 'weight'",
        ),
        (
            "populations=[(name=a, count=2, line=(from=[0,0,0], to=[1,0,0]))], connections=[(from=a, to=a, weight=r0:1)]",
            "uniform(0, 1)",
        ),
        (
            "neuron_types=[(name=x, extends=y)], neurons=[(name=n, type=x, position=[0,0,0])]",
            "'y' isn't defined",
        ),
        (
            "neuron_types=[(name=x, extends=x)], neurons=[(name=n, type=x, position=[0,0,0])]",
            "extends itself",
        ),
        ("neurons=[(name=n, position=[0, 0])]", "a point is"),
    ] {
        let errors = parse(&format!("({bad})")).unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains(says)), "{bad}: {errors:?}");
    }
}

#[test]
fn modulators_receptors_and_releases() {
    let d = parse(
        "(
modulators=[(name=dopamine, clearance_time=200 ms, reach=300 µm, speed=0.01 m/s, latency=2 ms, falloff=100 µm)],
neurons=[
    (name=vta, position=[0, 0, 0], releases=(modulator=dopamine, amount=2)),
    (name=pyr, position=[50, 0, 0], receptors=[(modulator=dopamine, excitatory_gain=0.5, eligibility_time=1 s, learning_rate=20)]),
],
)",
    )
    .unwrap();
    let m = &d.modulators[0];
    assert_eq!(
        (m.clearance_ms, m.radius_um, m.latency_ms, m.falloff_um),
        (200.0, 300.0, 2.0, Some(100.0))
    );
    assert_eq!(d.neuron("vta").unwrap().release(), Some((0, 2.0)));
    let r = &d.neuron("pyr").unwrap().receptors()[0];
    assert_eq!(r.excitatory_gain, 0.5);
    assert_eq!(r.three_factor.unwrap().eligibility_ms, 1000.0);
    let errors = parse("(neurons=[(name=x, position=[0,0,0], releases=(modulator=nope))])").unwrap_err();
    assert!(
        errors[0].message.contains("modulator 'nope' isn't defined"),
        "{errors:?}"
    );
}
