//! The network description: what a `.boo` file says, with units converted and connection sets expanded.

use std::collections::BTreeMap;

use spikes_model::{LearningRule, NeuronParams, Release, Sign, Stickiness, WeightDecay, WeightLimit};

use crate::Span;

/// A whole network.
#[derive(Clone, Debug, PartialEq)]
pub struct NetworkDescription {
    /// Execution groups (`GRP`).
    pub groups: Vec<GroupDescription>,
    /// Neurons in file order (`NRN`).
    pub neurons: Vec<NeuronDescription>,
    /// Connections, one per (pre, post) pair after expanding id sets (`CON`).
    pub connections: Vec<ConnectionDescription>,
    /// Learning rules by name (`LRN`); `flat` (no learning) is always defined.
    pub learning: BTreeMap<String, LearningRule>,
    /// Weight-stickiness functions by name (`CST`); `none` is always defined.
    pub stickiness: BTreeMap<String, Stickiness>,
}

impl NetworkDescription {
    /// The neuron with this ID.
    pub fn neuron(&self, id: &str) -> Option<&NeuronDescription> {
        self.neurons.iter().find(|n| n.id == id)
    }
}

/// An execution group: where its neurons run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupDescription {
    /// Group ID (`gid`).
    pub id: String,
    /// Remote host and port (`hst`, `prt`), or `None` for the local host.
    pub remote: Option<(String, u16)>,
}

/// One neuron.
#[derive(Clone, Debug, PartialEq)]
pub struct NeuronDescription {
    /// Neuron ID (`nid`).
    pub id: String,
    /// Execution group (`grp`).
    pub group: String,
    /// Default sign of its outgoing connections (`inh`).
    pub inhibitory: bool,
    /// Dynamics parameters.
    pub params: NeuronParams,
    /// Axon conduction speed in m/s (`csp`); sets outgoing connection delays.
    pub conduction_m_per_s: f64,
    /// Soma position in µm, cartesian (`LOC`, converted from cylindrical or spherical if needed).
    pub location_um: [f64; 3],
    /// Settings for this neuron's *incoming* synapses (`WDF`, `WLF`, `SRP`, `wnm`).
    pub synapses: SynapseDefaults,
    /// Where it is defined.
    pub span: Span,
}

/// Settings a neuron gives its incoming synapses.
#[derive(Clone, Debug, PartialEq)]
pub struct SynapseDefaults {
    /// Weight decay toward equilibrium (`WDF`).
    pub decay: WeightDecay,
    /// Weight bounds (`WLF`).
    pub limit: WeightLimit,
    /// Stochastic release (`SRP`).
    pub release: Option<Release>,
    /// Weight noise magnitude (`wnm`).
    pub weight_noise: f64,
}

/// A connection weight: fixed, or drawn uniformly from a range when the network is built.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum WeightSpec {
    /// A fixed value.
    Fixed(f64),
    /// Uniform in `[low, high)`, drawn with the network's seed (`r0:1`).
    Random {
        /// Lower end.
        low: f64,
        /// Upper end.
        high: f64,
    },
}

/// One connection.
#[derive(Clone, Debug, PartialEq)]
pub struct ConnectionDescription {
    /// Pre-synaptic neuron ID.
    pub pre: String,
    /// Post-synaptic neuron ID.
    pub post: String,
    /// Initial weight (`cnw`).
    pub weight: WeightSpec,
    /// Equilibrium weight (`eqw`; defaults to the initial weight).
    pub equilibrium: WeightSpec,
    /// Learning rule name (`lrn`; default `flat`).
    pub learning: String,
    /// Stickiness function name (`cst`; default `none`).
    pub stickiness: String,
    /// Excitatory or inhibitory (`typ`; defaults from the pre-synaptic neuron's `inh`).
    pub sign: Sign,
    /// Where it is defined.
    pub span: Span,
}
