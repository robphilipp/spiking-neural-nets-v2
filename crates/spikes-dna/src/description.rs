//! The network description: what a `.boo` file says, with units converted, types and overrides applied, and every
//! reference resolved (see `docs/plans/dna/README.md`).

use std::collections::BTreeMap;

use spikes_model::{LearningRule, NeuronParams, Receptor, Release, Sign, Stickiness, WeightDecay, WeightLimit};

use crate::Span;

/// A whole network.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NetworkDescription {
    /// Machines the network may run on besides the engine itself (`nodes`), for pinning neurons with `node=`.
    #[serde(default)]
    pub nodes: Vec<String>,
    /// Every neuron: listed ones (`neurons`), then each population's (`populations`), in order.
    pub neurons: crate::Neurons,
    /// Populations: which ranges of `neurons` they are.
    #[serde(default)]
    pub populations: Vec<PopulationDescription>,
    /// Named sets of neurons (`sets`).
    #[serde(default)]
    pub sets: Vec<SetDescription>,
    /// Connections (`connections`), expanded when the network is built: in order, and for each receiving neuron by
    /// receiving neuron.
    pub connections: Vec<ConnectionDescription>,
    /// Learning rules by name (`learning_rules`); `flat` (no learning) is always defined.
    pub learning: BTreeMap<String, LearningRule>,
    /// Weight-stickiness functions by name (`stickiness`); `none` is always defined.
    pub stickiness: BTreeMap<String, Stickiness>,
    /// Neuromodulators (`modulators`), in definition order: receptors and releases refer to them by index.
    pub modulators: Vec<ModulatorDescription>,
}

/// A neuromodulator: how it spreads from a release and how fast it is cleared.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ModulatorDescription {
    /// Name.
    pub name: String,
    /// Clearance time constant in ms (`clearance_time`).
    pub clearance_ms: f64,
    /// Reach in µm (`reach`; infinite by default).
    pub radius_um: f64,
    /// Spreading speed in m/s (`speed`; infinite, i.e. instantaneous, by default).
    pub speed_m_per_s: f64,
    /// Delay from release to the start of spreading, in ms (`latency`; default 1 ms). With several shards it must be
    /// at least the lookahead, so every shard learns of a release before it reaches any neuron.
    pub latency_ms: f64,
    /// Spatial falloff length in µm (`falloff`): amplitude × `exp(−distance/len)`; `None` means no falloff.
    pub falloff_um: Option<f64>,
}

/// One neuron: its type's settings with its own overrides applied, and where it is.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NeuronDescription {
    /// Its name.
    pub id: String,
    /// The node it's pinned to (`node`), if any.
    #[serde(default)]
    pub node: Option<String>,
    /// Default sign of its outgoing connections (`inhibitory`).
    pub inhibitory: bool,
    /// Dynamics parameters.
    pub params: NeuronParams,
    /// Axon conduction speed in m/s (`conduction_speed`): connection delays add `distance / speed`.
    pub conduction_m_per_s: f64,
    /// Soma position in µm, cartesian.
    pub location_um: [f64; 3],
    /// Neuromodulator receptors (`receptors`).
    pub receptors: Vec<Receptor>,
    /// Modulator released when this neuron spikes, and how much (`releases`).
    pub release: Option<(u16, f64)>,
    /// Where it is defined.
    pub span: Span,
}

/// A value drawn for each connection when the network is built (with the network's seed), or fixed.
#[derive(Copy, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum WeightSpec {
    /// A fixed value.
    Fixed(f64),
    /// Uniform in `[low, high)` (`uniform(low, high)`).
    Uniform {
        /// Lower end.
        low: f64,
        /// Upper end.
        high: f64,
    },
    /// Normal (`normal(mean, sd)`).
    Normal {
        /// Mean.
        mean: f64,
        /// Standard deviation.
        sd: f64,
    },
}

/// Everything about a synapse: a synapse type's settings with a connection's overrides applied.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SynapseSpec {
    /// Initial weight.
    pub weight: WeightSpec,
    /// The weight decay relaxes toward; `None`: the initial weight.
    pub equilibrium: Option<WeightSpec>,
    /// Excitatory or inhibitory; `None`: from the sending neuron's `inhibitory`.
    pub sign: Option<Sign>,
    /// Synaptic delay in ms; the connection's delay adds `distance / conduction speed`.
    pub delay_ms: f64,
    /// Learning rule name (`flat`: none).
    pub learning: String,
    /// Stickiness function name (`none`: none).
    pub stickiness: String,
    /// Weight decay toward the equilibrium.
    pub decay: WeightDecay,
    /// Weight bounds.
    pub limit: WeightLimit,
    /// Stochastic transmitter release.
    pub release: Option<Release>,
    /// Noise added to the weight at each signal.
    pub weight_noise: f64,
}

impl SynapseSpec {
    /// Synaptic delay of a synapse nothing says anything about, ms.
    pub const DEFAULT_DELAY_MS: f64 = 0.5;

    /// A plain synapse: no learning, decay, limits, release or noise.
    pub fn plain(weight: f64) -> Self {
        Self {
            weight: WeightSpec::Fixed(weight),
            equilibrium: None,
            sign: None,
            delay_ms: Self::DEFAULT_DELAY_MS,
            learning: "flat".into(),
            stickiness: "none".into(),
            decay: WeightDecay::None,
            limit: WeightLimit::Unbounded,
            release: None,
            weight_noise: 0.0,
        }
    }
}

/// How a connection's senders and receivers are paired.
#[derive(Copy, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Pattern {
    /// Every sender to every receiver.
    AllToAll,
    /// The i-th sender to the i-th receiver.
    OneToOne,
    /// Each receiver gets this many inputs, from different senders chosen at random.
    FanIn(u32),
    /// Each (sender, receiver) pair with this probability.
    Random(f64),
}

/// Connections from some neurons to others (`connections`).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ConnectionDescription {
    /// Sending neurons (indices into [`NetworkDescription::neurons`]), in order.
    pub from: Vec<u32>,
    /// Receiving neurons, in order.
    pub to: Vec<u32>,
    /// How they're paired.
    pub pattern: Pattern,
    /// Only pairs closer than this, µm.
    pub within_um: Option<f64>,
    /// A neuron may connect to itself.
    pub autapses: bool,
    /// The synapses.
    pub synapse: SynapseSpec,
    /// Where it is defined.
    pub span: Span,
}

/// A population: neurons of one type, placed together (`populations`). Its neurons are in
/// [`NetworkDescription::neurons`] at `first .. first + count`.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PopulationDescription {
    /// Name; its neurons are `<name>-<i>` unless named one by one.
    pub name: String,
    /// Index of its first neuron.
    pub first: u32,
    /// How many.
    pub count: u32,
    /// The node it's pinned to, if any.
    #[serde(default)]
    pub node: Option<String>,
    /// Where it is defined.
    pub span: Span,
}

/// A named set of neurons (`sets`).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SetDescription {
    /// Name.
    pub name: String,
    /// Its neurons (indices, ascending, without repeats).
    pub members: Vec<u32>,
    /// The node it's pinned to, if any.
    #[serde(default)]
    pub node: Option<String>,
    /// Where it is defined.
    pub span: Span,
}

impl NetworkDescription {
    /// The neuron with this name (for many lookups, use [`crate::Neurons::names`]).
    pub fn neuron(&self, id: &str) -> Option<crate::NeuronView<'_>> {
        let i = self.neurons.names().index(id)?;
        self.neurons.get(i as usize)
    }

    /// The neurons (indices, ascending) of a population or set, if one has that name.
    pub fn members(&self, name: &str) -> Option<Vec<u32>> {
        if let Some(p) = self.populations.iter().find(|p| p.name == name) {
            return Some((p.first..p.first + p.count).collect());
        }
        self.sets.iter().find(|s| s.name == name).map(|s| s.members.clone())
    }

    /// The node each neuron is pinned to (`None`: placed automatically): its own `node` (or its population's), or
    /// else the first set's that pins it.
    pub fn pinned_nodes(&self) -> Vec<Option<String>> {
        let mut nodes: Vec<Option<String>> = self.neurons.iter().map(|n| n.node().map(str::to_string)).collect();
        for set in &self.sets {
            if let Some(node) = &set.node {
                for &i in &set.members {
                    let slot = &mut nodes[i as usize];
                    if slot.is_none() {
                        *slot = Some(node.clone());
                    }
                }
            }
        }
        nodes
    }
}

/// What `release=()` with nothing set means (v1's release model).
pub const DEFAULT_RELEASE: Release = Release::V1_DEFAULT;
