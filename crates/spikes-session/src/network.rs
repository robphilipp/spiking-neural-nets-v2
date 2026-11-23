//! What a built network looks like from outside: dense neuron indices, names, positions and synapses.

use std::collections::HashMap;

use spikes_dna::NetworkDescription;
use spikes_model::Sign;
use spikes_network::{Network, NeuronActor, Runtime};
use tempo_runtime::ActorId;

/// One neuron.
#[derive(Clone, Debug, PartialEq)]
pub struct NeuronInfo {
    /// Dense index, in description order.
    pub index: u32,
    /// Name (`nid`).
    pub name: String,
    /// Execution group.
    pub group: String,
    /// Dynamics class, e.g. `monostable_integrator`.
    pub kind: &'static str,
    /// Soma position in µm.
    pub position: [f64; 3],
    /// Whether its outgoing connections default to inhibitory.
    pub inhibitory: bool,
}

/// One synapse, as built.
#[derive(Clone, Debug, PartialEq)]
pub struct SynapseInfo {
    /// The receiving neuron's index.
    pub post: u32,
    /// The synapse's index on the receiving neuron.
    pub synapse: u32,
    /// The sending neuron's index.
    pub pre: u32,
    /// Initial weight.
    pub weight: f32,
    /// Inhibitory or excitatory.
    pub inhibitory: bool,
}

/// A built network, indexed densely.
#[derive(Clone, Debug, Default)]
pub struct NetworkInfo {
    /// Neurons by index.
    pub neurons: Vec<NeuronInfo>,
    /// Synapses, grouped by receiving neuron.
    pub synapses: Vec<SynapseInfo>,
    /// Modulator names, by modulator index.
    pub modulators: Vec<String>,
    /// Learning rules new connections can use.
    pub learning_rules: Vec<String>,
    /// Each neuron's spatial cell (for binned spike frames): a 16 × 16 × 16 grid over the network's bounding box,
    /// numbered `ix + 16·(iy + 16·iz)`.
    pub cells: Vec<u32>,
    ids: Vec<ActorId>,
    indices: HashMap<ActorId, u32>,
    names: HashMap<String, u32>,
}

impl NetworkInfo {
    pub(crate) fn new(rt: &Runtime, net: &Network, description: &NetworkDescription) -> Self {
        let ids: Vec<ActorId> = net.neurons.iter().map(|r| r.id()).collect();
        let indices: HashMap<ActorId, u32> = ids.iter().enumerate().map(|(i, &id)| (id, i as u32)).collect();
        let neurons: Vec<NeuronInfo> = description
            .neurons
            .iter()
            .zip(0u32..)
            .map(|(d, index)| NeuronInfo {
                index,
                name: d.id.clone(),
                group: d.group.clone(),
                kind: d.params.kind.name(),
                position: d.location_um,
                inhibitory: d.inhibitory,
            })
            .collect();
        let mut synapses = Vec::new();
        for (post, &id) in ids.iter().enumerate() {
            let Some(actor) = rt.actor::<NeuronActor>(id) else {
                continue;
            };
            for (i, (syn, pre)) in actor.synapses.iter().zip(&actor.pre).enumerate() {
                synapses.push(SynapseInfo {
                    post: post as u32,
                    synapse: i as u32,
                    pre: indices.get(pre).copied().unwrap_or(u32::MAX),
                    weight: syn.weight,
                    inhibitory: syn.sign == Sign::Inhibitory,
                });
            }
        }
        Self {
            cells: cells(&neurons),
            names: neurons.iter().map(|n| (n.name.clone(), n.index)).collect(),
            neurons,
            synapses,
            modulators: description.modulators.iter().map(|m| m.name.clone()).collect(),
            learning_rules: net.learning_rules(),
            ids,
            indices,
        }
    }

    /// The actor behind neuron `index`.
    pub fn id(&self, index: u32) -> Option<ActorId> {
        self.ids.get(index as usize).copied()
    }

    /// A network for a replayed recording: neurons and synapses as recorded, played by the actors `ids`.
    pub(crate) fn replayed(neurons: Vec<NeuronInfo>, synapses: Vec<SynapseInfo>, ids: Vec<ActorId>) -> Self {
        Self {
            cells: cells(&neurons),
            names: neurons.iter().map(|n| (n.name.clone(), n.index)).collect(),
            indices: ids.iter().enumerate().map(|(i, &id)| (id, i as u32)).collect(),
            ids,
            neurons,
            synapses,
            modulators: Vec::new(),
            learning_rules: Vec::new(),
        }
    }

    /// The index of the neuron behind actor `id`.
    pub fn index(&self, id: ActorId) -> Option<u32> {
        self.indices.get(&id).copied()
    }

    /// The index of the neuron with this name.
    pub fn by_name(&self, name: &str) -> Option<u32> {
        self.names.get(name).copied()
    }

    /// The center of the neurons' positions (where the default modulator release site sits).
    pub fn centroid(&self) -> [f64; 3] {
        let n = self.neurons.len().max(1) as f64;
        let mut c = [0.0; 3];
        for neuron in &self.neurons {
            for (c, p) in c.iter_mut().zip(neuron.position) {
                *c += p / n;
            }
        }
        c
    }
}

/// Cells per axis of the spatial grid.
pub const GRID: u32 = 16;

fn cells(neurons: &[NeuronInfo]) -> Vec<u32> {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for n in neurons {
        for a in 0..3 {
            lo[a] = lo[a].min(n.position[a]);
            hi[a] = hi[a].max(n.position[a]);
        }
    }
    neurons
        .iter()
        .map(|n| {
            let mut cell = 0;
            for a in (0..3).rev() {
                let extent = hi[a] - lo[a];
                let i = if extent > 0.0 {
                    (((n.position[a] - lo[a]) / extent * f64::from(GRID)) as u32).min(GRID - 1)
                } else {
                    0
                };
                cell = cell * GRID + i;
            }
            cell
        })
        .collect()
}
