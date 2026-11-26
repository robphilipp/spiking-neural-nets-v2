//! Where a built network runs: its nodes and their shards, each neuron's node and shard, and how its populations
//! and sets are spread over the nodes, with the pins (`node=`) that asked for it.

use spikes_dna::NetworkDescription;

/// Where a built network runs.
#[derive(Clone, Debug, PartialEq)]
pub struct Deployment {
    /// Every node, this engine (`main`) first, then the node agents in the order given.
    pub nodes: Vec<DeployedNode>,
    /// Each neuron's node (an index into `nodes`).
    pub neuron_nodes: Vec<u32>,
    /// Each neuron's shard on its node.
    pub neuron_shards: Vec<u16>,
    /// The node each neuron is pinned to (`node=`; its own, its population's or a set's), if any, whether or not
    /// the pin could be honoured.
    pub neuron_pins: Vec<Option<String>>,
    /// The populations and sets, and neurons listed alone that are pinned, in description order.
    pub groups: Vec<DeployedGroup>,
    /// The nodes the description names (`nodes=[(name=…)]`).
    pub declared_nodes: Vec<String>,
    /// What doesn't fit: pins that weren't honoured, named nodes no agent is.
    pub warnings: Vec<String>,
}

/// One node of a deployment.
#[derive(Clone, Debug, PartialEq)]
pub struct DeployedNode {
    /// Its name: `main` for this engine, the name given to a node agent (`name=host:port`), or empty.
    pub name: String,
    /// `host:port` of a node agent; empty for this engine.
    pub address: String,
    /// Neurons on each of its shards.
    pub shard_neurons: Vec<u64>,
    /// Synapses it made (onto its own neurons), when built.
    pub synapses: u64,
}

/// What a group is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupKind {
    Population,
    Set,
    /// A neuron listed on its own (listed only when pinned).
    Neuron,
}

/// A population, set or pinned neuron, and where its neurons run.
#[derive(Clone, Debug, PartialEq)]
pub struct DeployedGroup {
    pub name: String,
    pub kind: GroupKind,
    /// The node it's pinned to (`node=`), if any.
    pub pinned: Option<String>,
    /// Its neurons on each node (as `Deployment::nodes`).
    pub per_node: Vec<u64>,
}

impl Deployment {
    /// `agents`: the node agents built on (`host:port` or `name=host:port`; empty for this engine alone). `homes`:
    /// each neuron's node and shard. `synapses`: each node's synapses.
    pub(crate) fn new(
        description: &NetworkDescription,
        agents: &[String],
        shards: usize,
        homes: &[(u32, u16)],
        synapses: &[u64],
        replay: bool,
    ) -> Self {
        let entries = crate::cluster::node_entries(agents);
        let mut nodes = vec![DeployedNode {
            name: "main".into(),
            address: String::new(),
            shard_neurons: vec![0; shards],
            synapses: 0,
        }];
        nodes.extend(entries.iter().map(|(name, address)| DeployedNode {
            name: name.clone().unwrap_or_default(),
            address: address.clone(),
            shard_neurons: vec![0; shards],
            synapses: 0,
        }));
        for (node, &count) in nodes.iter_mut().zip(synapses) {
            node.synapses = count;
        }
        for &(node, shard) in homes {
            if let Some(n) = nodes.get_mut(node as usize) {
                if n.shard_neurons.len() <= shard as usize {
                    n.shard_neurons.resize(shard as usize + 1, 0);
                }
                n.shard_neurons[shard as usize] += 1;
            }
        }

        let spread = |members: &mut dyn Iterator<Item = u32>| {
            let mut per_node = vec![0u64; nodes.len()];
            for i in members {
                if let Some(&(node, _)) = homes.get(i as usize) {
                    per_node[(node as usize).min(nodes.len() - 1)] += 1;
                }
            }
            per_node
        };
        let mut groups: Vec<DeployedGroup> = Vec::new();
        for (i, neuron) in description.neurons.iter().enumerate() {
            if neuron.population().is_none() && neuron.node().is_some() {
                groups.push(DeployedGroup {
                    name: neuron.id().into_owned(),
                    kind: GroupKind::Neuron,
                    pinned: neuron.node().map(str::to_string),
                    per_node: spread(&mut std::iter::once(i as u32)),
                });
            }
        }
        for p in &description.populations {
            groups.push(DeployedGroup {
                name: p.name.clone(),
                kind: GroupKind::Population,
                pinned: p.node.clone(),
                per_node: spread(&mut (p.first..p.first + p.count)),
            });
        }
        for s in &description.sets {
            groups.push(DeployedGroup {
                name: s.name.clone(),
                kind: GroupKind::Set,
                pinned: s.node.clone(),
                per_node: spread(&mut s.members.iter().copied()),
            });
        }

        let mut warnings = Vec::new();
        let named = |name: &str| nodes.iter().any(|n| n.name == name);
        if replay {
            warnings.push("A replay runs on this engine alone: pins are ignored.".into());
        } else if nodes.len() == 1 {
            if groups.iter().any(|g| g.pinned.as_deref().is_some_and(|p| p != "main")) {
                warnings.push(
                    "Built on this engine alone: pins to other nodes are ignored. Name node agents in Settings \
                     (name=host:port) to place them."
                        .into(),
                );
            }
        } else {
            // (A pin to a node no agent is refuses the build; a named node nothing is pinned to doesn't.)
            for name in &description.nodes {
                if !named(name) {
                    warnings.push(format!(
                        "The network names node '{name}', but no node agent is named '{name}'."
                    ));
                }
            }
        }
        Self {
            nodes,
            neuron_nodes: homes.iter().map(|h| h.0).collect(),
            neuron_shards: homes.iter().map(|h| h.1).collect(),
            neuron_pins: description.pinned_nodes(),
            groups,
            declared_nodes: description.nodes.clone(),
            warnings,
        }
    }
}
