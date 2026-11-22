//! From the syntax tree to a [`NetworkDescription`]: keys, units, defaults, expansion and reference checks.
//! Every problem found is reported, not just the first.

use std::collections::{BTreeMap, HashMap, HashSet};

use spikes_model::{
    AlphaStdp, ExponentialStdp, Homeostasis, IntrinsicParams, LearningRule, NeuronKind, NeuronParams, Release,
    Resonance, Sign, Stickiness, WeightDecay, WeightLimit,
};

use crate::description::{
    ConnectionDescription, GroupDescription, NetworkDescription, NeuronDescription, SynapseDefaults, WeightSpec,
};
use crate::syntax::{Field, Node, ParseError, Span, Value};
use crate::units::Quantity;

/// Collects errors while converting.
#[derive(Default)]
struct Errors(Vec<ParseError>);

impl Errors {
    fn add(&mut self, span: Span, message: impl Into<String>) {
        self.0.push(ParseError {
            span,
            message: message.into(),
        });
    }
}

/// A record (`(k=v, …)`) being read.
struct Rec<'a> {
    fields: &'a [Field],
    span: Span,
    /// What it is, for messages: "neuron", "connection", …
    what: &'static str,
}

impl<'a> Rec<'a> {
    fn of(node: &'a Node, what: &'static str, errors: &mut Errors) -> Option<Self> {
        match &node.value {
            Value::Record(fields) => Some(Self {
                fields,
                span: node.span,
                what,
            }),
            _ => {
                errors.add(node.span, format!("expected a {what} in parentheses: (key=value, …)"));
                None
            }
        }
    }

    /// Reports keys that aren't allowed here, and keys given twice.
    fn check_keys(&self, allowed: &[&str], errors: &mut Errors) {
        let mut seen = HashSet::new();
        for field in self.fields {
            if !allowed.contains(&field.key.as_str()) {
                errors.add(
                    field.span,
                    format!(
                        "unknown key '{}' in a {}; expected one of: {}",
                        field.key,
                        self.what,
                        allowed.join(", ")
                    ),
                );
            } else if !seen.insert(field.key.as_str()) {
                errors.add(field.span, format!("'{}' is given twice", field.key));
            }
        }
    }

    fn get(&self, key: &str) -> Option<&'a Node> {
        self.fields.iter().find(|f| f.key == key).map(|f| &f.value)
    }

    fn missing(&self, key: &str, errors: &mut Errors) {
        errors.add(self.span, format!("this {} needs '{key}'", self.what));
    }

    /// A number in `quantity`'s base unit; `default` if absent (or an error if there's no default).
    fn quantity(&self, key: &str, quantity: Quantity, default: Option<f64>, errors: &mut Errors) -> f64 {
        let Some(node) = self.get(key) else {
            if default.is_none() {
                self.missing(key, errors);
            }
            return default.unwrap_or(0.0);
        };
        match &node.value {
            Value::Number { value, unit: None } => *value,
            Value::Number {
                value,
                unit: Some(unit),
            } => match quantity.factor(unit) {
                Some(factor) if quantity != Quantity::Dimensionless => value * factor,
                _ => {
                    errors.add(
                        node.span,
                        if quantity == Quantity::Dimensionless {
                            format!("'{key}' takes a plain number, without a unit (got '{unit}')")
                        } else {
                            format!(
                                "'{unit}' is not a unit for '{key}' (base unit {})",
                                quantity.base_unit()
                            )
                        },
                    );
                    0.0
                }
            },
            _ => {
                errors.add(node.span, format!("'{key}' must be a number"));
                0.0
            }
        }
    }

    /// A single word; `default` if absent. Checked against `allowed` when it isn't empty.
    fn word(&self, key: &str, default: Option<&str>, allowed: &[&str], errors: &mut Errors) -> String {
        let Some(node) = self.get(key) else {
            if default.is_none() {
                self.missing(key, errors);
            }
            return default.unwrap_or_default().to_string();
        };
        match &node.value {
            Value::Ids(words) if words.len() == 1 => {
                let word = &words[0];
                if !allowed.is_empty() && !allowed.contains(&word.as_str()) {
                    errors.add(
                        node.span,
                        format!("'{key}' must be one of: {} (got '{word}')", allowed.join(", ")),
                    );
                }
                word.clone()
            }
            _ => {
                errors.add(node.span, format!("'{key}' must be a single name"));
                String::new()
            }
        }
    }

    fn boolean(&self, key: &str, default: bool, errors: &mut Errors) -> bool {
        match self
            .word(key, Some(if default { "t" } else { "f" }), &[], errors)
            .as_str()
        {
            "t" | "true" => true,
            "f" | "false" => false,
            other => {
                errors.add(
                    self.get(key).map_or(self.span, |n| n.span),
                    format!("'{key}' must be t or f, got '{other}'"),
                );
                default
            }
        }
    }

    /// The names an id expression expands to.
    fn ids(&self, key: &str, errors: &mut Errors) -> Vec<String> {
        match self.get(key).map(|n| (&n.value, n.span)) {
            Some((Value::Ids(ids), _)) => ids.clone(),
            Some((_, span)) => {
                errors.add(
                    span,
                    format!("'{key}' must be a neuron ID or an ID set such as in-{{1:4:1}}"),
                );
                Vec::new()
            }
            None => {
                self.missing(key, errors);
                Vec::new()
            }
        }
    }

    fn weight(&self, key: &str, errors: &mut Errors) -> Option<WeightSpec> {
        let node = self.get(key)?;
        match &node.value {
            Value::Number { value, unit: None } => Some(WeightSpec::Fixed(*value)),
            Value::Random { low, high } => Some(WeightSpec::Random { low: *low, high: *high }),
            _ => {
                errors.add(
                    node.span,
                    format!("'{key}' must be a number or a random range such as r0:1"),
                );
                None
            }
        }
    }

    fn record(&self, key: &str, what: &'static str, errors: &mut Errors) -> Option<Rec<'a>> {
        self.get(key).and_then(|node| Rec::of(node, what, errors))
    }
}

/// The items of a top-level list such as `NRN=[…]`.
fn list<'a>(top: &Rec<'a>, key: &str, errors: &mut Errors) -> &'a [Node] {
    match top.get(key).map(|n| (&n.value, n.span)) {
        Some((Value::List(items), _)) => items,
        Some((_, span)) => {
            errors.add(span, format!("'{key}' must be a list: {key}=[…]"));
            &[]
        }
        None => &[],
    }
}

pub(crate) fn convert(root: &Node) -> Result<NetworkDescription, Vec<ParseError>> {
    let mut errors = Errors::default();
    let Some(top) = Rec::of(root, "network description", &mut errors) else {
        return Err(errors.0);
    };
    top.check_keys(&["GRP", "NRN", "CON", "LRN", "CST"], &mut errors);
    if top.get("NRN").is_none() {
        top.missing("NRN", &mut errors);
    }

    let groups: Vec<GroupDescription> = list(&top, "GRP", &mut errors)
        .iter()
        .filter_map(|node| group(node, &mut errors))
        .collect();
    let learning = named(
        list(&top, "LRN", &mut errors),
        "learning function",
        learning_rule,
        &mut errors,
    );
    let stickiness = named(
        list(&top, "CST", &mut errors),
        "stickiness function",
        stickiness_fn,
        &mut errors,
    );
    let neurons: Vec<NeuronDescription> = list(&top, "NRN", &mut errors)
        .iter()
        .filter_map(|node| neuron(node, &mut errors))
        .collect();

    let mut learning_map: BTreeMap<String, LearningRule> = BTreeMap::from([("flat".to_string(), LearningRule::None)]);
    learning_map.extend(learning);
    let mut stickiness_map = BTreeMap::from([("none".to_string(), Stickiness::None)]);
    stickiness_map.extend(stickiness);

    // References: groups, duplicate neurons.
    let group_ids: HashSet<&str> = groups.iter().map(|g| g.id.as_str()).collect();
    let mut seen = HashMap::new();
    for n in &neurons {
        if let Some(first) = seen.insert(n.id.as_str(), n.span) {
            errors.add(n.span, format!("neuron '{}' is already defined at {first}", n.id));
        }
        if !groups.is_empty() && !group_ids.contains(n.group.as_str()) {
            errors.add(
                n.span,
                format!(
                    "neuron '{}' is in group '{}', which isn't defined in GRP",
                    n.id, n.group
                ),
            );
        }
    }
    let inhibitory: HashMap<&str, bool> = neurons.iter().map(|n| (n.id.as_str(), n.inhibitory)).collect();

    let mut connections = Vec::new();
    for node in list(&top, "CON", &mut errors) {
        connection(
            node,
            &inhibitory,
            &learning_map,
            &stickiness_map,
            &mut connections,
            &mut errors,
        );
    }

    // Without a GRP section, every neuron runs locally in the groups it names.
    let groups = if groups.is_empty() {
        let mut names: Vec<&str> = neurons.iter().map(|n| n.group.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        names
            .into_iter()
            .map(|id| GroupDescription {
                id: id.to_string(),
                remote: None,
            })
            .collect()
    } else {
        groups
    };

    if errors.0.is_empty() {
        Ok(NetworkDescription {
            groups,
            neurons,
            connections,
            learning: learning_map,
            stickiness: stickiness_map,
        })
    } else {
        errors.0.sort_by_key(|e| e.span);
        Err(errors.0)
    }
}

/// Converts a list of named definitions, reporting duplicate names.
fn named<T>(
    nodes: &[Node],
    what: &'static str,
    one: fn(&Rec<'_>, &mut Errors) -> Option<(String, T)>,
    errors: &mut Errors,
) -> Vec<(String, T)> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for node in nodes {
        let Some(rec) = Rec::of(node, what, errors) else {
            continue;
        };
        if let Some((name, value)) = one(&rec, errors) {
            if !seen.insert(name.clone()) {
                errors.add(
                    rec.span,
                    format!("{what} '{name}' is defined twice (use nam= to give one another name)"),
                );
            }
            out.push((name, value));
        }
    }
    out
}

fn group(node: &Node, errors: &mut Errors) -> Option<GroupDescription> {
    let rec = Rec::of(node, "group", errors)?;
    rec.check_keys(&["gid", "hst", "prt"], errors);
    let id = rec.word("gid", None, &[], errors);
    let remote = match (rec.get("hst"), rec.get("prt")) {
        (None, None) => None,
        (Some(_), Some(_)) => {
            let host = rec.word("hst", None, &[], errors);
            let port = rec.quantity("prt", Quantity::Dimensionless, None, errors);
            if !(0.0..=65_535.0).contains(&port) || port.fract() != 0.0 {
                errors.add(rec.span, format!("port {port} is not a valid port number"));
            }
            Some((host, port as u16))
        }
        _ => {
            errors.add(
                rec.span,
                "a remote group needs both 'hst' and 'prt' (a local group needs neither)",
            );
            None
        }
    };
    Some(GroupDescription { id, remote })
}

fn learning_rule(rec: &Rec<'_>, errors: &mut Errors) -> Option<(String, LearningRule)> {
    let function = rec.word(
        "fnc",
        None,
        &["stdp_hard", "stdp_soft", "stdp_alpha", "stdp", "flat"],
        errors,
    );
    let exponential = |errors: &mut Errors| ExponentialStdp {
        inhibition_amplitude: rec.quantity("ina", Quantity::Dimensionless, None, errors),
        inhibition_period_ms: rec.quantity("inp", Quantity::Time, None, errors),
        excitation_amplitude: rec.quantity("exa", Quantity::Dimensionless, None, errors),
        excitation_period_ms: rec.quantity("exp", Quantity::Time, None, errors),
    };
    let rule = match function.as_str() {
        // "stdp" is the 2017 name for what later became stdp_hard.
        "stdp_hard" | "stdp" => {
            rec.check_keys(&["fnc", "nam", "ina", "inp", "exa", "exp"], errors);
            LearningRule::HardLimit(exponential(errors))
        }
        "stdp_soft" => {
            rec.check_keys(&["fnc", "nam", "ina", "inp", "exa", "exp"], errors);
            LearningRule::SoftLimit(exponential(errors))
        }
        "stdp_alpha" => {
            rec.check_keys(&["fnc", "nam", "bln", "atc", "alr"], errors);
            LearningRule::Alpha(AlphaStdp::new(
                rec.quantity("bln", Quantity::Dimensionless, None, errors),
                rec.quantity("atc", Quantity::Time, None, errors),
                rec.quantity("alr", Quantity::Dimensionless, None, errors),
            ))
        }
        "flat" => {
            rec.check_keys(&["fnc", "nam"], errors);
            LearningRule::None
        }
        _ => return None,
    };
    let name = rec.word("nam", Some(&function), &[], errors);
    Some((name, rule))
}

fn stickiness_fn(rec: &Rec<'_>, errors: &mut Errors) -> Option<(String, Stickiness)> {
    let function = rec.word("fnc", None, &["sig", "none"], errors);
    let value = match function.as_str() {
        "sig" => {
            rec.check_keys(&["fnc", "nam", "dhl"], errors);
            Stickiness::Sigmoid {
                tau_ms: rec.quantity("dhl", Quantity::Time, None, errors),
            }
        }
        "none" => {
            rec.check_keys(&["fnc", "nam"], errors);
            Stickiness::None
        }
        _ => return None,
    };
    let name = rec.word("nam", Some(&function), &[], errors);
    Some((name, value))
}

const NEURON_KEYS: &[&str] = &[
    "nid", "grp", "nty", "inh", "mst", "lct", "rst", "tfr", "rfp", "rfb", "mnp", "mpd", "mpr", "mpn", "wnm", "spp",
    "csp", "ipb", "ipl", "ipd", "WDF", "WLF", "SRP", "SNT", "LOC", "RES", "HIP",
];

/// v1's defaults, used for keys a neuron doesn't give (older files lack several).
mod default {
    pub const THRESHOLD_MV: f64 = 10.0;
    pub const BISTABLE_THRESHOLD_MV: f64 = 15.0;
    pub const RESTING_THRESHOLD_MV: f64 = 10.0;
    pub const TONIC_RATE_HZ: f64 = 10.0;
    pub const REFRACTORY_MS: f64 = 20.0;
    pub const BASE_REFRACTORINESS: f64 = 0.08; // 8e-8 Wb
    pub const DECAY_MS: f64 = 250.0;
    pub const RISE_MS: f64 = 2.0;
    pub const NOISE_MV: f64 = 0.1;
    pub const SPIKE_MV: f64 = 1.0;
    pub const CONDUCTION_M_PER_S: f64 = 10.0;
    pub const IP_DECAY_MS: f64 = 600_000.0; // 10 min
    pub const WEIGHT_DECAY_MS: f64 = 1_500.0;
}

fn neuron(node: &Node, errors: &mut Errors) -> Option<NeuronDescription> {
    let rec = Rec::of(node, "neuron", errors)?;
    rec.check_keys(NEURON_KEYS, errors);
    let q = |key, quantity, default, errors: &mut Errors| rec.quantity(key, quantity, Some(default), errors);

    let id = rec.word("nid", None, &[], errors);
    let group = rec.word("grp", Some("default"), &[], errors);
    let kind_name = rec.word("nty", Some("mi"), &["mi", "bi", "mr", "br"], errors);
    let inhibitory = rec.boolean("inh", false, errors);

    let mut threshold_mv = q("mst", Quantity::Potential, default::THRESHOLD_MV, errors);
    let resonance = |errors: &mut Errors| -> Option<(Resonance, Rec<'_>)> {
        let Some(res) = rec.record("RES", "resonance", errors) else {
            rec.missing("RES", errors);
            return None;
        };
        let r = Resonance {
            frequency_hz: res.quantity("frq", Quantity::Frequency, None, errors),
            damping_ms: res.quantity("dmp", Quantity::Time, None, errors),
        };
        if r.frequency_hz <= 0.0 {
            errors.add(res.span, "the resonant frequency 'frq' must be positive");
        }
        Some((r, res))
    };
    let kind = match kind_name.as_str() {
        "bi" => {
            threshold_mv = rec.quantity("lct", Quantity::Potential, Some(default::BISTABLE_THRESHOLD_MV), errors);
            NeuronKind::BistableIntegrator {
                resting_threshold_mv: q("rst", Quantity::Potential, default::RESTING_THRESHOLD_MV, errors),
                tonic_rate_hz: q("tfr", Quantity::Frequency, default::TONIC_RATE_HZ, errors),
            }
        }
        "mr" => {
            let (r, res) = resonance(errors)?;
            res.check_keys(&["frq", "dmp"], errors);
            NeuronKind::MonostableResonator(r)
        }
        "br" => {
            let (r, res) = resonance(errors)?;
            res.check_keys(&["frq", "dmp", "lcr", "sep"], errors);
            let cycle_mv = res.quantity("lcr", Quantity::Potential, None, errors);
            let separatrix_mv = res.quantity("sep", Quantity::Potential, None, errors);
            if separatrix_mv >= cycle_mv {
                errors.add(
                    res.span,
                    "the separatrix radius 'sep' must be smaller than the limit cycle 'lcr'",
                );
            }
            NeuronKind::BistableResonator {
                resonance: r,
                cycle_mv,
                separatrix_mv,
            }
        }
        _ => NeuronKind::MonostableIntegrator,
    };

    let homeostasis = rec.record("HIP", "homeostasis", errors).map(|h| {
        h.check_keys(&["tgt", "str", "win", "lwb", "upb"], errors);
        Homeostasis {
            target_hz: h.quantity("tgt", Quantity::Frequency, None, errors),
            strength: h.quantity("str", Quantity::Dimensionless, None, errors),
            window_ms: h.quantity("win", Quantity::Time, Some(1_000.0), errors),
            min_mv: h.quantity("lwb", Quantity::Potential, Some(f64::NEG_INFINITY), errors),
            max_mv: h.quantity("upb", Quantity::Potential, Some(f64::INFINITY), errors),
        }
    });

    let params = NeuronParams {
        kind,
        threshold_mv,
        refractory_ms: q("rfp", Quantity::Time, default::REFRACTORY_MS, errors),
        base_refractoriness: q("rfb", Quantity::Flux, default::BASE_REFRACTORINESS, errors),
        min_potential_mv: q("mnp", Quantity::Potential, 0.0, errors),
        decay_ms: q("mpd", Quantity::Time, default::DECAY_MS, errors),
        rise_ms: q("mpr", Quantity::Time, default::RISE_MS, errors),
        noise_mv: q("mpn", Quantity::Potential, default::NOISE_MV, errors),
        spike_mv: q("spp", Quantity::Potential, default::SPIKE_MV, errors),
        intrinsic: IntrinsicParams {
            base_mv: q("ipb", Quantity::Potential, 0.0, errors),
            learning_mv: q("ipl", Quantity::Potential, 0.0, errors),
            decay_ms: q("ipd", Quantity::Time, default::IP_DECAY_MS, errors),
            homeostasis,
        },
    };

    let decay = match rec.record("WDF", "weight decay", errors) {
        None => WeightDecay::Exponential {
            tau_ms: default::WEIGHT_DECAY_MS,
        },
        Some(w) => match w.word("fnc", None, &["exp", "zer"], errors).as_str() {
            "exp" => {
                w.check_keys(&["fnc", "dhl"], errors);
                WeightDecay::Exponential {
                    tau_ms: w.quantity("dhl", Quantity::Time, None, errors),
                }
            }
            _ => {
                w.check_keys(&["fnc"], errors);
                WeightDecay::None
            }
        },
    };
    let limit = match rec.record("WLF", "weight limit", errors) {
        None => WeightLimit::Unbounded,
        Some(w) => match w.word("fnc", None, &["bnd", "unb"], errors).as_str() {
            "bnd" => {
                w.check_keys(&["fnc", "lwb", "upb", "uwb"], errors);
                let upper_key = if w.get("uwb").is_some() { "uwb" } else { "upb" };
                let lower = w.quantity("lwb", Quantity::Dimensionless, None, errors);
                let upper = w.quantity(upper_key, Quantity::Dimensionless, None, errors);
                if lower > upper {
                    errors.add(
                        w.span,
                        format!("the lower bound {lower} is above the upper bound {upper}"),
                    );
                }
                WeightLimit::Bounded { lower, upper }
            }
            _ => {
                w.check_keys(&["fnc"], errors);
                WeightLimit::Unbounded
            }
        },
    };
    // `SNT` is an older name for `SRP`.
    let release_rec = rec
        .record("SRP", "release probability", errors)
        .or_else(|| rec.record("SNT", "release probability", errors));
    let release = Some(match release_rec {
        None => Release::V1_DEFAULT,
        Some(r) => {
            r.check_keys(&["fcb", "fcm", "fct", "dpb", "dpm", "dpt"], errors);
            let d = Release::V1_DEFAULT;
            Release {
                facilitation_base: r.quantity("fcb", Quantity::Dimensionless, Some(d.facilitation_base), errors),
                facilitation_magnitude: r.quantity(
                    "fcm",
                    Quantity::Dimensionless,
                    Some(d.facilitation_magnitude),
                    errors,
                ),
                facilitation_tau_ms: r.quantity("fct", Quantity::Time, Some(d.facilitation_tau_ms), errors),
                depletion_base: r.quantity("dpb", Quantity::Dimensionless, Some(d.depletion_base), errors),
                depletion_magnitude: r.quantity("dpm", Quantity::Dimensionless, Some(d.depletion_magnitude), errors),
                depletion_tau_ms: r.quantity("dpt", Quantity::Time, Some(d.depletion_tau_ms), errors),
            }
        }
    });

    let location_um = match rec.record("LOC", "location", errors) {
        Some(loc) => location(&loc, errors),
        None => {
            rec.missing("LOC", errors);
            [0.0; 3]
        }
    };

    Some(NeuronDescription {
        id,
        group,
        inhibitory,
        params,
        conduction_m_per_s: q("csp", Quantity::Speed, default::CONDUCTION_M_PER_S, errors),
        location_um,
        synapses: SynapseDefaults {
            decay,
            limit,
            release,
            weight_noise: q("wnm", Quantity::Dimensionless, 0.0, errors),
        },
        span: rec.span,
    })
}

/// A location, converted to cartesian µm. Cylindrical is (r, φ, z); spherical is (r, φ, θ) with azimuth φ and
/// polar angle θ, as in v1.
fn location(loc: &Rec<'_>, errors: &mut Errors) -> [f64; 3] {
    loc.check_keys(&["cst", "px1", "px2", "px3"], errors);
    let system = loc.word("cst", Some("ct"), &["ct", "cl", "sp"], errors);
    let length = |key, errors: &mut Errors| loc.quantity(key, Quantity::Length, None, errors);
    let angle = |key, errors: &mut Errors| loc.quantity(key, Quantity::Angle, None, errors);
    match system.as_str() {
        "cl" => {
            let (r, phi, z) = (length("px1", errors), angle("px2", errors), length("px3", errors));
            [r * phi.cos(), r * phi.sin(), z]
        }
        "sp" => {
            let (r, phi, theta) = (length("px1", errors), angle("px2", errors), angle("px3", errors));
            [
                r * phi.cos() * theta.sin(),
                r * phi.sin() * theta.sin(),
                r * theta.cos(),
            ]
        }
        _ => [length("px1", errors), length("px2", errors), length("px3", errors)],
    }
}

fn connection(
    node: &Node,
    inhibitory: &HashMap<&str, bool>,
    learning: &BTreeMap<String, LearningRule>,
    stickiness: &BTreeMap<String, Stickiness>,
    out: &mut Vec<ConnectionDescription>,
    errors: &mut Errors,
) {
    let Some(rec) = Rec::of(node, "connection", errors) else {
        return;
    };
    rec.check_keys(&["prn", "psn", "cnw", "eqw", "lrn", "cst", "typ"], errors);
    let pres = rec.ids("prn", errors);
    let posts = rec.ids("psn", errors);
    let weight = rec.weight("cnw", errors);
    if weight.is_none() && rec.get("cnw").is_none() {
        rec.missing("cnw", errors);
    }
    let weight = weight.unwrap_or(WeightSpec::Fixed(0.0));
    let equilibrium = rec.weight("eqw", errors).unwrap_or(weight);
    let learning_name = rec.word("lrn", Some("flat"), &[], errors);
    if !learning.contains_key(&learning_name) {
        let known: Vec<&str> = learning.keys().map(String::as_str).collect();
        errors.add(
            rec.get("lrn").map_or(rec.span, |n| n.span),
            format!(
                "learning function '{learning_name}' isn't defined in LRN (defined: {})",
                known.join(", ")
            ),
        );
    }
    let stickiness_name = rec.word("cst", Some("none"), &[], errors);
    if !stickiness.contains_key(&stickiness_name) {
        errors.add(
            rec.get("cst").map_or(rec.span, |n| n.span),
            format!("stickiness function '{stickiness_name}' isn't defined in CST"),
        );
    }
    let sign = rec.get("typ").map(|_| rec.word("typ", None, &["exc", "inh"], errors));

    // One error per connection, however many names its id sets expand to.
    let mut missing: Vec<&str> = pres
        .iter()
        .chain(&posts)
        .map(String::as_str)
        .filter(|name| !inhibitory.contains_key(name))
        .collect();
    missing.dedup();
    match missing.as_slice() {
        [] => {}
        [name] => errors.add(
            rec.span,
            format!("connection refers to neuron '{name}', which isn't defined in NRN"),
        ),
        names => errors.add(
            rec.span,
            format!(
                "connection refers to {} neurons that aren't defined in NRN: {}",
                names.len(),
                names.join(", ")
            ),
        ),
    }
    for pre in &pres {
        for post in &posts {
            let sign = match sign.as_deref() {
                Some("inh") => Sign::Inhibitory,
                Some(_) => Sign::Excitatory,
                None if inhibitory.get(pre.as_str()).copied().unwrap_or(false) => Sign::Inhibitory,
                None => Sign::Excitatory,
            };
            out.push(ConnectionDescription {
                pre: pre.clone(),
                post: post.clone(),
                weight,
                equilibrium,
                learning: learning_name.clone(),
                stickiness: stickiness_name.clone(),
                sign,
                span: rec.span,
            });
        }
    }
}
