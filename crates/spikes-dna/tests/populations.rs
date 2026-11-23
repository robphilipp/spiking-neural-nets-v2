//! Populations (`POP`) and connection rules (`RUL`).

use spikes_dna::{InDegree, Region, parse};

const NET: &str = "(
NRN=[(nid=probe, mst=10 mV, LOC=(cst=ct, px1=0, px2=0, px3=0))],
POP=[
    (nam=exc, cnt=800, mst=10 mV, csp=1 m/s, REG=(cst=bx, px1=0, px2=0, px3=0, sx1=500 µm, sx2=500 µm, sx3=100 µm)),
    (nam=inh, cnt=200, inh=t, mst=8 mV, REG=(cst=sp, px1=250, px2=250, px3=50, rad=200 µm), sed=7),
],
RUL=[
    (prp=exc, psp=exc, cnt=50, cnw=r0.5:1.5, lrn=stdp),
    (prp=inh, psp=exc, prb=0.1, rad=150 µm, cnw=2),
    (prp=exc, psp=default, cnt=10, cnw=1),
],
LRN=[(fnc=stdp, ina=0.01, inp=30 ms, exa=0.01, exp=20 ms)],
)";

#[test]
fn populations_expand_to_neurons_placed_in_their_regions() {
    let d = parse(NET).unwrap();
    assert_eq!(d.neurons.len(), 1 + 800 + 200);
    assert_eq!(d.neurons[1].id, "exc-0");
    assert_eq!(d.neurons[1000].id, "inh-199");
    assert!(d.neurons[900].inhibitory && !d.neurons[500].inhibitory);
    assert_eq!(d.neurons[900].params.threshold_mv, 8.0);
    for n in &d.neurons[1..801] {
        let [x, y, z] = n.location_um;
        assert!((0.0..=500.0).contains(&x) && (0.0..=500.0).contains(&y) && (0.0..=100.0).contains(&z));
    }
    for n in &d.neurons[801..] {
        let [x, y, z] = n.location_um;
        assert!(((x - 250.0).powi(2) + (y - 250.0).powi(2) + (z - 50.0).powi(2)).sqrt() <= 200.0);
    }
    assert_eq!(d.populations[1].first, 801);
    assert!(matches!(d.populations[1].region, Region::Ball { radius, .. } if radius == 200.0));
    // The same file places them the same way.
    assert_eq!(parse(NET).unwrap().neurons, d.neurons);
}

#[test]
fn rules_stay_rules() {
    let d = parse(NET).unwrap();
    assert!(d.connections.is_empty());
    assert_eq!(d.rules.len(), 3);
    assert_eq!(d.rules[0].in_degree, InDegree::Fixed(50));
    assert_eq!(d.rules[1].in_degree, InDegree::Probability(0.1));
    assert_eq!(d.rules[1].radius_um, Some(150.0));
    assert_eq!(d.members("exc").unwrap().len(), 800);
    // `default` is the group every neuron without `grp` is in: the probe and both populations.
    assert_eq!(d.members("default").unwrap().len(), 1001);
    assert_eq!(d.members("nothing"), None);
}

#[test]
fn mistakes_in_rules_and_populations_are_reported() {
    for (bad, says) in [
        ("POP=[(nam=a, cnt=10)]", "REG"),
        (
            "POP=[(nam=a, cnt=1.5, REG=(px1=0, px2=0, px3=0, sx1=1, sx2=1, sx3=1))]",
            "whole number",
        ),
        (
            "POP=[(nam=a, cnt=2, REG=(px1=0, px2=0, px3=0, sx1=1, sx2=1, sx3=1))], RUL=[(prp=a, psp=b, cnt=1, cnw=1)]",
            "neither a population",
        ),
        (
            "POP=[(nam=a, cnt=2, REG=(px1=0, px2=0, px3=0, sx1=1, sx2=1, sx3=1))], RUL=[(prp=a, psp=a, cnw=1)]",
            "exactly one of",
        ),
        (
            "POP=[(nam=a, cnt=2, REG=(px1=0, px2=0, px3=0, sx1=1, sx2=1, sx3=1))], RUL=[(prp=a, psp=a, prb=2, cnw=1)]",
            "probability",
        ),
        (
            "POP=[(nam=a, cnt=2, nid=x, REG=(px1=0, px2=0, px3=0, sx1=1, sx2=1, sx3=1))]",
            "nid",
        ),
    ] {
        let errors = parse(&format!("({bad})")).unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains(says)), "{bad}: {errors:?}");
    }
}
