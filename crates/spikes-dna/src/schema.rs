//! The description language's schema: every record, its keys, what they mean, their units, defaults and allowed
//! values. The parser checks keys against it, and the editor's completions and hover help are generated from it
//! (`cargo test -p spikes-dna` writes `apps/spikes-ui/src/language/schema.json`), so the two can't disagree.

use serde::Serialize;

/// A record type.
#[derive(Debug, Serialize)]
pub struct RecordSchema {
    /// Its id, used by [`Value::Record`] and [`Value::List`].
    pub name: &'static str,
    /// What it is.
    pub doc: &'static str,
    /// The key whose value selects which other keys apply (`dynamics`, `rule`), if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind_key: Option<&'static str>,
    /// Its keys.
    pub keys: &'static [Key],
    /// Another record whose keys it also accepts, less these (a population takes every neuron setting).
    #[serde(skip)]
    pub extends: Option<(&'static str, &'static [&'static str])>,
}

/// One key of a record.
#[derive(Debug, Serialize)]
pub struct Key {
    /// The key as written.
    pub key: &'static str,
    /// What it means.
    pub doc: &'static str,
    /// What its value is.
    pub value: Value,
    /// The default, as written in a description (`None`: required, or absent means off).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<&'static str>,
    /// Required.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub required: bool,
    /// Only for these values of the record's kind key; empty means always.
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    pub only: &'static [&'static str],
    /// Accepted for older files but not suggested.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
}

/// A key's value.
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Value {
    /// A number with a unit of this kind; `unit` is the base unit written when none is given.
    Quantity {
        /// `potential`, `time`, `length`, `speed`, `frequency`, `flux`, `angle`.
        quantity: &'static str,
        /// Base unit.
        unit: &'static str,
    },
    /// A plain number.
    Number,
    /// One of these words.
    Choice {
        /// The words.
        values: &'static [&'static str],
    },
    /// A name (an identifier).
    Name,
    /// A reference to something defined elsewhere (a learning rule, a type…).
    Reference {
        /// What it refers to: `learning`, `stickiness`, `modulator`, `neuronType`, `synapseType`, `node`.
        to: &'static str,
    },
    /// Neurons: a population, a set, a neuron name or an id range (`input-1{1:7:1}`), or a list of them.
    Ids,
    /// `true` or `false`.
    Bool,
    /// A number, `uniform(low, high)` or `normal(mean, sd)`.
    Weight,
    /// A point in µm: `[x, y, z]`, `(r=…, phi=…, z=…)` (cylindrical) or `(r=…, theta=…, phi=…)` (spherical).
    Point,
    /// A list of points.
    Points,
    /// A record `(…)`.
    Record {
        /// The record type.
        record: &'static str,
    },
    /// A list of records `[(…), (…)]`.
    List {
        /// The element record type.
        record: &'static str,
    },
}

const fn key(key: &'static str, doc: &'static str, value: Value) -> Key {
    Key {
        key,
        doc,
        value,
        default: None,
        required: false,
        only: &[],
        hidden: false,
    }
}

const fn q(quantity: &'static str, unit: &'static str) -> Value {
    Value::Quantity { quantity, unit }
}

impl Key {
    const fn default(mut self, value: &'static str) -> Self {
        self.default = Some(value);
        self
    }
    const fn required(mut self) -> Self {
        self.required = true;
        self
    }
    const fn only(mut self, kinds: &'static [&'static str]) -> Self {
        self.only = kinds;
        self
    }
}

const MV: Value = q("potential", "mV");
const MS: Value = q("time", "ms");
const UM: Value = q("length", "µm");
const HZ: Value = q("frequency", "Hz");
const ANGLE: Value = q("angle", "rad");

/// The dynamics a neuron can have.
pub const DYNAMICS: &[&str] = &[
    "monostable_integrator",
    "bistable_integrator",
    "monostable_resonator",
    "bistable_resonator",
];
const INTEGRATORS_AND_RESONATORS: &[&str] = &["monostable_integrator", "monostable_resonator", "bistable_resonator"];
const RESONATORS: &[&str] = &["monostable_resonator", "bistable_resonator"];

/// The learning rules.
pub const RULES: &[&str] = &["stdp_hard", "stdp_soft", "stdp_alpha"];
const EXPONENTIAL_RULES: &[&str] = &["stdp_hard", "stdp_soft"];

/// The connection patterns.
pub const PATTERNS: &[&str] = &["all_to_all", "one_to_one", "fan_in", "random"];

/// Every record type.
pub static RECORDS: &[RecordSchema] = &[
    RecordSchema {
        name: "network",
        doc: "A whole network description.",
        kind_key: None,
        extends: None,
        keys: &[
            key(
                "nodes",
                "Machines (node agents) the network may run on besides the engine, named so neurons can be pinned to them \
                 with `node=`. Which machine each is, is set where the network runs (Settings).",
                Value::List { record: "node" },
            ),
            key(
                "neuron_types",
                "Neuron types: every neuron setting, for neurons and populations to use (`type=`) and override.",
                Value::List { record: "neuronType" },
            ),
            key(
                "synapse_types",
                "Synapse types: every synapse setting, for connections to use (`type=`) and override.",
                Value::List { record: "synapseType" },
            ),
            key("neurons", "Neurons one by one.", Value::List { record: "neuron" }),
            key(
                "populations",
                "Populations: many neurons of one type, placed together (named `<name>-0`, `<name>-1`, …).",
                Value::List { record: "population" },
            ),
            key(
                "sets",
                "Named sets of neurons, for connections and placement on nodes.",
                Value::List { record: "set" },
            ),
            key(
                "connections",
                "Connections: from some neurons to others, with a pattern and a synapse type.",
                Value::List { record: "connection" },
            ),
            key(
                "learning_rules",
                "Learning rules, referred to by synapses' `learning`.",
                Value::List { record: "learningRule" },
            ),
            key(
                "stickiness",
                "Weight-stickiness functions, referred to by synapses' `stickiness`.",
                Value::List { record: "stickiness" },
            ),
            key(
                "modulators",
                "Neuromodulators (dopamine, acetylcholine, …), referred to by receptors and releases.",
                Value::List { record: "modulator" },
            ),
        ],
    },
    RecordSchema {
        name: "node",
        doc: "A machine the network may run on (besides the engine, `main`).",
        kind_key: None,
        extends: None,
        keys: &[key("name", "Its name, for `node=`.", Value::Name).required()],
    },
    RecordSchema {
        name: "neuronSettings",
        doc: "Every setting of a neuron.",
        kind_key: Some("dynamics"),
        extends: None,
        keys: &[
            key(
                "dynamics",
                "`monostable_integrator`; `bistable_integrator` (fires tonically once excited); `monostable_resonator`; \
                 `bistable_resonator` (a limit cycle).",
                Value::Choice { values: DYNAMICS },
            )
            .default("monostable_integrator"),
            key(
                "inhibitory",
                "Its outgoing connections are inhibitory (unless their synapse type says otherwise).",
                Value::Bool,
            )
            .default("false"),
            key("threshold", "Firing threshold.", MV).default("10 mV").only(INTEGRATORS_AND_RESONATORS),
            key("tonic_on_threshold", "The threshold that starts tonic firing.", MV)
                .default("15 mV")
                .only(&["bistable_integrator"]),
            key("tonic_off_threshold", "The threshold below which tonic firing stops.", MV)
                .default("10 mV")
                .only(&["bistable_integrator"]),
            key("tonic_rate", "The tonic firing rate.", HZ).default("10 Hz").only(&["bistable_integrator"]),
            key(
                "refractory_period",
                "Absolute refractory period after a spike: input is lost and the potential held at rest.",
                MS,
            )
            .default("20 ms"),
            key(
                "relative_refractoriness",
                "Relative refractoriness after the absolute period: a kernel of this area (mV·ms) lowering the \
                 potential, decaying with time since the spike.",
                q("flux", "µWb"),
            )
            .default("0.08 µWb"),
            key("resting_potential", "Minimum (resting) membrane potential.", MV).default("0 mV"),
            key("decay_time", "Membrane potential decay time constant.", MS).default("250 ms"),
            key("rise_time", "How long a crossing input takes to make the spike.", MS).default("2 ms"),
            key("membrane_noise", "Membrane noise: a Wiener process of this magnitude per √ms.", MV).default("0.1 mV"),
            key(
                "spike_potential",
                "Spike amplitude sent along outgoing connections (scaled by each synapse's weight).",
                MV,
            )
            .default("1 mV"),
            key(
                "conduction_speed",
                "Axon conduction speed: an outgoing connection's delay is its synaptic delay plus distance / speed.",
                q("speed", "m/s"),
            )
            .default("10 m/s"),
            key(
                "intrinsic_plasticity",
                "Intrinsic plasticity: the threshold shifts with each spike.",
                Value::Record { record: "intrinsicPlasticity" },
            ),
            key(
                "homeostasis",
                "Homeostatic intrinsic plasticity: steer the firing rate toward a target.",
                Value::Record { record: "homeostasis" },
            ),
            key("resonance", "Resonator settings.", Value::Record { record: "resonance" }).only(RESONATORS),
            key(
                "receptors",
                "Neuromodulator receptors: how modulators change this neuron.",
                Value::List { record: "receptor" },
            ),
            key(
                "releases",
                "A modulator this neuron releases when it spikes.",
                Value::Record { record: "modulatorRelease" },
            ),
        ],
    },
    RecordSchema {
        name: "neuronType",
        doc: "A neuron type: settings for neurons and populations to use and override.",
        kind_key: Some("dynamics"),
        extends: Some(("neuronSettings", &[])),
        keys: &[
            key("name", "Its name, for `type=`.", Value::Name).required(),
            key("extends", "Another neuron type whose settings this one starts from.", Value::Reference { to: "neuronType" }),
        ],
    },
    RecordSchema {
        name: "neuron",
        doc: "A neuron: a type, its own settings (overriding the type's), and where it is.",
        kind_key: Some("dynamics"),
        extends: Some(("neuronSettings", &[])),
        keys: &[
            key("name", "Its name; connections and sets refer to it.", Value::Name).required(),
            key("type", "Its neuron type.", Value::Reference { to: "neuronType" }),
            key("position", "Where its soma is.", Value::Point).required(),
            key("node", "The machine it runs on (default: placed automatically).", Value::Reference { to: "node" }),
        ],
    },
    RecordSchema {
        name: "population",
        doc: "Many neurons of one type, placed together: by a list of positions, at random in a box or ball, on a grid, \
              along a line, or around a ring.",
        kind_key: Some("dynamics"),
        extends: Some(("neuronSettings", &[])),
        keys: &[
            key(
                "name",
                "Its name; its neurons are `<name>-0`, `<name>-1`, … (unless named in `positions`).",
                Value::Name,
            )
            .required(),
            key("type", "Its neuron type.", Value::Reference { to: "neuronType" }),
            key("count", "How many neurons (for `random`, `line`, `ring`).", Value::Number),
            key(
                "positions",
                "These points, in order; or `[(name=…, at=…), …]` to name the neurons too.",
                Value::Points,
            ),
            key("random", "At random in a box or a ball.", Value::Record { record: "randomPlacement" }),
            key("grid", "On a regular grid.", Value::Record { record: "grid" }),
            key("line", "Evenly along a line.", Value::Record { record: "line" }),
            key("ring", "Evenly around a circle.", Value::Record { record: "ring" }),
            key("seed", "Seed for random positions.", Value::Number).default("1"),
            key("node", "The machine it runs on (default: placed automatically).", Value::Reference { to: "node" }),
        ],
    },
    RecordSchema {
        name: "namedPosition",
        doc: "A neuron of a population, named, and where it is.",
        kind_key: None,
        extends: None,
        keys: &[
            key("name", "Its name.", Value::Name).required(),
            key("at", "Where it is.", Value::Point).required(),
        ],
    },
    RecordSchema {
        name: "randomPlacement",
        doc: "Neurons placed uniformly at random: in a box or a ball.",
        kind_key: None,
        extends: None,
        keys: &[
            key("box", "A box.", Value::Record { record: "box" }),
            key("ball", "A ball.", Value::Record { record: "ball" }),
        ],
    },
    RecordSchema {
        name: "box",
        doc: "A box, from a corner.",
        kind_key: None,
        extends: None,
        keys: &[
            key("corner", "Its lowest corner.", Value::Point).required(),
            key("size", "Its sides: `[x, y, z]`.", Value::Point).required(),
        ],
    },
    RecordSchema {
        name: "ball",
        doc: "A ball.",
        kind_key: None,
        extends: None,
        keys: &[
            key("center", "Its center.", Value::Point).required(),
            key("radius", "Its radius.", UM).required(),
        ],
    },
    RecordSchema {
        name: "grid",
        doc: "A regular grid of nx × ny × nz neurons.",
        kind_key: None,
        extends: None,
        keys: &[
            key("origin", "The first neuron's position.", Value::Point).default("[0, 0, 0]"),
            key("spacing", "Distance between neighbours along x, y and z: `[dx, dy, dz]`.", Value::Point).required(),
            key("shape", "How many along x, y and z: `[nx, ny, nz]`.", Value::Point).required(),
        ],
    },
    RecordSchema {
        name: "line",
        doc: "`count` neurons evenly along a line, both ends included.",
        kind_key: None,
        extends: None,
        keys: &[
            key("from", "The first neuron's position.", Value::Point).required(),
            key("to", "The last neuron's position.", Value::Point).required(),
        ],
    },
    RecordSchema {
        name: "ring",
        doc: "`count` neurons evenly around a circle.",
        kind_key: None,
        extends: None,
        keys: &[
            key("center", "The circle's center.", Value::Point).default("[0, 0, 0]"),
            key("radius", "Its radius.", UM).required(),
            key("axis", "The axis it goes around.", Value::Choice { values: &["x", "y", "z"] }).default("z"),
            key("start_angle", "Where the first neuron is.", ANGLE).default("0"),
        ],
    },
    RecordSchema {
        name: "set",
        doc: "A named set of neurons.",
        kind_key: None,
        extends: None,
        keys: &[
            key("name", "Its name; connections and other sets refer to it.", Value::Name).required(),
            key("members", "Populations, sets, neuron names and id ranges.", Value::Ids).required(),
            key(
                "node",
                "The machine its neurons run on (unless they're pinned themselves).",
                Value::Reference { to: "node" },
            ),
        ],
    },
    RecordSchema {
        name: "intrinsicPlasticity",
        doc: "Intrinsic plasticity: the threshold shifts with each spike, and relaxes between spikes.",
        kind_key: None,
        extends: None,
        keys: &[
            key("step", "Threshold shift added at each spike.", MV).default("0 mV"),
            key("amplitude", "Learning amplitude.", MV).default("0 mV"),
            key("decay_time", "Decay time constant between spikes.", MS).default("10 min"),
        ],
    },
    RecordSchema {
        name: "resonance",
        doc: "Resonate-and-fire dynamics: the potential oscillates as it decays.",
        kind_key: None,
        extends: None,
        keys: &[
            key("frequency", "Resonant frequency.", HZ).required(),
            key("damping_time", "Damping time constant of the oscillation.", MS).required(),
            key("limit_cycle", "Bistable resonator: radius of the tonic limit cycle.", MV),
            key(
                "separatrix",
                "Bistable resonator: separatrix radius; pulses that leave the state inside it stop tonic firing.",
                MV,
            ),
        ],
    },
    RecordSchema {
        name: "homeostasis",
        doc: "Homeostatic intrinsic plasticity: the threshold drifts so the firing rate approaches a target.",
        kind_key: None,
        extends: None,
        keys: &[
            key("target_rate", "Target firing rate.", HZ).required(),
            key("strength", "mV of threshold shift per Hz of error per second.", Value::Number).required(),
            key("window", "Window over which the rate is estimated.", MS).default("1 s"),
            key("min_shift", "Lowest threshold shift.", MV),
            key("max_shift", "Highest threshold shift.", MV),
        ],
    },
    RecordSchema {
        name: "receptor",
        doc: "A neuromodulator receptor: each effect is linear in the modulator's concentration c at the neuron.",
        kind_key: None,
        extends: None,
        keys: &[
            key("modulator", "The modulator.", Value::Reference { to: "modulator" }).required(),
            key("excitatory_gain", "Excitatory input × (1 + gain·c).", Value::Number).default("0"),
            key("inhibitory_gain", "Inhibitory input × (1 + gain·c).", Value::Number).default("0"),
            key("threshold_gain", "Threshold × (1 + gain·c).", Value::Number).default("0"),
            key("plasticity_gain", "STDP changes × (1 + gain·c).", Value::Number).default("0"),
            key(
                "eligibility_time",
                "Three-factor learning: eligibility trace time constant (with `learning_rate`).",
                MS,
            ),
            key(
                "learning_rate",
                "Three-factor learning: weight change per unit eligibility × concentration × second.",
                Value::Number,
            ),
            key("direct_potential", "c × this added to the membrane potential.", MV).default("0 mV"),
        ],
    },
    RecordSchema {
        name: "modulatorRelease",
        doc: "Releasing a modulator at each spike.",
        kind_key: None,
        extends: None,
        keys: &[
            key("modulator", "The modulator.", Value::Reference { to: "modulator" }).required(),
            key("amount", "Amount per spike.", Value::Number).default("1"),
        ],
    },
    RecordSchema {
        name: "synapseSettings",
        doc: "Every setting of a synapse.",
        kind_key: None,
        extends: None,
        keys: &[
            key("weight", "Initial weight: a number, `uniform(low, high)` or `normal(mean, sd)`.", Value::Weight),
            key("equilibrium", "The weight decay relaxes toward (default: the initial weight).", Value::Weight),
            key(
                "sign",
                "`excitatory` or `inhibitory` (default: from the sending neuron's `inhibitory`).",
                Value::Choice { values: &["excitatory", "inhibitory"] },
            ),
            key(
                "delay",
                "Synaptic delay; the connection's delay adds distance / the sender's conduction speed.",
                MS,
            )
            .default("0.5 ms"),
            key("learning", "A learning rule (default: none).", Value::Reference { to: "learning" }),
            key("stickiness", "A stickiness function (default: none).", Value::Reference { to: "stickiness" }),
            key("decay", "Weight decay toward the equilibrium (default: none).", Value::Record { record: "decay" }),
            key("limits", "Weight bounds (default: unbounded).", Value::Record { record: "limits" }),
            key(
                "release",
                "Stochastic transmitter release (default: none; `release=()` for the standard model).",
                Value::Record { record: "release" },
            ),
            key("weight_noise", "Noise added to the weight at each signal.", Value::Number).default("0"),
        ],
    },
    RecordSchema {
        name: "synapseType",
        doc: "A synapse type: settings for connections to use and override.",
        kind_key: None,
        extends: Some(("synapseSettings", &[])),
        keys: &[
            key("name", "Its name, for `type=`.", Value::Name).required(),
            key("extends", "Another synapse type whose settings this one starts from.", Value::Reference { to: "synapseType" }),
        ],
    },
    RecordSchema {
        name: "decay",
        doc: "Exponential weight decay toward the equilibrium weight.",
        kind_key: None,
        extends: None,
        keys: &[key("time", "Decay time constant.", MS).required()],
    },
    RecordSchema {
        name: "limits",
        doc: "Weight bounds.",
        kind_key: None,
        extends: None,
        keys: &[
            key("min", "Lowest weight.", Value::Number).required(),
            key("max", "Highest weight.", Value::Number).required(),
        ],
    },
    RecordSchema {
        name: "release",
        doc: "Stochastic transmitter release: each signal is transmitted with probability 1 − e^(−C·V), where \
              facilitation C and depletion V follow recent use.",
        kind_key: None,
        extends: None,
        keys: &[
            key("facilitation_base", "Facilitation base.", Value::Number).default("1"),
            key("facilitation_step", "Facilitation added per signal.", Value::Number).default("0.1"),
            key("facilitation_time", "Facilitation decay time constant.", MS).default("100 ms"),
            key("depletion_base", "Depletion (available vesicles) base.", Value::Number).default("1"),
            key("depletion_step", "Depletion per signal.", Value::Number).default("0.1"),
            key("depletion_time", "Depletion recovery time constant.", MS).default("100 ms"),
        ],
    },
    RecordSchema {
        name: "connection",
        doc: "Connections from some neurons to others: a pattern, and a synapse type (with overrides).",
        kind_key: None,
        extends: Some(("synapseSettings", &[])),
        keys: &[
            key("from", "Sending neurons: populations, sets, names, id ranges, or a list of them.", Value::Ids).required(),
            key("to", "Receiving neurons.", Value::Ids).required(),
            key(
                "pattern",
                "`all_to_all`; `one_to_one` (the i-th sender to the i-th receiver); `fan_in` (each receiver gets \
                 `count` inputs from different senders, at random); `random` (each pair with `probability`).",
                Value::Choice { values: PATTERNS },
            )
            .default("all_to_all"),
            key("count", "`fan_in`: inputs per receiver.", Value::Number),
            key("probability", "`random`: the probability of each pair.", Value::Number),
            key("within", "Only pairs closer than this.", UM),
            key("self", "A neuron may connect to itself.", Value::Bool).default("false"),
            key("type", "The synapse type.", Value::Reference { to: "synapseType" }),
        ],
    },
    RecordSchema {
        name: "learningRule",
        doc: "A learning rule.",
        kind_key: Some("rule"),
        extends: None,
        keys: &[
            key("name", "Its name, for synapses' `learning`.", Value::Name).required(),
            key(
                "rule",
                "`stdp_hard`: STDP with hard weight bounds; `stdp_soft`: STDP scaled by the distance to the bounds; \
                 `stdp_alpha`: alpha-function STDP.",
                Value::Choice { values: RULES },
            )
            .required(),
            key("depression", "Depression amplitude (post before pre).", Value::Number)
                .only(EXPONENTIAL_RULES)
                .required(),
            key("depression_time", "Depression time constant.", MS).only(EXPONENTIAL_RULES).required(),
            key("potentiation", "Potentiation amplitude (pre before post).", Value::Number)
                .only(EXPONENTIAL_RULES)
                .required(),
            key("potentiation_time", "Potentiation time constant.", MS).only(EXPONENTIAL_RULES).required(),
            key("baseline", "Baseline.", Value::Number).only(&["stdp_alpha"]).required(),
            key("time", "Alpha time constant.", MS).only(&["stdp_alpha"]).required(),
            key("rate", "Learning rate.", Value::Number).only(&["stdp_alpha"]).required(),
        ],
    },
    RecordSchema {
        name: "stickiness",
        doc: "Sigmoid weight stickiness: how firmly learned weights resist decay.",
        kind_key: None,
        extends: None,
        keys: &[
            key("name", "Its name, for synapses' `stickiness`.", Value::Name).required(),
            key("time", "Time constant.", MS).required(),
        ],
    },
    RecordSchema {
        name: "modulator",
        doc: "A neuromodulator: how a release spreads and clears.",
        kind_key: None,
        extends: None,
        keys: &[
            key("name", "Its name, for receptors' and releases' `modulator`.", Value::Name).required(),
            key("clearance_time", "Clearance time constant.", MS).required(),
            key("reach", "How far a release reaches (default: everywhere).", UM),
            key("speed", "Spreading speed (default: instantaneous).", q("speed", "m/s")),
            key(
                "latency",
                "Latency from release to spreading (with several shards, at least the lookahead).",
                MS,
            )
            .default("1 ms"),
            key("falloff", "Falloff length: amplitude × e^(−distance/falloff).", UM),
        ],
    },
];

/// Every key a record accepts: its own, then those of the record it extends (less the excluded ones).
pub fn all_keys(name: &str) -> Vec<&'static Key> {
    let r = record(name);
    let mut keys: Vec<&'static Key> = r.keys.iter().collect();
    if let Some((base, excluded)) = r.extends {
        keys.extend(
            all_keys(base)
                .into_iter()
                .filter(|k| !excluded.contains(&k.key) && !r.keys.iter().any(|own| own.key == k.key)),
        );
    }
    keys
}

/// A record with every key it accepts, for the editor's schema.
#[derive(Debug, Serialize)]
pub struct MergedRecord {
    /// Its id.
    pub name: &'static str,
    /// What it is.
    pub doc: &'static str,
    /// The kind key, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind_key: Option<&'static str>,
    /// Its keys, extended ones included.
    pub keys: Vec<&'static Key>,
}

/// Every record with its keys merged (what `schema.json` holds).
pub fn merged() -> Vec<MergedRecord> {
    RECORDS
        .iter()
        .map(|r| MergedRecord {
            name: r.name,
            doc: r.doc,
            kind_key: r.kind_key,
            keys: all_keys(r.name),
        })
        .collect()
}

/// The schema of a record type.
///
/// # Panics
/// For an unknown record type (a bug).
pub fn record(name: &str) -> &'static RecordSchema {
    RECORDS
        .iter()
        .find(|r| r.name == name)
        .unwrap_or_else(|| panic!("no record type '{name}' in the schema"))
}

/// The keys a record accepts, for a given value of its kind key (or all of them).
pub fn keys(name: &str, kind: Option<&str>) -> Vec<&'static str> {
    all_keys(name)
        .into_iter()
        .filter(|k| k.only.is_empty() || kind.is_none_or(|kind| k.only.contains(&kind)))
        .map(|k| k.key)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_refer_to_records_that_exist() {
        for r in RECORDS {
            for k in r.keys {
                if let Value::Record { record: name } | Value::List { record: name } = k.value {
                    assert!(RECORDS.iter().any(|r| r.name == name), "{}.{} → {name}", r.name, k.key);
                }
            }
            let mut seen = std::collections::HashSet::new();
            assert!(r.keys.iter().all(|k| seen.insert(k.key)), "duplicate key in {}", r.name);
        }
    }

    #[test]
    fn keys_depend_on_the_kind() {
        assert!(keys("neuron", Some("monostable_integrator")).contains(&"threshold"));
        assert!(!keys("neuron", Some("bistable_integrator")).contains(&"threshold"));
        assert!(keys("neuron", None).contains(&"tonic_rate"));
        assert!(keys("neuron", None).contains(&"name"));
        assert!(keys("connection", None).contains(&"weight"));
        assert_eq!(
            keys("learningRule", Some("stdp_alpha")),
            ["name", "rule", "baseline", "time", "rate"]
        );
    }
}
