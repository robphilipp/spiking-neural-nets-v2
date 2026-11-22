//! Parser tests: v1 conformance, ported v1 tests, v2 extensions and error reporting.

use std::path::Path;

use spikes_dna::{WeightSpec, parse};
use spikes_model::{LearningRule, NeuronKind, Sign, Stickiness, WeightDecay, WeightLimit};

fn v1_file(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-data/v1")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn every_v1_network_parses() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/v1");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "boo"))
        .collect();
    files.sort();
    assert!(files.len() >= 25, "expected the v1 test networks in {}", dir.display());
    let mut failures = Vec::new();
    for file in &files {
        let text = std::fs::read_to_string(file).unwrap();
        match parse(&text) {
            Ok(network) => assert!(!network.neurons.is_empty(), "{}", file.display()),
            Err(errors) => failures.push(format!(
                "{}:\n  {}",
                file.file_name().unwrap().to_string_lossy(),
                errors.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n  ")
            )),
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} files failed:\n{}",
        failures.len(),
        files.len(),
        failures.join("\n")
    );
}

// ---------- ported from v1's DnaParserTest (simple-wta.boo, 2019 version) ----------

#[test]
fn simple_wta_has_six_neurons_eight_connections_and_its_learning_functions() {
    let network = parse(&v1_file("simple-wta-2019.boo")).unwrap();
    let ids: Vec<&str> = network.neurons.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ids, ["in-1", "in-2", "inh-1", "inh-2", "out-1", "out-2"]);
    // in-{1,2} → out-{1,2} expands to 4, plus 2 output→inhibition and 2 inhibition→output.
    assert_eq!(network.connections.len(), 8);
    let input_to_output: Vec<(&str, &str)> = network.connections[..4]
        .iter()
        .map(|c| (c.pre.as_str(), c.post.as_str()))
        .collect();
    assert_eq!(
        input_to_output,
        [
            ("in-1", "out-1"),
            ("in-1", "out-2"),
            ("in-2", "out-1"),
            ("in-2", "out-2")
        ]
    );
    assert!(matches!(network.learning["stdp_soft"], LearningRule::SoftLimit(_)));
    assert!(matches!(network.learning["stdp_alpha"], LearningRule::Alpha(_)));
    assert!(matches!(network.learning["flat"], LearningRule::None));
    assert_eq!(network.groups.len(), 1);
}

#[test]
fn neuron_parameters_are_converted_to_base_units() {
    let network = parse(&v1_file("simple-wta-2019.boo")).unwrap();
    let inh = network.neuron("inh-1").unwrap();
    assert!(inh.inhibitory);
    assert_eq!(inh.params.threshold_mv, 0.4);
    assert_eq!(inh.params.refractory_ms, 0.1);
    assert_eq!(inh.params.base_refractoriness, 0.1); // 0.1 µWb
    assert_eq!(inh.params.decay_ms, 250.0);
    assert_eq!(inh.params.spike_mv, 0.5);
    assert_eq!(inh.conduction_m_per_s, 0.08);
    assert_eq!(inh.params.intrinsic.decay_ms, 3_600_000.0); // 3600 s
    assert_eq!(inh.synapses.decay, WeightDecay::Exponential { tau_ms: 10_000.0 });
    assert_eq!(inh.synapses.limit, WeightLimit::Bounded { lower: 0.0, upper: 1.5 });
    assert_eq!(inh.location_um, [-290.0, 0.0, 0.0]);
    let out = network.neuron("out-1").unwrap();
    assert_eq!(out.params.intrinsic.learning_mv, 0.0); // "0 nV"
    assert_eq!(out.synapses.weight_noise, 1e-5);
    // Connections from an inhibitory neuron are inhibitory by default.
    let from_inh = network.connections.iter().find(|c| c.pre == "inh-1").unwrap();
    assert_eq!(from_inh.sign, Sign::Inhibitory);
}

#[test]
fn an_invalid_connection_parameter_is_rejected() {
    let text = v1_file("simple-wta-2019.boo").replacen(
        "(prn=out-1, psn=inh-1, cnw=1, eqw=1, lrn=flat)",
        "(prn=out-1, psm=inh-1, cnw=1, eqw=1, lrn=flat)",
        1,
    );
    let errors = parse(&text).unwrap_err();
    assert!(
        errors.iter().any(|e| e.message.contains("unknown key 'psm'")),
        "{errors:?}"
    );
    assert!(errors.iter().any(|e| e.message.contains("needs 'psn'")), "{errors:?}");
}

// ---------- 2017-era syntax ----------

#[test]
fn older_files_get_defaults_and_old_names() {
    // stdp-test.boo lacks ipb/ipl/ipd and wnm, and uses fnc=stdp.
    let network = parse(&v1_file("stdp-test.boo")).unwrap();
    assert!(matches!(network.learning["stdp"], LearningRule::HardLimit(_)));
    let input = network.neuron("input-1").unwrap();
    assert_eq!(input.params.intrinsic.decay_ms, 600_000.0);
    assert_eq!(input.location_um, [1_000.0, 0.0, 0.0], "1 mm");
}

#[test]
fn id_ranges_expand_like_v1() {
    let network = parse(&v1_file("line-sensor.boo")).unwrap();
    let from_input_1x: Vec<&str> = network
        .connections
        .iter()
        .filter(|c| c.post == "hidden-1")
        .map(|c| c.pre.as_str())
        .collect();
    assert_eq!(
        from_input_1x,
        [
            "input-11", "input-12", "input-13", "input-14", "input-15", "input-16", "input-17"
        ]
    );
}

// ---------- v2 extensions ----------

const V2: &str = r#"
// v2 features: resonators, homeostasis, connection signs, named learning rules
(
NRN=[
    (nid=a, nty=mr, mst=10 mV, LOC=(cst=ct, px1=0, px2=0, px3=0), RES=(frq=40 Hz, dmp=50 ms)),
    (nid=b, nty=br, mst=10 mV, LOC=(cst=cl, px1=10 µm, px2=90 deg, px3=5 µm),
        RES=(frq=0.05 kHz, dmp=0.1 s, lcr=12 mV, sep=6 mV),
        HIP=(tgt=10 Hz, str=0.5, win=2 s, lwb=-5 mV, upb=5 mV)),
    (nid=c, inh=t, LOC=(cst=sp, px1=10, px2=0, px3=0)),
],
CON=[
    (prn=a, psn={b, c}, cnw=r0.5:1.5, lrn=fast, cst=sig),
    (prn=c, psn=a, cnw=1, typ=exc),
],
LRN=[
    (fnc=stdp_hard, nam=fast, ina=0.1, inp=5 ms, exa=0.1, exp=5 ms),
    (fnc=stdp_hard, nam=slow, ina=0.01, inp=50 ms, exa=0.01, exp=50 ms),
],
CST=[(fnc=sig, dhl=10 s)],
)
"#;

#[test]
fn v2_extensions_parse() {
    let network = parse(V2).unwrap();
    let a = network.neuron("a").unwrap();
    assert!(
        matches!(a.params.kind, NeuronKind::MonostableResonator(r) if r.frequency_hz == 40.0 && r.damping_ms == 50.0)
    );
    let b = network.neuron("b").unwrap();
    let NeuronKind::BistableResonator {
        resonance,
        cycle_mv,
        separatrix_mv,
    } = b.params.kind
    else {
        panic!("{:?}", b.params.kind)
    };
    assert_eq!(
        (resonance.frequency_hz, resonance.damping_ms, cycle_mv, separatrix_mv),
        (50.0, 100.0, 12.0, 6.0)
    );
    let h = b.params.intrinsic.homeostasis.unwrap();
    assert_eq!(
        (h.target_hz, h.strength, h.window_ms, h.min_mv, h.max_mv),
        (10.0, 0.5, 2_000.0, -5.0, 5.0)
    );
    // Cylindrical (r=10, φ=90°, z=5) → (0, 10, 5); spherical (r=10, φ=0, θ=0) → (0, 0, 10).
    assert!((b.location_um[0]).abs() < 1e-9 && (b.location_um[1] - 10.0).abs() < 1e-9 && b.location_um[2] == 5.0);
    assert!(network.neuron("c").unwrap().location_um[2] == 10.0);

    assert_eq!(network.connections.len(), 3);
    assert_eq!(
        network.connections[0].weight,
        WeightSpec::Random { low: 0.5, high: 1.5 }
    );
    assert_eq!(network.connections[0].learning, "fast");
    assert_eq!(network.connections[0].stickiness, "sig");
    assert_eq!(network.stickiness["sig"], Stickiness::Sigmoid { tau_ms: 10_000.0 });
    // c is inhibitory, but its connection says typ=exc.
    assert_eq!(network.connections[2].sign, Sign::Excitatory);
    assert!(network.learning.contains_key("slow"));
    // No GRP section: neurons run in the "default" group.
    assert_eq!(network.groups.len(), 1);
    assert_eq!(network.groups[0].id, "default");
}

// ---------- errors ----------

#[test]
fn all_problems_are_reported_with_positions() {
    let text = "(\n\
        NRN=[\n\
        (nid=a, mst=5 ms, LOC=(cst=ct, px1=0, px2=0, px3=0)),\n\
        (nid=a, xyz=1, LOC=(cst=ct, px1=0, px2=0, px3=0)),\n\
        (nid=b, nty=mr, LOC=(cst=ct, px1=0, px2=0, px3=0)),\n\
        ],\n\
        CON=[(prn=a, psn=zz, cnw=1, lrn=nope)],\n\
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
    has(3, "'ms' is not a unit for 'mst'");
    has(4, "unknown key 'xyz'");
    has(4, "neuron 'a' is already defined at 3:1");
    has(5, "needs 'RES'");
    has(7, "learning function 'nope' isn't defined");
    has(7, "neuron 'zz', which isn't defined");
}

#[test]
fn syntax_errors_point_at_the_spot() {
    let errors = parse("(\nNRN=[\n  (nid=a mst=1)\n])").unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!((errors[0].span.line, errors[0].span.column), (3, 10));
    assert!(errors[0].message.contains("expected ','"), "{}", errors[0]);
}

#[test]
fn undefined_neuron_sets_are_reported_once_per_connection() {
    let errors =
        parse("(NRN=[(nid=a, LOC=(cst=ct, px1=0, px2=0, px3=0))], CON=[(prn=a, psn=b{1:20:1}, cnw=1)])").unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        errors[0].message.starts_with("connection refers to 20 neurons"),
        "{}",
        errors[0]
    );
}

const MODULATED: &str = "(
MOD=[(nam=dopamine, clr=200 ms, rad=300 µm, spd=0.01 m/s, lat=2 ms, len=100 µm), (nam=ach, clr=1 s)],
NRN=[
    (nid=vta, LOC=(cst=ct, px1=0, px2=0, px3=0), REL=(mod=dopamine, amt=2)),
    (nid=pyr, LOC=(cst=ct, px1=50, px2=0, px3=0),
        RCP=[(mod=dopamine, exg=0.5, elg=500 ms, rat=0.1), (mod=ach, thr=-0.2, dir=1 mV)]),
],
)";

#[test]
fn modulators_receptors_and_releases_parse() {
    let network = parse(MODULATED).unwrap();
    assert_eq!(network.modulators.len(), 2);
    let da = &network.modulators[0];
    assert_eq!(
        (
            da.name.as_str(),
            da.clearance_ms,
            da.radius_um,
            da.speed_m_per_s,
            da.latency_ms,
            da.falloff_um
        ),
        ("dopamine", 200.0, 300.0, 0.01, 2.0, Some(100.0))
    );
    assert_eq!(network.modulators[1].radius_um, f64::INFINITY);
    assert_eq!(network.neuron("vta").unwrap().release, Some((0, 2.0)));
    let receptors = &network.neuron("pyr").unwrap().receptors;
    assert_eq!(receptors.len(), 2);
    assert_eq!(receptors[0].modulator, 0);
    assert_eq!(receptors[0].excitatory_gain, 0.5);
    assert_eq!(receptors[0].three_factor.unwrap().eligibility_ms, 500.0);
    assert_eq!(
        (receptors[1].modulator, receptors[1].threshold, receptors[1].direct_mv),
        (1, -0.2, 1.0)
    );
}

#[test]
fn unknown_modulators_are_reported() {
    let errors = parse(&MODULATED.replace("(mod=ach, thr", "(mod=serotonin, thr")).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("modulator 'serotonin' isn't defined")),
        "{errors:?}"
    );
}
