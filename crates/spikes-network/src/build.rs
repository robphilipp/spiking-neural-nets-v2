//! Building a runtime network from a [`NetworkDescription`].

use std::collections::HashMap;
use std::sync::Arc;

use rand::{RngExt, SeedableRng};
use rand_xoshiro::Xoshiro256PlusPlus;
use spikes_dna::{NetworkDescription, WeightSpec};
use spikes_model::{NeuronParams, Receptor, Synapse, SynapseParams};
use tempo_runtime::placement::spatial_partitions;
use tempo_runtime::{ActorId, EdgeIdx, Jitter, RouteEdge, SimDuration};

use crate::Runtime;
use crate::messages::{ModulatorRef, NeuronRef};
use crate::modulators::{ModulatorField, ModulatorKind, ModulatorRelease};
use crate::neuron::{NetworkParams, NeuronActor};

/// How to build a network.
#[derive(Clone, Debug)]
pub struct BuildOptions {
    /// Seed for random initial weights (`cnw=r0:1`).
    pub seed: u64,
    /// The shortest allowed connection delay. Neurons closer than `min_delay × speed` get this delay; it also keeps
    /// the lookahead (and so parallelism) from collapsing when neurons sit very close together.
    pub min_delay: SimDuration,
    /// Intentional timing noise on every connection.
    pub jitter: Jitter,
    /// Report every weight change from learning as an event.
    pub record_weights: bool,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            seed: 0,
            min_delay: SimDuration::ZERO,
            jitter: Jitter::None,
            record_weights: true,
        }
    }
}

/// A built network: its neurons, findable by the names in the description.
#[derive(Clone, Debug)]
pub struct Network {
    /// Neurons in description order.
    pub neurons: Vec<NeuronRef>,
    /// The shared parameter tables.
    pub params: Arc<NetworkParams>,
    by_name: HashMap<String, NeuronRef>,
    names: HashMap<ActorId, String>,
}

impl Network {
    /// The neuron with this name.
    pub fn neuron(&self, name: &str) -> Option<NeuronRef> {
        self.by_name.get(name).copied()
    }

    /// The name of the neuron with this ID.
    pub fn name(&self, id: ActorId) -> Option<&str> {
        self.names.get(&id).map(String::as_str)
    }

    /// Adds a modulator release site at `position` (µm), e.g. for a UI "reward" button. Send it
    /// `ModulatorMsg::Release` to release a modulator there.
    pub fn add_release_site(&self, rt: &mut Runtime, position: [f64; 3]) -> ModulatorRef {
        let id = rt.spawn(ModulatorRelease {
            position,
            kinds: Arc::clone(&self.params.modulators),
        });
        ModulatorRef::new(id)
    }

    /// The index of the modulator with this name (as used in `ModulatorMsg::Release`).
    pub fn modulator(&self, name: &str) -> Option<u16> {
        self.params
            .modulators
            .iter()
            .position(|m| m.name == name)
            .map(|i| i as u16)
    }

    /// The neurons whose names satisfy `matches`, in description order.
    pub fn matching(&self, matches: impl Fn(&str) -> bool) -> Vec<NeuronRef> {
        self.neurons
            .iter()
            .copied()
            .filter(|n| self.names.get(&n.id()).is_some_and(|name| matches(name)))
            .collect()
    }
}

/// Deduplicates parameter sets (which hold floats, so they can't be hashed directly) by their debug form.
struct Table<T> {
    items: Vec<T>,
    index: HashMap<String, u32>,
}

impl<T: std::fmt::Debug> Table<T> {
    fn new() -> Self {
        Self {
            items: Vec::new(),
            index: HashMap::new(),
        }
    }

    fn insert(&mut self, item: T) -> u32 {
        let key = format!("{item:?}");
        if let Some(&i) = self.index.get(&key) {
            return i;
        }
        let i = u32::try_from(self.items.len()).expect("parameter table fits u32");
        self.items.push(item);
        self.index.insert(key, i);
        i
    }
}

/// A population: dynamics, receptors, and modulator release on spiking.
type Population = (NeuronParams, Vec<Receptor>, Option<(u16, f64)>);

fn draw(spec: WeightSpec, rng: &mut Xoshiro256PlusPlus) -> f64 {
    match spec {
        WeightSpec::Fixed(w) => w,
        WeightSpec::Random { low, high } => low + (high - low) * rng.random::<f64>(),
    }
}

/// Builds `description` into `rt`: one actor per neuron, placed by location; one route per neuron with
/// connection delays `distance / conduction speed` (at least `min_delay`); synapses on the receiving neurons.
///
/// # Panics
/// If the description refers to undefined neurons, learning functions or stickiness functions (a description
/// from [`spikes_dna::parse`] never does).
pub fn build(rt: &mut Runtime, description: &NetworkDescription, options: &BuildOptions) -> Network {
    let n = description.neurons.len();
    let index: HashMap<&str, usize> = description
        .neurons
        .iter()
        .enumerate()
        .map(|(i, d)| (d.id.as_str(), i))
        .collect();

    // Parameter tables, shared by all neurons. A population is a distinct combination of dynamics, receptors and
    // modulator release.
    let mut population_table: Table<Population> = Table::new();
    let populations: Vec<u32> = description
        .neurons
        .iter()
        .map(|d| population_table.insert((d.params.clone(), d.receptors.clone(), d.release)))
        .collect();
    let mut synapse_table: Table<SynapseParams> = Table::new();
    let connection_params: Vec<u16> = description
        .connections
        .iter()
        .map(|c| {
            let post = &description.neurons[index[c.post.as_str()]];
            let params = SynapseParams {
                decay: post.synapses.decay,
                limit: post.synapses.limit,
                stickiness: description.stickiness[&c.stickiness],
                release: post.synapses.release,
                learning: description.learning[&c.learning].clone(),
                weight_noise: post.synapses.weight_noise,
            };
            u16::try_from(synapse_table.insert(params)).expect("fewer than 65,536 distinct synapse parameter sets")
        })
        .collect();
    let direct: Vec<bool> = (0..description.modulators.len())
        .map(|k| {
            description
                .neurons
                .iter()
                .flat_map(|n| &n.receptors)
                .any(|r| usize::from(r.modulator) == k && r.direct_mv != 0.0)
        })
        .collect();
    let modulators: Arc<Vec<ModulatorKind>> = Arc::new(
        description
            .modulators
            .iter()
            .zip(&direct)
            .map(|(m, &direct)| ModulatorKind::from_description(m, direct))
            .collect(),
    );
    if !modulators.is_empty() {
        rt.add_field(ModulatorField::new(Arc::clone(&modulators)));
    }
    let (neurons, receptors, releases) = population_table.items.into_iter().fold(
        (Vec::new(), Vec::new(), Vec::new()),
        |(mut n, mut r, mut l), (neuron, receptors, release)| {
            n.push(neuron);
            r.push(receptors);
            l.push(release);
            (n, r, l)
        },
    );
    let params = Arc::new(NetworkParams {
        neurons,
        receptors,
        releases,
        modulators,
        synapses: synapse_table.items,
        record_weights: options.record_weights,
    });

    // Neurons, placed so that nearby neurons share a partition.
    let locations: Vec<[f64; 3]> = description.neurons.iter().map(|d| d.location_um).collect();
    let partitions = spatial_partitions(&locations, rt.partitions());
    let ids: Vec<ActorId> = (0..n)
        .map(|i| {
            let actor = NeuronActor::new(Arc::clone(&params), populations[i], locations[i]);
            let id = rt.spawn_in(partitions[i], actor);
            rt.set_position(id, locations[i]);
            id
        })
        .collect();

    // Connections: synapses on the receiver, route edges on the sender.
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(options.seed);
    let mut synapses: Vec<Vec<Synapse>> = vec![Vec::new(); n];
    let mut pre: Vec<Vec<ActorId>> = vec![Vec::new(); n];
    let mut edges: Vec<Vec<RouteEdge>> = vec![Vec::new(); n];
    for (c, &synapse_params) in description.connections.iter().zip(&connection_params) {
        let (from, to) = (index[c.pre.as_str()], index[c.post.as_str()]);
        let weight = draw(c.weight, &mut rng);
        let equilibrium = if c.equilibrium == c.weight {
            weight
        } else {
            draw(c.equilibrium, &mut rng)
        };
        let edge = EdgeIdx(u32::try_from(synapses[to].len()).expect("edge index fits u32"));
        synapses[to].push(Synapse::new(weight, equilibrium, c.sign, synapse_params));
        pre[to].push(ids[from]);

        let distance_um = locations[from]
            .iter()
            .zip(&locations[to])
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f64>()
            .sqrt();
        // Synaptic delay plus conduction along the axon.
        let neuron = &description.neurons[from];
        let synaptic = SimDuration::from_millis_f64(c.synaptic_delay_ms.unwrap_or(neuron.synaptic_delay_ms));
        let conduction = SimDuration::from_micros_f64(distance_um / neuron.conduction_m_per_s); // m/s = µm/µs
        let delay = (synaptic + conduction).max(options.min_delay);
        edges[from].push(RouteEdge {
            target: ids[to],
            delay,
            edge,
        });
    }
    for (i, ((synapses, pre), edges)) in synapses.into_iter().zip(pre).zip(edges).enumerate() {
        let route = (!edges.is_empty()).then(|| rt.add_route(ids[i], edges, options.jitter));
        let actor = rt.actor_mut::<NeuronActor>(ids[i]).expect("just spawned");
        actor.synapses = synapses;
        actor.pre = pre;
        actor.out = route;
    }

    let neurons: Vec<NeuronRef> = ids.iter().map(|&id| NeuronRef::new(id)).collect();
    Network {
        params,
        by_name: description
            .neurons
            .iter()
            .zip(&neurons)
            .map(|(d, &r)| (d.id.clone(), r))
            .collect(),
        names: description
            .neurons
            .iter()
            .zip(&ids)
            .map(|(d, &id)| (id, d.id.clone()))
            .collect(),
        neurons,
    }
}
