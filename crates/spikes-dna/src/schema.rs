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
    /// The key whose value selects which other keys apply (`fnc`, `nty`), if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind_key: Option<&'static str>,
    /// Its keys.
    pub keys: &'static [Key],
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
    /// A reference to something defined elsewhere (a learning rule, a modulator…).
    Reference {
        /// What it refers to: `learning`, `stickiness`, `modulator`, `neuron`, `group`.
        to: &'static str,
    },
    /// A set of neuron ids: a name, `{a, b}`, or a range like `n{1:10:1}`.
    Ids,
    /// `t` or `f`.
    Bool,
    /// A weight: a number, or `rLOW:HIGH` for a uniform random draw.
    Weight,
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
    const fn hidden(mut self) -> Self {
        self.hidden = true;
        self
    }
}

const MV: Value = q("potential", "mV");
const MS: Value = q("time", "ms");
const UM: Value = q("length", "µm");
const HZ: Value = q("frequency", "Hz");

/// Every record type.
pub static RECORDS: &[RecordSchema] = &[
    RecordSchema {
        name: "network",
        doc: "A whole network description.",
        kind_key: None,
        keys: &[
            key(
                "GRP",
                "Execution groups: where neurons run.",
                Value::List { record: "group" },
            ),
            key("NRN", "The neurons.", Value::List { record: "neuron" }).required(),
            key(
                "CON",
                "Connections between neurons.",
                Value::List { record: "connection" },
            ),
            key(
                "LRN",
                "Learning rules, referred to by connections' `lrn`.",
                Value::List { record: "learning" },
            ),
            key(
                "CST",
                "Weight-stickiness functions, referred to by connections' `cst`.",
                Value::List { record: "stickiness" },
            ),
            key(
                "MOD",
                "Neuromodulators (dopamine, acetylcholine, …), referred to by receptors and releases.",
                Value::List { record: "modulator" },
            ),
        ],
    },
    RecordSchema {
        name: "group",
        doc: "An execution group: a set of neurons that run together, here or on another machine.",
        kind_key: None,
        keys: &[
            key("gid", "The group's id.", Value::Name).required(),
            key("hst", "Host of a remote group (with `prt`).", Value::Name),
            key("prt", "Port of a remote group (with `hst`).", Value::Number),
        ],
    },
    RecordSchema {
        name: "neuron",
        doc: "A neuron: its dynamics, position, and the settings of its incoming synapses.",
        kind_key: Some("nty"),
        keys: &[
            key("nid", "The neuron's id; connections refer to it.", Value::Name).required(),
            key("grp", "Its execution group.", Value::Reference { to: "group" }).default("default"),
            key(
                "nty",
                "Dynamics: `mi` monostable integrator, `bi` bistable integrator (fires tonically once excited), \
                 `mr` monostable resonator, `br` bistable resonator (limit cycle).",
                Value::Choice {
                    values: &["mi", "bi", "mr", "br"],
                },
            )
            .default("mi"),
            key(
                "inh",
                "Its outgoing connections are inhibitory by default.",
                Value::Bool,
            )
            .default("f"),
            key("mst", "Firing threshold.", MV)
                .default("10 mV")
                .only(&["mi", "mr", "br"]),
            key("lct", "Bistable: the threshold that starts tonic firing.", MV)
                .default("15 mV")
                .only(&["bi"]),
            key("rst", "Bistable: the threshold below which tonic firing stops.", MV)
                .default("10 mV")
                .only(&["bi"]),
            key("tfr", "Bistable: the tonic firing rate.", HZ)
                .default("10 Hz")
                .only(&["bi"]),
            key(
                "rfp",
                "Absolute refractory period after a spike: input is lost and the potential held at `mnp`.",
                MS,
            )
            .default("20 ms"),
            key(
                "rfb",
                "Relative refractoriness after the absolute period: a kernel of this area (mV·ms) lowering the \
                 potential, decaying with time since the spike.",
                q("flux", "µWb"),
            )
            .default("0.08 µWb"),
            key("mnp", "Minimum (resting) membrane potential.", MV).default("0 mV"),
            key("mpd", "Membrane potential decay time constant.", MS).default("250 ms"),
            key(
                "mpr",
                "Rise time: how long a crossing input takes to make the spike.",
                MS,
            )
            .default("2 ms"),
            key("mpn", "Membrane noise: a Wiener process of this magnitude per √ms.", MV).default("0.1 mV"),
            key("wnm", "Noise added to incoming weights at each signal.", Value::Number).default("0"),
            key(
                "spp",
                "Spike amplitude sent along outgoing connections (scaled by each synapse's weight).",
                MV,
            )
            .default("1 mV"),
            key(
                "csp",
                "Axon conduction speed: outgoing delays are `syd + distance / csp`.",
                q("speed", "m/s"),
            )
            .default("10 m/s"),
            key(
                "syd",
                "Synaptic delay of outgoing connections (transmitter release to postsynaptic current); \
                 fast chemical synapses take ~0.2–0.5 ms.",
                MS,
            )
            .default("0.5 ms"),
            key(
                "ipb",
                "Intrinsic plasticity: the threshold shift added at each spike.",
                MV,
            )
            .default("0 mV"),
            key("ipl", "Intrinsic plasticity: the learning amplitude.", MV).default("0 mV"),
            key("ipd", "Intrinsic plasticity: decay time constant between spikes.", MS).default("10 min"),
            key(
                "WDF",
                "Weight decay of incoming synapses toward their equilibrium weight.",
                Value::Record { record: "weightDecay" },
            ),
            key(
                "WLF",
                "Weight limits of incoming synapses.",
                Value::Record { record: "weightLimit" },
            ),
            key(
                "SRP",
                "Stochastic transmitter release of incoming synapses (facilitation and depletion).",
                Value::Record { record: "release" },
            ),
            key("SNT", "Old name for `SRP`.", Value::Record { record: "release" }).hidden(),
            key("LOC", "Soma position.", Value::Record { record: "location" }).required(),
            key("RES", "Resonator parameters.", Value::Record { record: "resonance" }).only(&["mr", "br"]),
            key(
                "HIP",
                "Homeostatic intrinsic plasticity: steer the firing rate toward a target.",
                Value::Record { record: "homeostasis" },
            ),
            key(
                "RCP",
                "Neuromodulator receptors: how modulators change this neuron.",
                Value::List { record: "receptor" },
            ),
            key(
                "REL",
                "A modulator this neuron releases when it spikes.",
                Value::Record {
                    record: "modulatorRelease",
                },
            ),
        ],
    },
    RecordSchema {
        name: "resonance",
        doc: "Resonate-and-fire dynamics: the potential oscillates as it decays. Its kind is the neuron's `nty`.",
        kind_key: None,
        keys: &[
            key("frq", "Resonant frequency.", HZ).required(),
            key("dmp", "Damping time constant of the oscillation.", MS).required(),
            key("lcr", "Bistable resonator: radius of the tonic limit cycle.", MV)
                .only(&["br"])
                .required(),
            key(
                "sep",
                "Bistable resonator: separatrix radius; pulses that leave the state inside it stop tonic firing.",
                MV,
            )
            .only(&["br"])
            .required(),
        ],
    },
    RecordSchema {
        name: "homeostasis",
        doc: "Homeostatic intrinsic plasticity: the threshold drifts so the firing rate approaches a target.",
        kind_key: None,
        keys: &[
            key("tgt", "Target firing rate.", HZ).required(),
            key(
                "str",
                "Strength: mV of threshold shift per Hz of error per second.",
                Value::Number,
            )
            .required(),
            key("win", "Window over which the rate is estimated.", MS).default("1 s"),
            key("lwb", "Lowest threshold shift.", MV),
            key("upb", "Highest threshold shift.", MV),
        ],
    },
    RecordSchema {
        name: "weightDecay",
        doc: "How incoming weights relax toward their equilibrium weight.",
        kind_key: Some("fnc"),
        keys: &[
            key(
                "fnc",
                "`exp`: exponential decay; `zer`: no decay.",
                Value::Choice {
                    values: &["exp", "zer"],
                },
            )
            .required(),
            key("dhl", "Decay time constant.", MS).only(&["exp"]).required(),
        ],
    },
    RecordSchema {
        name: "weightLimit",
        doc: "Bounds on incoming weights.",
        kind_key: Some("fnc"),
        keys: &[
            key(
                "fnc",
                "`bnd`: bounded; `unb`: unbounded.",
                Value::Choice {
                    values: &["bnd", "unb"],
                },
            )
            .required(),
            key("lwb", "Lowest weight.", Value::Number).only(&["bnd"]).required(),
            key("upb", "Highest weight.", Value::Number).only(&["bnd"]).required(),
            key("uwb", "Old name for `upb`.", Value::Number).only(&["bnd"]).hidden(),
        ],
    },
    RecordSchema {
        name: "release",
        doc: "Stochastic transmitter release: each signal is transmitted with probability 1 − e^(−C·V), where \
              facilitation C and depletion V follow recent use.",
        kind_key: None,
        keys: &[
            key("fcb", "Facilitation base.", Value::Number).default("1"),
            key("fcm", "Facilitation added per signal.", Value::Number).default("0.1"),
            key("fct", "Facilitation decay time constant.", MS).default("100 ms"),
            key("dpb", "Depletion (available vesicles) base.", Value::Number).default("1"),
            key("dpm", "Depletion per signal.", Value::Number).default("0.1"),
            key("dpt", "Depletion recovery time constant.", MS).default("100 ms"),
        ],
    },
    RecordSchema {
        name: "location",
        doc: "A position in µm.",
        kind_key: Some("cst"),
        keys: &[
            key(
                "cst",
                "Coordinates: `ct` cartesian (x, y, z), `cl` cylindrical (r, φ, z), `sp` spherical (r, θ, φ).",
                Value::Choice {
                    values: &["ct", "cl", "sp"],
                },
            )
            .default("ct"),
            key("px1", "x, or r.", UM).required(),
            key(
                "px2",
                "y, or φ (cylindrical) / θ (spherical); angles in radians or with `rad`/`deg`.",
                UM,
            )
            .required(),
            key("px3", "z, or φ (spherical).", UM).required(),
        ],
    },
    RecordSchema {
        name: "receptor",
        doc: "A neuromodulator receptor: each effect is linear in the modulator's concentration c at the neuron.",
        kind_key: None,
        keys: &[
            key("mod", "The modulator.", Value::Reference { to: "modulator" }).required(),
            key("exg", "Excitatory gain: excitatory input × (1 + exg·c).", Value::Number).default("0"),
            key("ing", "Inhibitory gain: inhibitory input × (1 + ing·c).", Value::Number).default("0"),
            key("thr", "Threshold change: threshold × (1 + thr·c).", Value::Number).default("0"),
            key("pla", "Plasticity: STDP changes × (1 + pla·c).", Value::Number).default("0"),
            key(
                "elg",
                "Three-factor learning: eligibility trace time constant (with `rat`).",
                MS,
            ),
            key(
                "rat",
                "Three-factor learning: rate (weight change per unit eligibility × concentration × second).",
                Value::Number,
            ),
            key("dir", "Direct potential: c × dir added to the membrane potential.", MV).default("0 mV"),
        ],
    },
    RecordSchema {
        name: "modulatorRelease",
        doc: "Releasing a modulator at each spike.",
        kind_key: None,
        keys: &[
            key("mod", "The modulator.", Value::Reference { to: "modulator" }).required(),
            key("amt", "Amount per spike.", Value::Number).default("1"),
        ],
    },
    RecordSchema {
        name: "connection",
        doc: "Connections from each `prn` neuron to each `psn` neuron.",
        kind_key: None,
        keys: &[
            key("prn", "Sending neurons.", Value::Ids).required(),
            key("psn", "Receiving neurons.", Value::Ids).required(),
            key("cnw", "Initial weight.", Value::Weight).required(),
            key(
                "eqw",
                "Equilibrium weight that decay relaxes toward (default: the initial weight).",
                Value::Weight,
            ),
            key("lrn", "Learning rule.", Value::Reference { to: "learning" }).default("flat"),
            key("cst", "Weight stickiness.", Value::Reference { to: "stickiness" }).default("none"),
            key(
                "typ",
                "Sign: `exc` or `inh` (default: from the sending neuron's `inh`).",
                Value::Choice {
                    values: &["exc", "inh"],
                },
            ),
            key("syd", "Synaptic delay, overriding the sending neuron's.", MS),
        ],
    },
    RecordSchema {
        name: "learning",
        doc: "A learning rule.",
        kind_key: Some("fnc"),
        keys: &[
            key(
                "fnc",
                "`stdp_hard`: STDP with hard weight bounds; `stdp_soft`: STDP scaled by the distance to the bounds; \
                 `stdp_alpha`: alpha-function STDP; `flat`: no learning.",
                Value::Choice {
                    values: &["stdp_hard", "stdp_soft", "stdp_alpha", "flat", "stdp"],
                },
            )
            .required(),
            key(
                "nam",
                "A name for connections' `lrn` (default: the function's name).",
                Value::Name,
            ),
            key("ina", "Depression amplitude (post before pre).", Value::Number)
                .only(&["stdp_hard", "stdp_soft", "stdp"])
                .required(),
            key("inp", "Depression time constant.", MS)
                .only(&["stdp_hard", "stdp_soft", "stdp"])
                .required(),
            key("exa", "Potentiation amplitude (pre before post).", Value::Number)
                .only(&["stdp_hard", "stdp_soft", "stdp"])
                .required(),
            key("exp", "Potentiation time constant.", MS)
                .only(&["stdp_hard", "stdp_soft", "stdp"])
                .required(),
            key("bln", "Baseline.", Value::Number).only(&["stdp_alpha"]).required(),
            key("atc", "Alpha time constant.", MS).only(&["stdp_alpha"]).required(),
            key("alr", "Learning rate.", Value::Number)
                .only(&["stdp_alpha"])
                .required(),
        ],
    },
    RecordSchema {
        name: "stickiness",
        doc: "Weight stickiness: how firmly learned weights resist decay.",
        kind_key: Some("fnc"),
        keys: &[
            key(
                "fnc",
                "`sig`: sigmoid stickiness; `none`.",
                Value::Choice {
                    values: &["sig", "none"],
                },
            )
            .required(),
            key(
                "nam",
                "A name for connections' `cst` (default: the function's name).",
                Value::Name,
            ),
            key("dhl", "Time constant.", MS).only(&["sig"]).required(),
        ],
    },
    RecordSchema {
        name: "modulator",
        doc: "A neuromodulator: how a release spreads and clears.",
        kind_key: None,
        keys: &[
            key("nam", "Its name, for receptors' and releases' `mod`.", Value::Name).required(),
            key("clr", "Clearance time constant.", MS).required(),
            key("rad", "Reach.", UM),
            key("spd", "Spreading speed (default: instantaneous).", q("speed", "m/s")),
            key(
                "lat",
                "Latency from release to spreading (with several shards, at least the lookahead).",
                MS,
            )
            .default("1 ms"),
            key("len", "Falloff length: amplitude × e^(−distance/len).", UM),
        ],
    },
];

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
    record(name)
        .keys
        .iter()
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
        assert_eq!(keys("weightDecay", Some("zer")), ["fnc"]);
        assert_eq!(keys("weightDecay", Some("exp")), ["fnc", "dhl"]);
        assert!(keys("neuron", Some("mi")).contains(&"mst"));
        assert!(!keys("neuron", Some("bi")).contains(&"mst"));
        assert!(keys("neuron", None).contains(&"lct"));
    }
}
