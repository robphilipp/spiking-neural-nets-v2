//! Connection rules are expanded when building: per receiver, reproducibly, and the same on every node.

use std::collections::{BTreeMap, BTreeSet};

use spikes_network::{BuildOptions, Network, NeuronActor, Runtime, build};
use tempo_runtime::{ActorId, NodePlacement, RuntimeConfig};

const NET: &str = "(
POP=[
    (nam=exc, cnt=400, csp=1 m/s, REG=(cst=bx, px1=0, px2=0, px3=0, sx1=400 µm, sx2=400 µm, sx3=400 µm)),
    (nam=inh, cnt=100, inh=t, REG=(cst=bx, px1=0, px2=0, px3=0, sx1=400 µm, sx2=400 µm, sx3=400 µm)),
],
RUL=[
    (prp=exc, psp=exc, cnt=30, cnw=r0.5:1.5),
    (prp=inh, psp=exc, prb=0.2, cnw=1),
    (prp=exc, psp=inh, cnt=10, rad=120 µm, cnw=1),
],
)";

fn runtime(cluster: Option<NodePlacement>) -> Runtime {
    Runtime::new(RuntimeConfig {
        seed: 1,
        shards: 1,
        cluster,
        ..RuntimeConfig::default()
    })
}

/// Every stored synapse: (post, edge index) → (pre, weight bits, sign).
fn synapses(rt: &Runtime, net: &Network) -> BTreeMap<(ActorId, u32), (ActorId, u32, bool)> {
    let mut all = BTreeMap::new();
    for n in &net.neurons {
        if let Some(a) = rt.actor::<NeuronActor>(n.id()) {
            for (i, (s, pre)) in a.synapses.iter().zip(&a.pre).enumerate() {
                all.insert(
                    (n.id(), i as u32),
                    (*pre, s.weight.to_bits(), s.sign == spikes_model::Sign::Inhibitory),
                );
            }
        }
    }
    all
}

#[test]
fn rules_give_each_receiver_its_inputs() {
    let d = spikes_dna::parse(NET).unwrap();
    let mut rt = runtime(None);
    let net = build(&mut rt, &d, &BuildOptions::default());
    let index = |id: ActorId| net.neurons.iter().position(|n| n.id() == id).unwrap();
    let mut probability_inputs = 0usize;
    for (i, n) in net.neurons.iter().enumerate() {
        let a = rt.actor::<NeuronActor>(n.id()).unwrap();
        let pres: Vec<usize> = a.pre.iter().map(|&p| index(p)).collect();
        let distinct: BTreeSet<usize> = pres.iter().copied().collect();
        assert_eq!(distinct.len(), pres.len(), "neuron {i}: no repeated sender");
        assert!(!pres.contains(&i), "neuron {i}: no autapse");
        if i < 400 {
            // 30 from exc, then each inh with probability 0.2.
            assert!(pres[..30].iter().all(|&p| p < 400));
            let from_inh = pres[30..].iter().filter(|&&p| p >= 400).count();
            assert_eq!(from_inh, pres.len() - 30);
            assert!(
                a.synapses[30..]
                    .iter()
                    .all(|s| s.sign == spikes_model::Sign::Inhibitory)
            );
            probability_inputs += from_inh;
        } else {
            // Up to 10 from exc within 120 µm.
            assert!(pres.len() <= 10);
            let here = d.neurons[i].location_um;
            for &p in &pres {
                let there = d.neurons[p].location_um;
                let distance = (0..3).map(|k| (here[k] - there[k]).powi(2)).sum::<f64>().sqrt();
                assert!(distance <= 120.0, "{distance}");
            }
        }
    }
    // 400 receivers × 100 senders × 0.2 = 8000 expected.
    assert!((7000..9000).contains(&probability_inputs), "{probability_inputs}");
    assert_eq!(net.synapse_count, synapses(&rt, &net).len() as u64);

    // The same seed, the same network; another seed, another.
    let mut again = runtime(None);
    let net2 = build(&mut again, &d, &BuildOptions::default());
    assert_eq!(synapses(&again, &net2), synapses(&rt, &net));
    let mut other = runtime(None);
    let net3 = build(
        &mut other,
        &d,
        &BuildOptions {
            seed: 2,
            ..BuildOptions::default()
        },
    );
    assert_ne!(synapses(&other, &net3), synapses(&rt, &net));
}

#[test]
fn every_node_builds_its_share_of_the_same_network() {
    let d = spikes_dna::parse(NET).unwrap();
    let mut alone = runtime(None);
    let net = build(&mut alone, &d, &BuildOptions::default());
    let whole = synapses(&alone, &net);
    let mut union = BTreeMap::new();
    for node in 0..3 {
        let mut rt = runtime(Some(NodePlacement {
            node,
            shards_per_node: vec![1, 1, 1],
        }));
        let part = build(&mut rt, &d, &BuildOptions::default());
        assert_eq!(part.synapse_count, net.synapse_count);
        let mine = synapses(&rt, &part);
        assert!(mine.len() < whole.len(), "a node keeps only its share");
        union.extend(mine);
    }
    assert_eq!(union, whole);
}
