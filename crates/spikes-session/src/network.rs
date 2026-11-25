//! What a built network looks like from outside: dense neuron indices, names, positions and synapses.

use std::sync::Arc;

use spikes_dna::{NetworkDescription, NeuronNames};
use spikes_network::Network;
use tempo_runtime::ActorId;

/// Networks with more synapses than this don't list them (in summaries, over the API, in the UI).
pub const SYNAPSE_LIST_LIMIT: u64 = 500_000;

/// One neuron.
#[derive(Clone, Debug, PartialEq)]
pub struct NeuronInfo {
    /// Dense index, in description order.
    pub index: u32,
    /// Name (`nid`).
    pub name: String,
    /// Its population (empty for a neuron listed on its own).
    pub population: String,
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

/// A network's neurons, by index: compact (a name table shared with the built network, and per neuron only its
/// kind and position), with each [`NeuronInfo`] made when asked for.
#[derive(Clone, Debug)]
pub struct NeuronList(List);

#[derive(Clone, Debug)]
enum List {
    Described {
        names: Arc<NeuronNames>,
        /// (population, dynamics class, inhibitory) of each kind of neuron (a population, or a neuron listed alone).
        kinds: Vec<(String, &'static str, bool)>,
        /// Each neuron's kind and position.
        neurons: Vec<(u32, [f64; 3])>,
    },
    /// As recorded (a replay).
    Listed(Vec<NeuronInfo>),
}

impl Default for NeuronList {
    fn default() -> Self {
        Self(List::Listed(Vec::new()))
    }
}

impl NeuronList {
    fn described(description: &NetworkDescription, names: Arc<NeuronNames>) -> Self {
        // Kinds by what they show (neurons listed one by one share them too).
        let mut kinds: Vec<(String, &'static str, bool)> = Vec::new();
        let mut index: std::collections::HashMap<(&str, &'static str, bool), u32> = std::collections::HashMap::new();
        let neurons = description
            .neurons
            .iter()
            .map(|d| {
                let key = (
                    d.population().unwrap_or_default(),
                    d.params().kind.name(),
                    d.inhibitory(),
                );
                let kind = *index.entry(key).or_insert_with(|| {
                    kinds.push((key.0.to_string(), key.1, key.2));
                    kinds.len() as u32 - 1
                });
                (kind, d.location_um())
            })
            .collect();
        Self(List::Described { names, kinds, neurons })
    }

    /// How many.
    pub fn len(&self) -> usize {
        match &self.0 {
            List::Described { neurons, .. } => neurons.len(),
            List::Listed(neurons) => neurons.len(),
        }
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Neuron `index`.
    pub fn get(&self, index: u32) -> Option<NeuronInfo> {
        match &self.0 {
            List::Described { names, kinds, neurons } => {
                let &(kind, position) = neurons.get(index as usize)?;
                let (population, kind, inhibitory) = &kinds[kind as usize];
                Some(NeuronInfo {
                    index,
                    name: names.name(index)?.into_owned(),
                    population: population.clone(),
                    kind,
                    position,
                    inhibitory: *inhibitory,
                })
            }
            List::Listed(neurons) => neurons.get(index as usize).cloned(),
        }
    }

    /// Neuron `index`'s position in µm.
    pub fn position(&self, index: u32) -> Option<[f64; 3]> {
        match &self.0 {
            List::Described { neurons, .. } => neurons.get(index as usize).map(|n| n.1),
            List::Listed(neurons) => neurons.get(index as usize).map(|n| n.position),
        }
    }

    /// Every neuron, in order.
    pub fn iter(&self) -> impl Iterator<Item = NeuronInfo> + '_ {
        (0..self.len() as u32).map(|i| self.get(i).expect("in range"))
    }

    /// The index of the neuron with this name.
    pub fn index(&self, name: &str) -> Option<u32> {
        match &self.0 {
            List::Described { names, .. } => names.index(name),
            List::Listed(neurons) => neurons.iter().position(|n| n.name == name).map(|i| i as u32),
        }
    }

    fn positions(&self) -> impl Iterator<Item = [f64; 3]> + '_ {
        (0..self.len() as u32).map(|i| self.position(i).expect("in range"))
    }
}

/// A built network, indexed densely.
#[derive(Clone, Debug, Default)]
pub struct NetworkInfo {
    /// Neurons by index.
    pub neurons: NeuronList,
    /// Synapses, grouped by receiving neuron: all of them, up to [`SYNAPSE_LIST_LIMIT`]; none beyond (ask a neuron
    /// for its own).
    pub synapses: Vec<SynapseInfo>,
    /// How many synapses there are.
    pub synapse_count: u64,
    /// Modulator names, by modulator index.
    pub modulators: Vec<String>,
    /// Learning rules new connections can use.
    pub learning_rules: Vec<String>,
    /// Each neuron's spatial cell (for binned spike frames): a 16 × 16 × 16 grid over the network's bounding box,
    /// numbered `ix + 16·(iy + 16·iz)`.
    pub cells: Vec<u32>,
    ids: Vec<ActorId>,
    /// (actor, index), by actor.
    indices: Vec<(ActorId, u32)>,
}

impl NetworkInfo {
    /// The network's neurons, and its synapses as given (every node's, on a cluster; see `synapse_rows`).
    pub(crate) fn new(net: &Network, description: &NetworkDescription, rows: &[spikes_record::SynapseRow]) -> Self {
        let ids: Vec<ActorId> = net.neurons.iter().map(|r| r.id()).collect();
        let indices = sorted_indices(&ids);
        let neurons = NeuronList::described(description, Arc::clone(net.names()));
        let index_of = |raw: u64| {
            let id = ActorId::from_raw(raw);
            indices
                .binary_search_by_key(&id, |&(id, _)| id)
                .map_or(u32::MAX, |i| indices[i].1)
        };
        let mut synapses: Vec<SynapseInfo> = rows
            .iter()
            .map(|r| SynapseInfo {
                post: index_of(r.post),
                synapse: r.synapse,
                pre: index_of(r.pre),
                weight: r.weight,
                inhibitory: r.inhibitory,
            })
            .collect();
        // Grouped by receiving neuron, in synapse order, whichever node each came from.
        synapses.sort_by_key(|s| (s.post, s.synapse));
        Self {
            cells: cells(&neurons),
            neurons,
            synapses,
            synapse_count: net.synapse_count,
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
        let neurons = NeuronList(List::Listed(neurons));
        Self {
            cells: cells(&neurons),
            indices: sorted_indices(&ids),
            ids,
            neurons,
            synapse_count: synapses.len() as u64,
            synapses,
            modulators: Vec::new(),
            learning_rules: Vec::new(),
        }
    }

    /// Whether [`Self::synapses`] lists every synapse.
    pub fn lists_synapses(&self) -> bool {
        self.synapses.len() as u64 == self.synapse_count
    }

    /// The index of the neuron behind actor `id`.
    pub fn index(&self, id: ActorId) -> Option<u32> {
        let i = self.indices.binary_search_by_key(&id, |&(id, _)| id).ok()?;
        Some(self.indices[i].1)
    }

    /// The index of the neuron with this name.
    pub fn by_name(&self, name: &str) -> Option<u32> {
        self.neurons.index(name)
    }

    /// The center of the neurons' positions (where the default modulator release site sits).
    pub fn centroid(&self) -> [f64; 3] {
        let n = self.neurons.len().max(1) as f64;
        let mut c = [0.0; 3];
        for position in self.neurons.positions() {
            for (c, p) in c.iter_mut().zip(position) {
                *c += p / n;
            }
        }
        c
    }
}

/// Cells per axis of the spatial grid.
pub const GRID: u32 = 16;

fn sorted_indices(ids: &[ActorId]) -> Vec<(ActorId, u32)> {
    let mut indices: Vec<(ActorId, u32)> = ids.iter().copied().zip(0u32..).collect();
    indices.sort_unstable();
    indices
}

fn cells(neurons: &NeuronList) -> Vec<u32> {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for position in neurons.positions() {
        for a in 0..3 {
            lo[a] = lo[a].min(position[a]);
            hi[a] = hi[a].max(position[a]);
        }
    }
    neurons
        .positions()
        .map(|position| {
            let mut cell = 0;
            for a in (0..3).rev() {
                let extent = hi[a] - lo[a];
                let i = if extent > 0.0 {
                    (((position[a] - lo[a]) / extent * f64::from(GRID)) as u32).min(GRID - 1)
                } else {
                    0
                };
                cell = cell * GRID + i;
            }
            cell
        })
        .collect()
}
