//! From the syntax tree to a [`NetworkDescription`]: keys, units, defaults, types and overrides, placements, and
//! reference checks. Every problem found is reported, not just the first.

use std::collections::{BTreeMap, HashMap, HashSet};

use spikes_model::{
    AlphaStdp, ExponentialStdp, Homeostasis, IntrinsicParams, LearningRule, NeuronKind, NeuronParams, Receptor,
    Release, Resonance, Sign, Stickiness, ThreeFactor, WeightDecay, WeightLimit,
};

use crate::Neurons;
use crate::description::{
    ConnectionDescription, DEFAULT_RELEASE, ModulatorDescription, NetworkDescription, NeuronDescription, Pattern,
    PopulationDescription, SetDescription, SynapseSpec, WeightSpec,
};
use crate::schema;
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

/// A record (`(k=v, …)`) being read: its own fields, or a type's merged with an instance's.
#[derive(Clone)]
struct Rec<'a> {
    fields: Vec<&'a Field>,
    span: Span,
    /// What it is, for messages: "neuron", "connection", …
    what: &'static str,
}

impl<'a> Rec<'a> {
    fn of(node: &'a Node, what: &'static str, errors: &mut Errors) -> Option<Self> {
        match &node.value {
            Value::Record(fields) => Some(Self {
                fields: fields.iter().collect(),
                span: node.span,
                what,
            }),
            _ => {
                errors.add(node.span, format!("expected a {what} in parentheses: (key=value, …)"));
                None
            }
        }
    }

    fn empty(&self) -> Rec<'a> {
        Rec {
            fields: Vec::new(),
            span: self.span,
            what: self.what,
        }
    }

    /// Reports keys that aren't allowed here, and keys given twice.
    fn check_keys(&self, allowed: &[&str], errors: &mut Errors) {
        let mut seen = HashSet::new();
        for field in &self.fields {
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

    /// This record's fields over `base`'s: a key given here replaces the base's (a nested record whole). Keys in
    /// `own_only` (the record's own, like `name`) aren't settings: they're neither taken from `base` nor kept.
    fn over(&self, base: &Rec<'a>, own_only: &[&str]) -> Rec<'a> {
        let mut fields: Vec<&'a Field> = base
            .fields
            .iter()
            .copied()
            .filter(|f| !own_only.contains(&f.key.as_str()) && !self.fields.iter().any(|g| g.key == f.key))
            .collect();
        fields.extend(
            self.fields
                .iter()
                .copied()
                .filter(|f| !own_only.contains(&f.key.as_str())),
        );
        Rec {
            fields,
            span: self.span,
            what: self.what,
        }
    }

    fn get(&self, key: &str) -> Option<&'a Node> {
        self.fields.iter().find(|f| f.key == key).map(|f| &f.value)
    }

    fn span_of(&self, key: &str) -> Span {
        self.get(key).map_or(self.span, |n| n.span)
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
        number(node, key, quantity, errors)
    }

    /// A number if the key is there.
    fn optional(&self, key: &str, quantity: Quantity, errors: &mut Errors) -> Option<f64> {
        self.get(key).map(|node| number(node, key, quantity, errors))
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

    fn optional_word(&self, key: &str, allowed: &[&str], errors: &mut Errors) -> Option<String> {
        self.get(key).map(|_| self.word(key, None, allowed, errors))
    }

    fn boolean(&self, key: &str, default: bool, errors: &mut Errors) -> bool {
        if self.get(key).is_none() {
            return default;
        }
        match self.word(key, None, &[], errors).as_str() {
            "true" => true,
            "false" => false,
            other => {
                errors.add(
                    self.span_of(key),
                    format!("'{key}' must be true or false, got '{other}'"),
                );
                default
            }
        }
    }

    fn weight(&self, key: &str, errors: &mut Errors) -> Option<WeightSpec> {
        let node = self.get(key)?;
        match &node.value {
            Value::Number { value, unit: None } => Some(WeightSpec::Fixed(*value)),
            Value::Call { name, args } if name == "uniform" || name == "normal" => {
                let numbers: Vec<f64> = args
                    .iter()
                    .map(|a| number(a, key, Quantity::Dimensionless, errors))
                    .collect();
                let [a, b] = numbers[..] else {
                    errors.add(node.span, format!("{name}(…) takes two numbers"));
                    return None;
                };
                if name == "uniform" {
                    if a > b {
                        errors.add(
                            node.span,
                            format!("uniform({a}, {b}): the low end is above the high end"),
                        );
                    }
                    Some(WeightSpec::Uniform { low: a, high: b })
                } else {
                    if b < 0.0 {
                        errors.add(node.span, "normal(mean, sd): the standard deviation can't be negative");
                    }
                    Some(WeightSpec::Normal { mean: a, sd: b })
                }
            }
            Value::Random { low, high } => {
                errors.add(
                    node.span,
                    format!("write uniform({low}, {high}) instead of r{low}:{high}"),
                );
                None
            }
            _ => {
                errors.add(
                    node.span,
                    format!("'{key}' must be a number, uniform(low, high) or normal(mean, sd)"),
                );
                None
            }
        }
    }

    fn record(&self, key: &str, what: &'static str, errors: &mut Errors) -> Option<Rec<'a>> {
        self.get(key).and_then(|node| Rec::of(node, what, errors))
    }

    /// A point (µm, cartesian).
    fn point(&self, key: &str, errors: &mut Errors) -> Option<[f64; 3]> {
        self.get(key).map(|node| point(node, errors))
    }

    /// Three plain numbers `[a, b, c]` (counts, or sizes in µm).
    fn triple(&self, key: &str, quantity: Quantity, errors: &mut Errors) -> Option<[f64; 3]> {
        let node = self.get(key)?;
        match &node.value {
            Value::List(items) if items.len() == 3 => {
                Some(std::array::from_fn(|i| number(&items[i], key, quantity, errors)))
            }
            _ => {
                errors.add(node.span, format!("'{key}' must be three numbers: [a, b, c]"));
                None
            }
        }
    }
}

fn number(node: &Node, key: &str, quantity: Quantity, errors: &mut Errors) -> f64 {
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

/// A point: `[x, y, z]`, `(x=…, y=…, z=…)`, `(r=…, phi=…, z=…)` (cylindrical) or `(r=…, theta=…, phi=…)`
/// (spherical: polar angle θ from +z, azimuth φ), in µm, cartesian.
fn point(node: &Node, errors: &mut Errors) -> [f64; 3] {
    match &node.value {
        Value::List(items) if items.len() == 3 => {
            std::array::from_fn(|i| number(&items[i], "point", Quantity::Length, errors))
        }
        Value::Record(fields) => {
            let rec = Rec {
                fields: fields.iter().collect(),
                span: node.span,
                what: "point",
            };
            let length = |k, errors: &mut Errors| rec.quantity(k, Quantity::Length, None, errors);
            let angle = |k, errors: &mut Errors| rec.quantity(k, Quantity::Angle, None, errors);
            if rec.get("theta").is_some() {
                rec.check_keys(&["r", "theta", "phi"], errors);
                let (r, theta, phi) = (length("r", errors), angle("theta", errors), angle("phi", errors));
                [
                    r * phi.cos() * theta.sin(),
                    r * phi.sin() * theta.sin(),
                    r * theta.cos(),
                ]
            } else if rec.get("r").is_some() {
                rec.check_keys(&["r", "phi", "z"], errors);
                let (r, phi, z) = (length("r", errors), angle("phi", errors), length("z", errors));
                [r * phi.cos(), r * phi.sin(), z]
            } else {
                rec.check_keys(&["x", "y", "z"], errors);
                [length("x", errors), length("y", errors), length("z", errors)]
            }
        }
        _ => {
            errors.add(
                node.span,
                "a point is [x, y, z], (r=…, phi=…, z=…) or (r=…, theta=…, phi=…)",
            );
            [0.0; 3]
        }
    }
}

/// The items of a top-level list such as `neurons=[…]`.
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

/// Types (neuron or synapse) by name, with `extends` chains merged: each type's own fields over its base's.
fn types<'a>(
    nodes: &'a [Node],
    record: &'static str,
    what: &'static str,
    errors: &mut Errors,
) -> HashMap<String, Rec<'a>> {
    let mut raw: Vec<(String, Rec<'a>)> = Vec::new();
    for node in nodes {
        let Some(rec) = Rec::of(node, what, errors) else {
            continue;
        };
        rec.check_keys(&schema::keys(record, None), errors);
        let name = rec.word("name", None, &[], errors);
        if raw.iter().any(|(n, _)| *n == name) {
            errors.add(rec.span, format!("{what} '{name}' is defined twice"));
            continue;
        }
        raw.push((name, rec));
    }
    let mut resolved: HashMap<String, Rec<'a>> = HashMap::new();
    for (name, own) in &raw {
        let mut chain: Vec<&Rec<'a>> = Vec::new();
        let mut at = name.clone();
        while let Some((_, rec)) = raw.iter().find(|(n, _)| *n == at) {
            if chain.iter().any(|r| std::ptr::eq(*r, rec)) {
                errors.add(own.span, format!("{what} '{name}' extends itself (through '{at}')"));
                break;
            }
            chain.push(rec);
            if rec.get("extends").is_none() {
                break;
            }
            let base = rec.word("extends", None, &[], errors);
            if !raw.iter().any(|(n, _)| *n == base) {
                errors.add(rec.span_of("extends"), format!("{what} '{base}' isn't defined"));
                break;
            }
            at = base;
        }
        // From the root down: each type's fields over its base's.
        let mut merged = own.empty();
        for rec in chain.iter().rev() {
            merged = rec.over(&merged, &["name", "extends"]);
        }
        merged.span = own.span;
        resolved.insert(name.clone(), merged);
    }
    resolved
}

/// A record's settings over those of the type it names (`type=`).
fn with_type<'a>(
    rec: &Rec<'a>,
    types: &HashMap<String, Rec<'a>>,
    what: &str,
    own_only: &[&str],
    errors: &mut Errors,
) -> Rec<'a> {
    let Some(name) = rec.optional_word("type", &[], errors) else {
        return rec.over(&rec.empty(), own_only);
    };
    match types.get(&name) {
        Some(base) => rec.over(base, own_only),
        None => {
            errors.add(rec.span_of("type"), format!("{what} '{name}' isn't defined"));
            rec.over(&rec.empty(), own_only)
        }
    }
}

const NEURON_OWN: &[&str] = &["name", "type", "position", "node"];
const POPULATION_OWN: &[&str] = &[
    "name",
    "type",
    "count",
    "positions",
    "random",
    "grid",
    "line",
    "ring",
    "seed",
    "node",
];
const CONNECTION_OWN: &[&str] = &[
    "from",
    "to",
    "pattern",
    "count",
    "probability",
    "within",
    "self",
    "type",
];

pub(crate) fn convert(root: &Node) -> Result<NetworkDescription, Vec<ParseError>> {
    let mut errors = Errors::default();
    let Some(top) = Rec::of(root, "network description", &mut errors) else {
        return Err(errors.0);
    };
    top.check_keys(&schema::keys("network", None), &mut errors);
    if top.get("neurons").is_none() && top.get("populations").is_none() {
        errors.add(
            top.span,
            "a network needs neurons: 'neurons' (one by one) or 'populations'",
        );
    }

    // Nodes.
    let mut nodes: Vec<String> = Vec::new();
    for node in list(&top, "nodes", &mut errors) {
        let Some(rec) = Rec::of(node, "node", &mut errors) else {
            continue;
        };
        rec.check_keys(&schema::keys("node", None), &mut errors);
        let name = rec.word("name", None, &[], &mut errors);
        if name == "main" || nodes.contains(&name) {
            errors.add(
                rec.span,
                format!("node '{name}' is defined twice ('main' is the engine itself)"),
            );
        }
        nodes.push(name);
    }
    let node_of = |rec: &Rec<'_>, errors: &mut Errors| -> Option<String> {
        let name = rec.optional_word("node", &[], errors)?;
        if name != "main" && !nodes.contains(&name) {
            errors.add(rec.span_of("node"), format!("node '{name}' isn't defined in 'nodes'"));
        }
        Some(name)
    };

    // Learning rules, stickiness, modulators.
    let mut learning: BTreeMap<String, LearningRule> = BTreeMap::from([("flat".to_string(), LearningRule::None)]);
    for node in list(&top, "learning_rules", &mut errors) {
        if let Some((name, rule, span)) = learning_rule(node, &mut errors)
            && learning.insert(name.clone(), rule).is_some()
        {
            errors.add(span, format!("learning rule '{name}' is defined twice"));
        }
    }
    let mut stickiness: BTreeMap<String, Stickiness> = BTreeMap::from([("none".to_string(), Stickiness::None)]);
    for node in list(&top, "stickiness", &mut errors) {
        let Some(rec) = Rec::of(node, "stickiness function", &mut errors) else {
            continue;
        };
        rec.check_keys(&schema::keys("stickiness", None), &mut errors);
        let name = rec.word("name", None, &[], &mut errors);
        let tau_ms = rec.quantity("time", Quantity::Time, None, &mut errors);
        if stickiness
            .insert(name.clone(), Stickiness::Sigmoid { tau_ms })
            .is_some()
        {
            errors.add(rec.span, format!("stickiness function '{name}' is defined twice"));
        }
    }
    let modulators: Vec<ModulatorDescription> = list(&top, "modulators", &mut errors)
        .iter()
        .filter_map(|node| modulator(node, &mut errors))
        .collect();
    let mut modulator_index = HashMap::new();
    for (i, m) in modulators.iter().enumerate() {
        if modulator_index.insert(m.name.clone(), i as u16).is_some() {
            errors.add(top.span, format!("modulator '{}' is defined twice", m.name));
        }
    }

    // Neurons: types, then listed neurons, then populations.
    let neuron_types = types(
        list(&top, "neuron_types", &mut errors),
        "neuronType",
        "neuron type",
        &mut errors,
    );
    let mut neurons = Neurons::new();
    let mut taken: HashMap<String, Span> = HashMap::new();
    for node in list(&top, "neurons", &mut errors) {
        let Some(rec) = Rec::of(node, "neuron", &mut errors) else {
            continue;
        };
        rec.check_keys(&schema::keys("neuron", None), &mut errors);
        let name = rec.word("name", None, &[], &mut errors);
        if let Some(first) = taken.insert(name.clone(), rec.span) {
            errors.add(rec.span, format!("neuron '{name}' is already defined at {first}"));
        }
        let settings = with_type(&rec, &neuron_types, "neuron type", NEURON_OWN, &mut errors);
        let location = rec.point("position", &mut errors).unwrap_or_else(|| {
            rec.missing("position", &mut errors);
            [0.0; 3]
        });
        let node_name = node_of(&rec, &mut errors);
        if let Some(mut n) = neuron_settings(&settings, &modulator_index, &mut errors) {
            n.id = name;
            n.location_um = location;
            n.node = node_name;
            n.span = rec.span;
            neurons.push(n);
        }
    }
    let mut populations: Vec<PopulationDescription> = Vec::new();
    for node in list(&top, "populations", &mut errors) {
        let Some(rec) = Rec::of(node, "population", &mut errors) else {
            continue;
        };
        rec.check_keys(&schema::keys("population", None), &mut errors);
        let name = rec.word("name", None, &[], &mut errors);
        if populations.iter().any(|p| p.name == name) || taken.contains_key(&name) {
            errors.add(
                rec.span,
                format!("'{name}' is already the name of a population or neuron"),
            );
        }
        let settings = with_type(&rec, &neuron_types, "neuron type", POPULATION_OWN, &mut errors);
        let node_name = node_of(&rec, &mut errors);
        let Some(mut template) = neuron_settings(&settings, &modulator_index, &mut errors) else {
            continue;
        };
        template.node = node_name.clone();
        template.span = rec.span;
        let (locations, own_names) = placement(&rec, &name, &mut errors);
        if let Some(own_names) = &own_names {
            for n in own_names {
                if let Some(first) = taken.insert(n.clone(), rec.span) {
                    errors.add(rec.span, format!("neuron '{n}' is already defined at {first}"));
                }
            }
        }
        let first = neurons.len() as u32;
        let count = locations.len() as u32;
        neurons.push_population(name.clone(), template, locations, own_names);
        populations.push(PopulationDescription {
            name,
            first,
            count,
            node: node_name,
            span: rec.span,
        });
    }
    let names = neurons.names();
    // A neuron named one by one can't have the name of a population's neuron (`<population>-<i>`).
    for (n, &span) in &taken {
        if let Some((p, k)) = n.rsplit_once('-')
            && let Some(pop) = populations.iter().find(|q| q.name == p)
            && k.parse::<u32>().is_ok_and(|k| k < pop.count)
            && names
                .index(n)
                .is_some_and(|i| !(pop.first..pop.first + pop.count).contains(&i))
        {
            errors.add(
                span,
                format!("neuron '{n}' has the name of a neuron of population '{p}'"),
            );
        }
    }

    // Sets, in order (a set may use earlier ones).
    let mut sets: Vec<SetDescription> = Vec::new();
    for node in list(&top, "sets", &mut errors) {
        let Some(rec) = Rec::of(node, "set", &mut errors) else {
            continue;
        };
        rec.check_keys(&schema::keys("set", None), &mut errors);
        let name = rec.word("name", None, &[], &mut errors);
        if sets.iter().any(|s| s.name == name)
            || populations.iter().any(|p| p.name == name)
            || names.index(&name).is_some()
        {
            errors.add(
                rec.span,
                format!("'{name}' is already the name of a set, population or neuron"),
            );
        }
        let mut members = resolve(&rec, "members", &names, &populations, &sets, &mut errors);
        members.sort_unstable();
        members.dedup();
        sets.push(SetDescription {
            name,
            members,
            node: node_of(&rec, &mut errors),
            span: rec.span,
        });
    }

    // Connections.
    let synapse_types = types(
        list(&top, "synapse_types", &mut errors),
        "synapseType",
        "synapse type",
        &mut errors,
    );
    let mut connections = Vec::new();
    for node in list(&top, "connections", &mut errors) {
        let Some(rec) = Rec::of(node, "connection", &mut errors) else {
            continue;
        };
        rec.check_keys(&schema::keys("connection", None), &mut errors);
        let from = resolve(&rec, "from", &names, &populations, &sets, &mut errors);
        let to = resolve(&rec, "to", &names, &populations, &sets, &mut errors);
        let pattern = match rec
            .word("pattern", Some("all_to_all"), schema::PATTERNS, &mut errors)
            .as_str()
        {
            "one_to_one" => {
                if from.len() != to.len() {
                    errors.add(
                        rec.span,
                        format!(
                            "one_to_one needs as many senders as receivers ({} and {})",
                            from.len(),
                            to.len()
                        ),
                    );
                }
                Pattern::OneToOne
            }
            "fan_in" => {
                let count = rec.quantity("count", Quantity::Dimensionless, None, &mut errors);
                if !(count >= 0.0 && count.fract() == 0.0 && count <= f64::from(u32::MAX)) {
                    errors.add(rec.span_of("count"), "'count' must be a whole number");
                }
                Pattern::FanIn(count as u32)
            }
            "random" => {
                let p = rec.quantity("probability", Quantity::Dimensionless, None, &mut errors);
                if !(0.0..=1.0).contains(&p) {
                    errors.add(rec.span_of("probability"), "'probability' must be from 0 to 1");
                }
                Pattern::Random(p)
            }
            _ => Pattern::AllToAll,
        };
        for (key, applies) in [
            ("count", matches!(pattern, Pattern::FanIn(_))),
            ("probability", matches!(pattern, Pattern::Random(_))),
        ] {
            if !applies && rec.get(key).is_some() {
                errors.add(rec.span_of(key), format!("'{key}' doesn't apply to this pattern"));
            }
        }
        let within_um = rec.optional("within", Quantity::Length, &mut errors);
        let autapses = rec.boolean("self", false, &mut errors);
        let settings = with_type(&rec, &synapse_types, "synapse type", CONNECTION_OWN, &mut errors);
        let Some(synapse) = synapse_spec(&settings, &learning, &stickiness, &mut errors) else {
            continue;
        };
        connections.push(ConnectionDescription {
            from,
            to,
            pattern,
            within_um,
            autapses,
            synapse,
            span: rec.span,
        });
    }

    if errors.0.is_empty() {
        Ok(NetworkDescription {
            nodes,
            neurons,
            populations,
            sets,
            connections,
            learning,
            stickiness,
            modulators,
        })
    } else {
        errors.0.sort_by_key(|e| e.span);
        errors.0.dedup();
        Err(errors.0)
    }
}

/// The neurons a `from`, `to` or `members` names: populations, sets, neuron names, id ranges, or a list of them.
fn resolve(
    rec: &Rec<'_>,
    key: &str,
    names: &crate::NeuronNames,
    populations: &[PopulationDescription],
    sets: &[SetDescription],
    errors: &mut Errors,
) -> Vec<u32> {
    let Some(node) = rec.get(key) else {
        rec.missing(key, errors);
        return Vec::new();
    };
    let items: Vec<&Node> = match &node.value {
        Value::List(items) => items.iter().collect(),
        _ => vec![node],
    };
    let mut words: Vec<&String> = Vec::new();
    for item in items {
        match &item.value {
            Value::Ids(ids) => words.extend(ids.iter()),
            _ => errors.add(
                item.span,
                format!("'{key}' takes neurons: names, populations, sets or id ranges"),
            ),
        }
    }
    let mut out = Vec::new();
    let mut unknown: Vec<&str> = Vec::new();
    for word in words {
        if let Some(p) = populations.iter().find(|p| p.name == *word) {
            out.extend(p.first..p.first + p.count);
        } else if let Some(s) = sets.iter().find(|s| s.name == *word) {
            out.extend(&s.members);
        } else if let Some(i) = names.index(word) {
            out.push(i);
        } else {
            unknown.push(word);
        }
    }
    match unknown.as_slice() {
        [] => {}
        [one] => errors.add(
            node.span,
            format!("'{one}' isn't a neuron, a population or a set (defined before here)"),
        ),
        many => errors.add(
            node.span,
            format!(
                "{} names aren't neurons, populations or sets: {}{}",
                many.len(),
                many.iter().take(5).copied().collect::<Vec<_>>().join(", "),
                if many.len() > 5 { ", …" } else { "" }
            ),
        ),
    }
    out
}

/// A population's positions (and names, if it names its neurons), from its placement.
fn placement(rec: &Rec<'_>, name: &str, errors: &mut Errors) -> (Vec<[f64; 3]>, Option<Vec<String>>) {
    let given: Vec<&str> = ["positions", "random", "grid", "line", "ring"]
        .into_iter()
        .filter(|k| rec.get(k).is_some())
        .collect();
    let counted = |errors: &mut Errors| -> usize {
        let c = rec.quantity("count", Quantity::Dimensionless, None, errors);
        const MAX: f64 = 100_000_000.0;
        if !((1.0..=MAX).contains(&c) && c.fract() == 0.0) {
            errors.add(
                rec.span_of("count"),
                format!("'count' must be a whole number from 1 to {MAX:e}"),
            );
            return 0;
        }
        c as usize
    };
    let no_count = |why: &str, errors: &mut Errors| {
        if rec.get("count").is_some() {
            errors.add(rec.span_of("count"), format!("'count' comes from {why}; leave it out"));
        }
    };
    match given.as_slice() {
        ["positions"] => {
            no_count("the positions", errors);
            let Some(Value::List(items)) = rec.get("positions").map(|n| &n.value) else {
                errors.add(rec.span_of("positions"), "'positions' must be a list of points");
                return (Vec::new(), None);
            };
            let named = items
                .first()
                .is_some_and(|n| matches!(&n.value, Value::Record(f) if f.iter().any(|f| f.key == "at")));
            if named {
                let mut locations = Vec::new();
                let mut names = Vec::new();
                for item in items {
                    let Some(p) = Rec::of(item, "named position", errors) else {
                        continue;
                    };
                    p.check_keys(&schema::keys("namedPosition", None), errors);
                    names.push(p.word("name", None, &[], errors));
                    locations.push(p.point("at", errors).unwrap_or_else(|| {
                        p.missing("at", errors);
                        [0.0; 3]
                    }));
                }
                (locations, Some(names))
            } else {
                (items.iter().map(|n| point(n, errors)).collect(), None)
            }
        }
        ["random"] => {
            let n = counted(errors);
            let Some(r) = rec.record("random", "random placement", errors) else {
                return (Vec::new(), None);
            };
            r.check_keys(&schema::keys("randomPlacement", None), errors);
            let seed = rec.quantity("seed", Quantity::Dimensionless, Some(1.0), errors) as u64;
            use rand::{RngExt, SeedableRng};
            // Positions depend only on the population (its name and seed), not on what else is in the file.
            let name_hash = name.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
                (h ^ u64::from(b)).wrapping_mul(0x1000_0000_01b3)
            });
            let mut rng = rand_xoshiro::Xoshiro256PlusPlus::seed_from_u64(seed ^ name_hash);
            match (r.record("box", "box", errors), r.record("ball", "ball", errors)) {
                (Some(b), None) => {
                    b.check_keys(&schema::keys("box", None), errors);
                    let corner = b.point("corner", errors).unwrap_or_else(|| {
                        b.missing("corner", errors);
                        [0.0; 3]
                    });
                    let size = b.triple("size", Quantity::Length, errors).unwrap_or_else(|| {
                        b.missing("size", errors);
                        [0.0; 3]
                    });
                    if size.iter().any(|s| s.is_nan() || *s < 0.0) {
                        errors.add(b.span, "a box's sides can't be negative");
                    }
                    let locations = (0..n)
                        .map(|_| std::array::from_fn(|d| corner[d] + size[d] * rng.random::<f64>()))
                        .collect();
                    (locations, None)
                }
                (None, Some(b)) => {
                    b.check_keys(&schema::keys("ball", None), errors);
                    let center = b.point("center", errors).unwrap_or_else(|| {
                        b.missing("center", errors);
                        [0.0; 3]
                    });
                    let radius = b.quantity("radius", Quantity::Length, None, errors);
                    if radius.is_nan() || radius <= 0.0 {
                        errors.add(b.span, "a ball needs a positive radius");
                        return (Vec::new(), None);
                    }
                    let locations = (0..n)
                        .map(|_| {
                            loop {
                                let p: [f64; 3] = std::array::from_fn(|_| 2.0 * rng.random::<f64>() - 1.0);
                                if p.iter().map(|v| v * v).sum::<f64>() <= 1.0 {
                                    break std::array::from_fn(|d| center[d] + radius * p[d]);
                                }
                            }
                        })
                        .collect();
                    (locations, None)
                }
                _ => {
                    errors.add(r.span, "random placement needs a box=(…) or a ball=(…), not both");
                    (Vec::new(), None)
                }
            }
        }
        ["grid"] => {
            no_count("the grid's shape", errors);
            let Some(g) = rec.record("grid", "grid", errors) else {
                return (Vec::new(), None);
            };
            g.check_keys(&schema::keys("grid", None), errors);
            let origin = g.point("origin", errors).unwrap_or([0.0; 3]);
            let spacing = g.triple("spacing", Quantity::Length, errors).unwrap_or_else(|| {
                g.missing("spacing", errors);
                [0.0; 3]
            });
            let shape = g.triple("shape", Quantity::Dimensionless, errors).unwrap_or_else(|| {
                g.missing("shape", errors);
                [1.0; 3]
            });
            if shape.iter().any(|s| *s < 1.0 || s.fract() != 0.0) || shape.iter().product::<f64>() > 1e8 {
                errors.add(
                    g.span_of("shape"),
                    "a grid's shape is three whole numbers of at least 1",
                );
                return (Vec::new(), None);
            }
            let [nx, ny, nz] = shape.map(|s| s as usize);
            let mut locations = Vec::with_capacity(nx * ny * nz);
            for k in 0..nz {
                for j in 0..ny {
                    for i in 0..nx {
                        let at = [i, j, k];
                        locations.push(std::array::from_fn(|d| origin[d] + spacing[d] * at[d] as f64));
                    }
                }
            }
            (locations, None)
        }
        ["line"] => {
            let n = counted(errors);
            let Some(l) = rec.record("line", "line", errors) else {
                return (Vec::new(), None);
            };
            l.check_keys(&schema::keys("line", None), errors);
            let from = l.point("from", errors).unwrap_or_else(|| {
                l.missing("from", errors);
                [0.0; 3]
            });
            let to = l.point("to", errors).unwrap_or_else(|| {
                l.missing("to", errors);
                [0.0; 3]
            });
            let at = |i: usize| if n > 1 { i as f64 / (n - 1) as f64 } else { 0.0 };
            let locations = (0..n)
                .map(|i| std::array::from_fn(|d| from[d] + (to[d] - from[d]) * at(i)))
                .collect();
            (locations, None)
        }
        ["ring"] => {
            let n = counted(errors);
            let Some(r) = rec.record("ring", "ring", errors) else {
                return (Vec::new(), None);
            };
            r.check_keys(&schema::keys("ring", None), errors);
            let center = r.point("center", errors).unwrap_or([0.0; 3]);
            let radius = r.quantity("radius", Quantity::Length, None, errors);
            let axis = r.word("axis", Some("z"), &["x", "y", "z"], errors);
            let start = r.quantity("start_angle", Quantity::Angle, Some(0.0), errors);
            let locations = (0..n)
                .map(|i| {
                    let a = start + std::f64::consts::TAU * i as f64 / n as f64;
                    let (u, v) = (radius * a.cos(), radius * a.sin());
                    let offset = match axis.as_str() {
                        "x" => [0.0, u, v],
                        "y" => [v, 0.0, u],
                        _ => [u, v, 0.0],
                    };
                    std::array::from_fn(|d| center[d] + offset[d])
                })
                .collect();
            (locations, None)
        }
        [] => {
            errors.add(
                rec.span,
                "a population needs a placement: positions, random, grid, line or ring",
            );
            (Vec::new(), None)
        }
        _ => {
            errors.add(
                rec.span,
                format!("a population has one placement, not {}", given.join(" and ")),
            );
            (Vec::new(), None)
        }
    }
}

fn learning_rule(node: &Node, errors: &mut Errors) -> Option<(String, LearningRule, Span)> {
    let rec = Rec::of(node, "learning rule", errors)?;
    let rule = rec.word("rule", None, schema::RULES, errors);
    rec.check_keys(&schema::keys("learningRule", Some(&rule)), errors);
    let exponential = |errors: &mut Errors| ExponentialStdp {
        inhibition_amplitude: rec.quantity("depression", Quantity::Dimensionless, None, errors),
        inhibition_period_ms: rec.quantity("depression_time", Quantity::Time, None, errors),
        excitation_amplitude: rec.quantity("potentiation", Quantity::Dimensionless, None, errors),
        excitation_period_ms: rec.quantity("potentiation_time", Quantity::Time, None, errors),
    };
    let value = match rule.as_str() {
        "stdp_hard" => LearningRule::HardLimit(exponential(errors)),
        "stdp_soft" => LearningRule::SoftLimit(exponential(errors)),
        "stdp_alpha" => LearningRule::Alpha(AlphaStdp::new(
            rec.quantity("baseline", Quantity::Dimensionless, None, errors),
            rec.quantity("time", Quantity::Time, None, errors),
            rec.quantity("rate", Quantity::Dimensionless, None, errors),
        )),
        _ => return None,
    };
    let name = rec.word("name", None, &[], errors);
    if name == "flat" {
        errors.add(rec.span, "'flat' means no learning; give this rule another name");
    }
    Some((name, value, rec.span))
}

fn modulator(node: &Node, errors: &mut Errors) -> Option<ModulatorDescription> {
    let rec = Rec::of(node, "modulator", errors)?;
    rec.check_keys(&schema::keys("modulator", None), errors);
    let description = ModulatorDescription {
        name: rec.word("name", None, &[], errors),
        clearance_ms: rec.quantity("clearance_time", Quantity::Time, None, errors),
        radius_um: rec.quantity("reach", Quantity::Length, Some(f64::INFINITY), errors),
        speed_m_per_s: rec.quantity("speed", Quantity::Speed, Some(f64::INFINITY), errors),
        latency_ms: rec.quantity("latency", Quantity::Time, Some(1.0), errors),
        falloff_um: rec.optional("falloff", Quantity::Length, errors),
    };
    if description.clearance_ms <= 0.0 {
        errors.add(rec.span, "the clearance time must be positive");
    }
    Some(description)
}

/// Looks up a modulator by the name in `key`.
fn modulator_ref(rec: &Rec<'_>, key: &str, modulators: &HashMap<String, u16>, errors: &mut Errors) -> u16 {
    let name = rec.word(key, None, &[], errors);
    match modulators.get(&name) {
        Some(&i) => i,
        None => {
            errors.add(
                rec.span_of(key),
                format!("modulator '{name}' isn't defined in 'modulators'"),
            );
            0
        }
    }
}

fn receptor(node: &Node, modulators: &HashMap<String, u16>, errors: &mut Errors) -> Option<Receptor> {
    let rec = Rec::of(node, "receptor", errors)?;
    rec.check_keys(&schema::keys("receptor", None), errors);
    let three_factor = match (rec.get("eligibility_time"), rec.get("learning_rate")) {
        (None, None) => None,
        (Some(_), Some(_)) => Some(ThreeFactor {
            eligibility_ms: rec.quantity("eligibility_time", Quantity::Time, None, errors),
            rate: rec.quantity("learning_rate", Quantity::Dimensionless, None, errors),
        }),
        _ => {
            errors.add(
                rec.span,
                "three-factor learning needs both 'eligibility_time' and 'learning_rate'",
            );
            None
        }
    };
    Some(Receptor {
        modulator: modulator_ref(&rec, "modulator", modulators, errors),
        excitatory_gain: rec.quantity("excitatory_gain", Quantity::Dimensionless, Some(0.0), errors),
        inhibitory_gain: rec.quantity("inhibitory_gain", Quantity::Dimensionless, Some(0.0), errors),
        threshold: rec.quantity("threshold_gain", Quantity::Dimensionless, Some(0.0), errors),
        plasticity: rec.quantity("plasticity_gain", Quantity::Dimensionless, Some(0.0), errors),
        three_factor,
        direct_mv: rec.quantity("direct_potential", Quantity::Potential, Some(0.0), errors),
    })
}

/// Defaults for settings a neuron doesn't give.
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
}

/// A neuron's settings (a type's merged with its own), without its name, position or node.
fn neuron_settings(rec: &Rec<'_>, modulators: &HashMap<String, u16>, errors: &mut Errors) -> Option<NeuronDescription> {
    let q = |key, quantity, default, errors: &mut Errors| rec.quantity(key, quantity, Some(default), errors);
    let dynamics = rec.word("dynamics", Some("monostable_integrator"), schema::DYNAMICS, errors);
    let inhibitory = rec.boolean("inhibitory", false, errors);

    let mut threshold_mv = q("threshold", Quantity::Potential, default::THRESHOLD_MV, errors);
    let resonance = |errors: &mut Errors| -> Option<(Resonance, Rec<'_>)> {
        let Some(res) = rec.record("resonance", "resonance", errors) else {
            rec.missing("resonance", errors);
            return None;
        };
        res.check_keys(&schema::keys("resonance", None), errors);
        let r = Resonance {
            frequency_hz: res.quantity("frequency", Quantity::Frequency, None, errors),
            damping_ms: res.quantity("damping_time", Quantity::Time, None, errors),
        };
        if r.frequency_hz <= 0.0 {
            errors.add(res.span, "the resonant frequency must be positive");
        }
        Some((r, res))
    };
    let kind = match dynamics.as_str() {
        "bistable_integrator" => {
            threshold_mv = q(
                "tonic_on_threshold",
                Quantity::Potential,
                default::BISTABLE_THRESHOLD_MV,
                errors,
            );
            NeuronKind::BistableIntegrator {
                resting_threshold_mv: q(
                    "tonic_off_threshold",
                    Quantity::Potential,
                    default::RESTING_THRESHOLD_MV,
                    errors,
                ),
                tonic_rate_hz: q("tonic_rate", Quantity::Frequency, default::TONIC_RATE_HZ, errors),
            }
        }
        "monostable_resonator" => NeuronKind::MonostableResonator(resonance(errors)?.0),
        "bistable_resonator" => {
            let (r, res) = resonance(errors)?;
            let cycle_mv = res.quantity("limit_cycle", Quantity::Potential, None, errors);
            let separatrix_mv = res.quantity("separatrix", Quantity::Potential, None, errors);
            if separatrix_mv >= cycle_mv {
                errors.add(res.span, "the separatrix must be smaller than the limit cycle");
            }
            NeuronKind::BistableResonator {
                resonance: r,
                cycle_mv,
                separatrix_mv,
            }
        }
        _ => NeuronKind::MonostableIntegrator,
    };

    let homeostasis = rec.record("homeostasis", "homeostasis", errors).map(|h| {
        h.check_keys(&schema::keys("homeostasis", None), errors);
        Homeostasis {
            target_hz: h.quantity("target_rate", Quantity::Frequency, None, errors),
            strength: h.quantity("strength", Quantity::Dimensionless, None, errors),
            window_ms: h.quantity("window", Quantity::Time, Some(1_000.0), errors),
            min_mv: h.quantity("min_shift", Quantity::Potential, Some(f64::NEG_INFINITY), errors),
            max_mv: h.quantity("max_shift", Quantity::Potential, Some(f64::INFINITY), errors),
        }
    });
    let intrinsic = rec.record("intrinsic_plasticity", "intrinsic plasticity", errors);
    if let Some(ip) = &intrinsic {
        ip.check_keys(&schema::keys("intrinsicPlasticity", None), errors);
    }
    let ip = |key, quantity, default, errors: &mut Errors| {
        intrinsic
            .as_ref()
            .map_or(default, |r| r.quantity(key, quantity, Some(default), errors))
    };

    let params = NeuronParams {
        kind,
        threshold_mv,
        refractory_ms: q("refractory_period", Quantity::Time, default::REFRACTORY_MS, errors),
        base_refractoriness: q(
            "relative_refractoriness",
            Quantity::Flux,
            default::BASE_REFRACTORINESS,
            errors,
        ),
        min_potential_mv: q("resting_potential", Quantity::Potential, 0.0, errors),
        decay_ms: q("decay_time", Quantity::Time, default::DECAY_MS, errors),
        rise_ms: q("rise_time", Quantity::Time, default::RISE_MS, errors),
        noise_mv: q("membrane_noise", Quantity::Potential, default::NOISE_MV, errors),
        spike_mv: q("spike_potential", Quantity::Potential, default::SPIKE_MV, errors),
        intrinsic: IntrinsicParams {
            base_mv: ip("step", Quantity::Potential, 0.0, errors),
            learning_mv: ip("amplitude", Quantity::Potential, 0.0, errors),
            decay_ms: ip("decay_time", Quantity::Time, default::IP_DECAY_MS, errors),
            homeostasis,
        },
    };

    let receptors = match rec.get("receptors").map(|n| (&n.value, n.span)) {
        None => Vec::new(),
        Some((Value::List(items), _)) => items.iter().filter_map(|n| receptor(n, modulators, errors)).collect(),
        Some((_, span)) => {
            errors.add(span, "'receptors' must be a list: receptors=[(modulator=…, …)]");
            Vec::new()
        }
    };
    let release = rec.record("releases", "release", errors).map(|r| {
        r.check_keys(&schema::keys("modulatorRelease", None), errors);
        (
            modulator_ref(&r, "modulator", modulators, errors),
            r.quantity("amount", Quantity::Dimensionless, Some(1.0), errors),
        )
    });

    Some(NeuronDescription {
        id: String::new(),
        node: None,
        inhibitory,
        params,
        conduction_m_per_s: q("conduction_speed", Quantity::Speed, default::CONDUCTION_M_PER_S, errors),
        location_um: [0.0; 3],
        receptors,
        release,
        span: rec.span,
    })
}

/// A synapse's settings (a type's merged with a connection's).
fn synapse_spec(
    rec: &Rec<'_>,
    learning: &BTreeMap<String, LearningRule>,
    stickiness: &BTreeMap<String, Stickiness>,
    errors: &mut Errors,
) -> Option<SynapseSpec> {
    let Some(weight) = rec.weight("weight", errors) else {
        if rec.get("weight").is_none() {
            errors.add(
                rec.span,
                "this connection needs a 'weight' (here or in its synapse type)",
            );
        }
        return None;
    };
    let learning_name = rec.word("learning", Some("flat"), &[], errors);
    if !learning.contains_key(&learning_name) {
        errors.add(
            rec.span_of("learning"),
            format!("learning rule '{learning_name}' isn't defined in 'learning_rules'"),
        );
    }
    let stickiness_name = rec.word("stickiness", Some("none"), &[], errors);
    if !stickiness.contains_key(&stickiness_name) {
        errors.add(
            rec.span_of("stickiness"),
            format!("stickiness function '{stickiness_name}' isn't defined in 'stickiness'"),
        );
    }
    let decay = match rec.record("decay", "weight decay", errors) {
        None => WeightDecay::None,
        Some(d) => {
            d.check_keys(&schema::keys("decay", None), errors);
            WeightDecay::Exponential {
                tau_ms: d.quantity("time", Quantity::Time, None, errors),
            }
        }
    };
    let limit = match rec.record("limits", "weight limits", errors) {
        None => WeightLimit::Unbounded,
        Some(l) => {
            l.check_keys(&schema::keys("limits", None), errors);
            let lower = l.quantity("min", Quantity::Dimensionless, None, errors);
            let upper = l.quantity("max", Quantity::Dimensionless, None, errors);
            if lower > upper {
                errors.add(
                    l.span,
                    format!("the lower bound {lower} is above the upper bound {upper}"),
                );
            }
            WeightLimit::Bounded { lower, upper }
        }
    };
    let release = rec.record("release", "release", errors).map(|r| {
        r.check_keys(&schema::keys("release", None), errors);
        let d = DEFAULT_RELEASE;
        let n = |key, default, errors: &mut Errors| r.quantity(key, Quantity::Dimensionless, Some(default), errors);
        let t = |key, default, errors: &mut Errors| r.quantity(key, Quantity::Time, Some(default), errors);
        Release {
            facilitation_base: n("facilitation_base", d.facilitation_base, errors),
            facilitation_magnitude: n("facilitation_step", d.facilitation_magnitude, errors),
            facilitation_tau_ms: t("facilitation_time", d.facilitation_tau_ms, errors),
            depletion_base: n("depletion_base", d.depletion_base, errors),
            depletion_magnitude: n("depletion_step", d.depletion_magnitude, errors),
            depletion_tau_ms: t("depletion_time", d.depletion_tau_ms, errors),
        }
    });
    let sign = rec
        .optional_word("sign", &["excitatory", "inhibitory"], errors)
        .map(|s| {
            if s == "inhibitory" {
                Sign::Inhibitory
            } else {
                Sign::Excitatory
            }
        });
    Some(SynapseSpec {
        weight,
        equilibrium: rec.weight("equilibrium", errors),
        sign,
        delay_ms: rec.quantity("delay", Quantity::Time, Some(SynapseSpec::DEFAULT_DELAY_MS), errors),
        learning: learning_name,
        stickiness: stickiness_name,
        decay,
        limit,
        release,
        weight_noise: rec.quantity("weight_noise", Quantity::Dimensionless, Some(0.0), errors),
    })
}
