//! The network description: what a `.boo` file says, with units converted and connection sets expanded.

use std::collections::BTreeMap;

use spikes_model::{LearningRule, NeuronParams, Receptor, Release, Sign, Stickiness, WeightDecay, WeightLimit};

use crate::Span;

/// A whole network.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NetworkDescription {
    /// Execution groups (`GRP`).
    pub groups: Vec<GroupDescription>,
    /// Neurons in file order (`NRN`), then every population's neurons (`POP`), population by population.
    pub neurons: crate::Neurons,
    /// Populations (`POP`): which ranges of `neurons` they are.
    #[serde(default)]
    pub populations: Vec<PopulationDescription>,
    /// Connections, one per (pre, post) pair after expanding id sets (`CON`).
    pub connections: Vec<ConnectionDescription>,
    /// Connection rules (`RUL`), expanded when the network is built: after `connections`, rule by rule, and for
    /// each rule receiving neuron by receiving neuron.
    #[serde(default)]
    pub rules: Vec<ConnectionRule>,
    /// Learning rules by name (`LRN`); `flat` (no learning) is always defined.
    pub learning: BTreeMap<String, LearningRule>,
    /// Weight-stickiness functions by name (`CST`); `none` is always defined.
    pub stickiness: BTreeMap<String, Stickiness>,
    /// Neuromodulators (`MOD`), in definition order: receptors and releases refer to them by index.
    pub modulators: Vec<ModulatorDescription>,
}

/// A neuromodulator: how it spreads from a release and how fast it is cleared.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ModulatorDescription {
    /// Name (`nam`).
    pub name: String,
    /// Clearance time constant in ms (`clr`).
    pub clearance_ms: f64,
    /// Reach in µm (`rad`; infinite by default).
    pub radius_um: f64,
    /// Spreading speed in m/s (`spd`; infinite, i.e. instantaneous, by default).
    pub speed_m_per_s: f64,
    /// Delay from release to the start of spreading, in ms (`lat`; default 1 ms). With several shards it must be
    /// at least the lookahead, so every shard learns of a release before it reaches any neuron.
    pub latency_ms: f64,
    /// Spatial falloff length in µm (`len`): amplitude × `exp(−distance/len)`; `None` means no falloff.
    pub falloff_um: Option<f64>,
}

impl NetworkDescription {
    /// The neuron with this ID (for many lookups, use [`crate::Neurons::names`]).
    pub fn neuron(&self, id: &str) -> Option<crate::NeuronView<'_>> {
        let i = self.neurons.names().index(id)?;
        self.neurons.get(i as usize)
    }
}

/// An execution group: where its neurons run.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GroupDescription {
    /// Group ID (`gid`).
    pub id: String,
    /// Remote host and port (`hst`, `prt`), or `None` for the local host.
    pub remote: Option<(String, u16)>,
}

/// One neuron.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NeuronDescription {
    /// Neuron ID (`nid`).
    pub id: String,
    /// Execution group (`grp`).
    pub group: String,
    /// Default sign of its outgoing connections (`inh`).
    pub inhibitory: bool,
    /// Dynamics parameters.
    pub params: NeuronParams,
    /// Axon conduction speed in m/s (`csp`); with the distance, sets outgoing connection delays.
    pub conduction_m_per_s: f64,
    /// Synaptic delay of its outgoing connections in ms (`syd`; default 0.5 ms): the time from a spike's arrival at
    /// a synapse to the current in the receiving neuron. A connection's delay is `syd + distance / csp`.
    pub synaptic_delay_ms: f64,
    /// Soma position in µm, cartesian (`LOC`, converted from cylindrical or spherical if needed).
    pub location_um: [f64; 3],
    /// Settings for this neuron's *incoming* synapses (`WDF`, `WLF`, `SRP`, `wnm`).
    pub synapses: SynapseDefaults,
    /// Neuromodulator receptors (`RCP`).
    pub receptors: Vec<Receptor>,
    /// Modulator released when this neuron spikes, and how much (`REL`).
    pub release: Option<(u16, f64)>,
    /// Where it is defined.
    pub span: Span,
}

/// Settings a neuron gives its incoming synapses.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
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
#[derive(Copy, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
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
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
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
    /// Synaptic delay in ms (`syd`), overriding the pre-synaptic neuron's.
    pub synaptic_delay_ms: Option<f64>,
    /// Where it is defined.
    pub span: Span,
}

/// A population: neurons alike, placed at random in a region (`POP`). Its neurons are in
/// [`NetworkDescription::neurons`] at `first .. first + count`.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PopulationDescription {
    /// Name (`nam`); its neurons are `<name>-<i>`.
    pub name: String,
    /// Index of its first neuron.
    pub first: u32,
    /// How many (`cnt`).
    pub count: u32,
    /// Where they were placed (`REG`).
    pub region: Region,
    /// Where it is defined.
    pub span: Span,
}

/// A region of space, in µm.
#[derive(Copy, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Region {
    /// A box from `corner` with sides `size`.
    Box {
        /// The lowest corner.
        corner: [f64; 3],
        /// Side lengths.
        size: [f64; 3],
    },
    /// A ball.
    Ball {
        /// Center.
        center: [f64; 3],
        /// Radius.
        radius: f64,
    },
}

/// How many inputs a rule gives each receiving neuron.
#[derive(Copy, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum InDegree {
    /// Exactly this many (fewer if there aren't enough candidates), from different senders (`cnt`).
    Fixed(u32),
    /// Each candidate sender independently with this probability (`prb`).
    Probability(f64),
}

/// A connection rule (`RUL`): random connections from a set of senders to each of a set of receivers.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ConnectionRule {
    /// Senders: a population or group name (`prp`).
    pub pre: String,
    /// Receivers (`psp`).
    pub post: String,
    /// Inputs per receiver.
    pub in_degree: InDegree,
    /// Only senders within this distance in µm (`rad`).
    pub radius_um: Option<f64>,
    /// A neuron may connect to itself (`aut`).
    pub autapses: bool,
    /// Initial weight (`cnw`).
    pub weight: WeightSpec,
    /// Equilibrium weight (`eqw`).
    pub equilibrium: WeightSpec,
    /// Learning rule name (`lrn`).
    pub learning: String,
    /// Stickiness function name (`cst`).
    pub stickiness: String,
    /// Sign (`typ`); `None`: from each sender's `inh`.
    pub sign: Option<Sign>,
    /// Synaptic delay in ms, overriding the senders' (`syd`).
    pub synaptic_delay_ms: Option<f64>,
    /// Where it is defined.
    pub span: Span,
}

impl NetworkDescription {
    /// The neurons (indices into [`Self::neurons`], ascending) of a population or group, if one has that name. A
    /// population wins over a group of the same name.
    pub fn members(&self, name: &str) -> Option<Vec<u32>> {
        if let Some(p) = self.populations.iter().find(|p| p.name == name) {
            return Some((p.first..p.first + p.count).collect());
        }
        let members: Vec<u32> = self
            .neurons
            .iter()
            .zip(0u32..)
            .filter(|(n, _)| n.group() == name)
            .map(|(_, i)| i)
            .collect();
        (!members.is_empty() || self.groups.iter().any(|g| g.id == name)).then_some(members)
    }
}
