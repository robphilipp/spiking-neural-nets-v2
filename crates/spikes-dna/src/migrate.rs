//! Converts a description in the older (v1-derived) form to the current one, keeping what it means:
//!
//! - neurons alike become neuron types (named after what their names share); neurons and populations keep their
//!   own names and positions;
//! - the synapse settings each receiving neuron gave its incoming synapses (`WDF`, `WLF`, `SRP`, `wnm`), with the
//!   old defaults written out (weight decay 1.5 s, release on), and each connection's learning, stickiness and sign,
//!   become synapse types; a connection whose receivers differ is split;
//! - `CON` and `RUL` become connections (all-to-all, `fan_in`, `random`); groups a rule names become sets;
//! - every key gets its long name.
//!
//! The file's leading comments are kept; comments inside it aren't.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

use crate::syntax::{self, Field, Node, ParseError, Span, Value};

/// Converts a description in the older form to the current one.
///
/// # Errors
/// If it doesn't parse, or uses keys the older form didn't have.
pub fn migrate(text: &str) -> Result<String, Vec<ParseError>> {
    let root = syntax::parse(text).map_err(|e| vec![e])?;
    let Value::Record(top) = &root.value else {
        return Err(vec![error(root.span, "expected a record")]);
    };
    let mut m = Migration::default();
    m.run(top);
    if !m.errors.is_empty() {
        return Err(m.errors);
    }
    let header: String = text
        .lines()
        .take_while(|l| l.trim().is_empty() || l.trim_start().starts_with("//"))
        .map(|l| format!("{l}\n"))
        .collect();
    Ok(format!("{header}{}", m.output()))
}

fn error(span: Span, message: impl Into<String>) -> ParseError {
    ParseError {
        span,
        message: message.into(),
    }
}

/// Settings as (key, rendered value), in a canonical order.
type Settings = Vec<(String, String)>;

#[derive(Default)]
struct Migration {
    errors: Vec<ParseError>,
    neuron_types: Vec<(String, Settings)>,
    neurons: Vec<String>,
    populations: Vec<String>,
    sets: Vec<String>,
    synapse_types: Vec<(String, Settings)>,
    connections: Vec<String>,
    learning: Vec<String>,
    stickiness: Vec<String>,
    modulators: Vec<String>,
    /// Each neuron's (and population's) incoming-synapse settings and outgoing synaptic delay.
    synapses_of: HashMap<String, Settings>,
    delay_of: HashMap<String, Option<String>>,
    /// Population names; group → members.
    population_names: Vec<String>,
    groups: BTreeMap<String, Vec<String>>,
}

fn get<'a>(fields: &'a [Field], key: &str) -> Option<&'a Node> {
    fields.iter().find(|f| f.key == key).map(|f| &f.value)
}

fn word(node: &Node) -> Option<String> {
    match &node.value {
        Value::Ids(w) if w.len() == 1 => Some(w[0].clone()),
        _ => None,
    }
}

fn number(value: f64) -> String {
    format!("{value}")
}

/// A value as text in the current form.
fn render(node: &Node) -> String {
    match &node.value {
        Value::Number { value, unit: None } => number(*value),
        Value::Number { value, unit: Some(u) } => format!("{} {u}", number(*value)),
        Value::Random { low, high } => format!("uniform({}, {})", number(*low), number(*high)),
        Value::Ids(words) => names(words),
        Value::Call { name, args } => format!("{name}({})", args.iter().map(render).collect::<Vec<_>>().join(", ")),
        Value::List(items) => format!("[{}]", items.iter().map(render).collect::<Vec<_>>().join(", ")),
        Value::Record(fields) => format!(
            "({})",
            fields
                .iter()
                .map(|f| format!("{}={}", f.key, render(&f.value)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// Neuron names, compressed into id ranges where they run (`input-1{1:7:1}`); a list if several.
fn names(words: &[String]) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut i = 0;
    let split = |w: &str| -> Option<(String, i64)> {
        let digits = w.chars().rev().take_while(char::is_ascii_digit).count();
        if digits == 0 || digits == w.len() || w[w.len() - digits..].starts_with('0') && digits > 1 {
            return None;
        }
        Some((w[..w.len() - digits].to_string(), w[w.len() - digits..].parse().ok()?))
    };
    while i < words.len() {
        let mut j = i + 1;
        if let Some((prefix, start)) = split(&words[i])
            && let Some((p2, second)) = words.get(i + 1).and_then(|w| split(w))
            && p2 == prefix
            && second > start
        {
            let step = second - start;
            let mut last = second;
            j = i + 2;
            while let Some((p, n)) = words.get(j).and_then(|w| split(w)) {
                if p != prefix || n != last + step {
                    break;
                }
                last = n;
                j += 1;
            }
            if j - i >= 3 {
                parts.push(format!("{prefix}{{{start}:{last}:{step}}}"));
                i = j;
                continue;
            }
            j = i + 1;
        }
        parts.push(words[i].clone());
        i = j;
    }
    if parts.len() == 1 {
        parts.remove(0)
    } else {
        format!("[{}]", parts.join(", "))
    }
}

/// The text of a list of settings: `key=value, …`.
fn joined(settings: &Settings) -> String {
    settings
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// A nested record's fields renamed: `(new=value, …)` for the keys given.
fn renamed(fields: &[Field], map: &[(&str, &str)], errors: &mut Vec<ParseError>) -> String {
    let mut out = Vec::new();
    for f in fields {
        match map.iter().find(|(old, _)| *old == f.key) {
            Some((_, new)) => out.push(format!("{new}={}", render(&f.value))),
            None => errors.push(error(f.span, format!("unknown key '{}'", f.key))),
        }
    }
    format!("({})", out.join(", "))
}

const SIMPLE: &[(&str, &str)] = &[
    ("mst", "threshold"),
    ("lct", "tonic_on_threshold"),
    ("rst", "tonic_off_threshold"),
    ("tfr", "tonic_rate"),
    ("rfp", "refractory_period"),
    ("rfb", "relative_refractoriness"),
    ("mnp", "resting_potential"),
    ("mpd", "decay_time"),
    ("mpr", "rise_time"),
    ("mpn", "membrane_noise"),
    ("spp", "spike_potential"),
    ("csp", "conduction_speed"),
];

impl Migration {
    fn run(&mut self, top: &[Field]) {
        for f in top {
            if !["GRP", "NRN", "POP", "CON", "RUL", "LRN", "CST", "MOD"].contains(&f.key.as_str()) {
                self.errors.push(error(f.span, format!("unknown section '{}'", f.key)));
            }
        }
        let items = |key: &str| -> &[Node] {
            match get(top, key).map(|n| &n.value) {
                Some(Value::List(items)) => items,
                _ => &[],
            }
        };
        for node in items("MOD") {
            self.modulator(node);
        }
        for node in items("LRN") {
            self.learning_rule(node);
        }
        for node in items("CST") {
            self.stickiness_fn(node);
        }
        // Neurons, grouped into types by their settings.
        let mut listed: Vec<(String, Settings, String)> = Vec::new();
        for node in items("NRN") {
            let Value::Record(fields) = &node.value else { continue };
            let name = get(fields, "nid").and_then(word).unwrap_or_default();
            let group = get(fields, "grp").and_then(word).unwrap_or_else(|| "default".into());
            self.groups.entry(group).or_default().push(name.clone());
            let settings = self.neuron_settings(fields, &["nid", "grp", "LOC"]);
            let position = get(fields, "LOC").map_or_else(|| "[0, 0, 0]".into(), |n| self.position(n));
            listed.push((name, settings, position));
        }
        let mut by_settings: Vec<(Settings, Vec<String>)> = Vec::new();
        for (name, settings, _) in &listed {
            match by_settings.iter_mut().find(|(s, _)| s == settings) {
                Some((_, members)) => members.push(name.clone()),
                None => by_settings.push((settings.clone(), vec![name.clone()])),
            }
        }
        let mut type_of: HashMap<Settings, String> = HashMap::new();
        for (settings, members) in &by_settings {
            if settings.is_empty() || members.len() < 2 {
                continue;
            }
            let base = type_name(members);
            let mut name = base.clone();
            let mut k = 2;
            while self.neuron_types.iter().any(|(n, _)| *n == name) {
                name = format!("{base}_{k}");
                k += 1;
            }
            type_of.insert(settings.clone(), name.clone());
            self.neuron_types.push((name, settings.clone()));
        }
        for (name, settings, position) in listed {
            let mut parts = vec![format!("name={name}")];
            match type_of.get(&settings) {
                Some(t) => parts.push(format!("type={t}")),
                None if !settings.is_empty() => parts.push(joined(&settings)),
                None => {}
            }
            parts.push(format!("position={position}"));
            self.neurons.push(format!("({})", parts.join(", ")));
        }
        for node in items("POP") {
            self.population(node);
        }
        for node in items("CON") {
            self.connection(node);
        }
        for node in items("RUL") {
            self.rule(node);
        }
    }

    /// A neuron's (or population's) settings in the new form; also records its incoming-synapse settings and
    /// outgoing delay under its name.
    fn neuron_settings(&mut self, fields: &[Field], skip: &[&str]) -> Settings {
        let name = get(fields, "nid")
            .or_else(|| get(fields, "nam"))
            .and_then(word)
            .unwrap_or_default();
        let mut out: Settings = Vec::new();
        let mut intrinsic = Vec::new();
        for f in fields {
            let key = f.key.as_str();
            if skip.contains(&key) {
                continue;
            }
            if let Some((_, new)) = SIMPLE.iter().find(|(old, _)| *old == key) {
                out.push(((*new).into(), render(&f.value)));
                continue;
            }
            match key {
                "nty" => match word(&f.value).as_deref() {
                    Some("bi") => out.push(("dynamics".into(), "bistable_integrator".into())),
                    Some("mr") => out.push(("dynamics".into(), "monostable_resonator".into())),
                    Some("br") => out.push(("dynamics".into(), "bistable_resonator".into())),
                    _ => {}
                },
                "inh" => {
                    if matches!(word(&f.value).as_deref(), Some("t" | "true")) {
                        out.push(("inhibitory".into(), "true".into()));
                    }
                }
                "ipb" => intrinsic.push(format!("step={}", render(&f.value))),
                "ipl" => intrinsic.push(format!("amplitude={}", render(&f.value))),
                "ipd" => intrinsic.push(format!("decay_time={}", render(&f.value))),
                "HIP" => out.push((
                    "homeostasis".into(),
                    self.nested(
                        &f.value,
                        &[
                            ("tgt", "target_rate"),
                            ("str", "strength"),
                            ("win", "window"),
                            ("lwb", "min_shift"),
                            ("upb", "max_shift"),
                        ],
                    ),
                )),
                "RES" => out.push((
                    "resonance".into(),
                    self.nested(
                        &f.value,
                        &[
                            ("frq", "frequency"),
                            ("dmp", "damping_time"),
                            ("lcr", "limit_cycle"),
                            ("sep", "separatrix"),
                        ],
                    ),
                )),
                "RCP" => {
                    let Value::List(items) = &f.value.value else { continue };
                    let receptors: Vec<String> = items
                        .iter()
                        .map(|r| {
                            self.nested(
                                r,
                                &[
                                    ("mod", "modulator"),
                                    ("exg", "excitatory_gain"),
                                    ("ing", "inhibitory_gain"),
                                    ("thr", "threshold_gain"),
                                    ("pla", "plasticity_gain"),
                                    ("elg", "eligibility_time"),
                                    ("rat", "learning_rate"),
                                    ("dir", "direct_potential"),
                                ],
                            )
                        })
                        .collect();
                    out.push(("receptors".into(), format!("[{}]", receptors.join(", "))));
                }
                "REL" => out.push((
                    "releases".into(),
                    self.nested(&f.value, &[("mod", "modulator"), ("amt", "amount")]),
                )),
                // Incoming-synapse settings and the outgoing delay: see below.
                "WDF" | "WLF" | "SRP" | "SNT" | "wnm" | "syd" => {}
                _ => self.errors.push(error(f.span, format!("unknown key '{key}'"))),
            }
        }
        if !intrinsic.is_empty() {
            out.push(("intrinsic_plasticity".into(), format!("({})", intrinsic.join(", "))));
        }
        let incoming = self.incoming(fields);
        self.synapses_of.insert(name.clone(), incoming);
        self.delay_of.insert(name, get(fields, "syd").map(render));
        out
    }

    /// What a neuron's `WDF`, `WLF`, `SRP` and `wnm` meant for its incoming synapses, with the old defaults (decay
    /// over 1.5 s, release on) written out.
    fn incoming(&mut self, fields: &[Field]) -> Settings {
        let mut out = Vec::new();
        match get(fields, "WDF").map(|n| &n.value) {
            None => out.push(("decay".into(), "(time=1500 ms)".into())),
            Some(Value::Record(w)) => {
                if get(w, "fnc").and_then(word).as_deref() == Some("exp") {
                    let time = get(w, "dhl").map_or_else(|| "1500 ms".into(), render);
                    out.push(("decay".into(), format!("(time={time})")));
                }
            }
            Some(_) => {}
        }
        if let Some(Value::Record(w)) = get(fields, "WLF").map(|n| &n.value)
            && get(w, "fnc").and_then(word).as_deref() == Some("bnd")
        {
            let lower = get(w, "lwb").map_or_else(|| "0".into(), render);
            let upper = get(w, "upb")
                .or_else(|| get(w, "uwb"))
                .map_or_else(|| "0".into(), render);
            out.push(("limits".into(), format!("(min={lower}, max={upper})")));
        }
        match get(fields, "SRP").or_else(|| get(fields, "SNT")) {
            None => out.push(("release".into(), "()".into())),
            Some(node) => {
                let r = self.nested(
                    node,
                    &[
                        ("fcb", "facilitation_base"),
                        ("fcm", "facilitation_step"),
                        ("fct", "facilitation_time"),
                        ("dpb", "depletion_base"),
                        ("dpm", "depletion_step"),
                        ("dpt", "depletion_time"),
                    ],
                );
                out.push(("release".into(), r));
            }
        }
        if let Some(n) = get(fields, "wnm")
            && !matches!(n.value, Value::Number { value, .. } if value == 0.0)
        {
            out.push(("weight_noise".into(), render(n)));
        }
        out
    }

    fn nested(&mut self, node: &Node, map: &[(&str, &str)]) -> String {
        match &node.value {
            Value::Record(fields) => renamed(fields, map, &mut self.errors),
            _ => {
                self.errors.push(error(node.span, "expected a record"));
                "()".into()
            }
        }
    }

    fn position(&mut self, node: &Node) -> String {
        let Value::Record(fields) = &node.value else {
            self.errors.push(error(node.span, "expected a location record"));
            return "[0, 0, 0]".into();
        };
        let v = |k: &str| get(fields, k).map_or_else(|| "0".into(), render);
        match get(fields, "cst").and_then(word).as_deref() {
            Some("cl") => format!("(r={}, phi={}, z={})", v("px1"), v("px2"), v("px3")),
            Some("sp") => format!("(r={}, phi={}, theta={})", v("px1"), v("px2"), v("px3")),
            _ => format!("[{}, {}, {}]", v("px1"), v("px2"), v("px3")),
        }
    }

    fn population(&mut self, node: &Node) {
        let Value::Record(fields) = &node.value else { return };
        let name = get(fields, "nam").and_then(word).unwrap_or_default();
        self.population_names.push(name.clone());
        let settings = self.neuron_settings(fields, &["nam", "cnt", "REG", "sed", "grp"]);
        let mut parts = vec![format!("name={name}")];
        if !settings.is_empty() {
            parts.push(joined(&settings));
        }
        parts.push(format!(
            "count={}",
            get(fields, "cnt").map_or_else(|| "1".into(), render)
        ));
        if let Some(Value::Record(r)) = get(fields, "REG").map(|n| &n.value) {
            let v = |k: &str| get(r, k).map_or_else(|| "0".into(), render);
            let placement = match get(r, "cst").and_then(word).as_deref() {
                Some("sp") => format!(
                    "random=(ball=(center=[{}, {}, {}], radius={}))",
                    v("px1"),
                    v("px2"),
                    v("px3"),
                    v("rad")
                ),
                _ => format!(
                    "random=(box=(corner=[{}, {}, {}], size=[{}, {}, {}]))",
                    v("px1"),
                    v("px2"),
                    v("px3"),
                    v("sx1"),
                    v("sx2"),
                    v("sx3")
                ),
            };
            parts.push(placement);
        }
        if let Some(seed) = get(fields, "sed") {
            parts.push(format!("seed={}", render(seed)));
        }
        self.populations.push(format!("({})", parts.join(", ")));
    }

    /// The synapse type for these incoming-synapse settings and a connection's learning, stickiness and sign.
    fn synapse_type(&mut self, incoming: &Settings, fields: &[Field]) -> String {
        let mut settings = incoming.clone();
        if let Some(l) = get(fields, "lrn").and_then(word).filter(|l| l != "flat") {
            settings.push(("learning".into(), l));
        }
        if let Some(c) = get(fields, "cst").and_then(word).filter(|c| c != "none") {
            settings.push(("stickiness".into(), c));
        }
        match get(fields, "typ").and_then(word).as_deref() {
            Some("inh") => settings.push(("sign".into(), "inhibitory".into())),
            Some("exc") => settings.push(("sign".into(), "excitatory".into())),
            _ => {}
        }
        if let Some((name, _)) = self.synapse_types.iter().find(|(_, s)| *s == settings) {
            return name.clone();
        }
        // Named after its learning rule ('fixed' without one).
        let base = settings
            .iter()
            .find(|(k, _)| k == "learning")
            .map_or_else(|| "fixed".to_string(), |(_, v)| v.clone());
        let mut name = base.clone();
        let mut k = 2;
        while self.synapse_types.iter().any(|(n, _)| *n == name) {
            name = format!("{base}_{k}");
            k += 1;
        }
        self.synapse_types.push((name.clone(), settings));
        name
    }

    /// The weight, equilibrium and delay keys of a connection.
    fn weights(fields: &[Field], delay: Option<&String>) -> String {
        let mut parts = Vec::new();
        let weight = get(fields, "cnw").map(render);
        if let Some(w) = &weight {
            parts.push(format!("weight={w}"));
        }
        if let Some(e) = get(fields, "eqw").map(render)
            && Some(&e) != weight.as_ref()
        {
            parts.push(format!("equilibrium={e}"));
        }
        if let Some(d) = delay {
            parts.push(format!("delay={d}"));
        }
        parts.join(", ")
    }

    fn connection(&mut self, node: &Node) {
        let Value::Record(fields) = &node.value else { return };
        let ids = |k: &str| match get(fields, k).map(|n| &n.value) {
            Some(Value::Ids(w)) => w.clone(),
            _ => Vec::new(),
        };
        let (pres, posts) = (ids("prn"), ids("psn"));
        let own_delay = get(fields, "syd").map(render);
        // Receivers by their incoming-synapse settings, senders by their delay (connections split where they differ).
        let mut by_incoming: Vec<(Settings, Vec<String>)> = Vec::new();
        for post in &posts {
            let s = self.synapses_of.get(post).cloned().unwrap_or_default();
            match by_incoming.iter_mut().find(|(k, _)| *k == s) {
                Some((_, v)) => v.push(post.clone()),
                None => by_incoming.push((s, vec![post.clone()])),
            }
        }
        let mut by_delay: Vec<(Option<String>, Vec<String>)> = Vec::new();
        for pre in &pres {
            let d = own_delay.clone().or_else(|| self.delay_of.get(pre).cloned().flatten());
            match by_delay.iter_mut().find(|(k, _)| *k == d) {
                Some((_, v)) => v.push(pre.clone()),
                None => by_delay.push((d, vec![pre.clone()])),
            }
        }
        for (incoming, receivers) in &by_incoming {
            let t = self.synapse_type(incoming, fields);
            for (delay, senders) in &by_delay {
                // The older form connected a neuron to itself when it was among both senders and receivers.
                let selfish = senders.iter().any(|s| receivers.contains(s));
                let mut parts = vec![
                    format!("from={}", names(senders)),
                    format!("to={}", names(receivers)),
                    format!("type={t}"),
                ];
                let w = Self::weights(fields, delay.as_ref());
                if !w.is_empty() {
                    parts.push(w);
                }
                if selfish {
                    parts.push("self=true".into());
                }
                self.connections.push(format!("({})", parts.join(", ")));
            }
        }
    }

    /// The neurons a rule's `prp`/`psp` names: a population, or a group (which becomes a set).
    fn rule_members(&mut self, name: &str) -> Vec<String> {
        if self.population_names.iter().any(|p| p == name) {
            return vec![name.to_string()];
        }
        match self.groups.get(name).cloned() {
            Some(members) => {
                if !self.sets.iter().any(|s| s.starts_with(&format!("(name={name},"))) {
                    self.sets.push(format!("(name={name}, members={})", names(&members)));
                }
                members
            }
            None => vec![name.to_string()],
        }
    }

    fn rule(&mut self, node: &Node) {
        let Value::Record(fields) = &node.value else { return };
        let pre = get(fields, "prp").and_then(word).unwrap_or_default();
        let post = get(fields, "psp").and_then(word).unwrap_or_default();
        self.rule_members(&pre);
        let receivers = self.rule_members(&post);
        let mut pattern = Vec::new();
        if let Some(c) = get(fields, "cnt") {
            pattern.push(format!("pattern=fan_in, count={}", render(c)));
        } else if let Some(p) = get(fields, "prb") {
            pattern.push(format!("pattern=random, probability={}", render(p)));
        }
        if let Some(r) = get(fields, "rad") {
            pattern.push(format!("within={}", render(r)));
        }
        if matches!(get(fields, "aut").and_then(word).as_deref(), Some("t" | "true")) {
            pattern.push("self=true".into());
        }
        // Receivers' incoming-synapse settings: a population's, or (for a group) each member's.
        let incoming = if self.population_names.contains(&post) {
            self.synapses_of.get(&post).cloned().unwrap_or_default()
        } else {
            let mut kinds: Vec<Settings> = receivers
                .iter()
                .filter_map(|r| self.synapses_of.get(r).cloned())
                .collect();
            kinds.dedup();
            if kinds.len() > 1 {
                self.errors.push(error(
                    node.span,
                    format!("the neurons of group '{post}' give their incoming synapses different settings; split the rule by hand"),
                ));
            }
            kinds.into_iter().next().unwrap_or_default()
        };
        let t = self.synapse_type(&incoming, fields);
        let delay = get(fields, "syd").map(render);
        let mut parts = vec![format!("from={pre}"), format!("to={post}")];
        parts.extend(pattern);
        parts.push(format!("type={t}"));
        let w = Self::weights(fields, delay.as_ref());
        if !w.is_empty() {
            parts.push(w);
        }
        self.connections.push(format!("({})", parts.join(", ")));
    }

    fn learning_rule(&mut self, node: &Node) {
        let Value::Record(fields) = &node.value else { return };
        let function = get(fields, "fnc").and_then(word).unwrap_or_default();
        if function == "flat" {
            return;
        }
        let name = get(fields, "nam").and_then(word).unwrap_or_else(|| function.clone());
        let rule = if function == "stdp" {
            "stdp_hard".to_string()
        } else {
            function
        };
        let mut parts = vec![format!("name={name}"), format!("rule={rule}")];
        for f in fields {
            let new = match f.key.as_str() {
                "fnc" | "nam" => continue,
                "ina" => "depression",
                "inp" => "depression_time",
                "exa" => "potentiation",
                "exp" => "potentiation_time",
                "bln" => "baseline",
                "atc" => "time",
                "alr" => "rate",
                other => {
                    self.errors.push(error(f.span, format!("unknown key '{other}'")));
                    continue;
                }
            };
            parts.push(format!("{new}={}", render(&f.value)));
        }
        self.learning.push(format!("({})", parts.join(", ")));
    }

    fn stickiness_fn(&mut self, node: &Node) {
        let Value::Record(fields) = &node.value else { return };
        if get(fields, "fnc").and_then(word).as_deref() != Some("sig") {
            return;
        }
        let name = get(fields, "nam").and_then(word).unwrap_or_else(|| "sig".into());
        let time = get(fields, "dhl").map_or_else(|| "0".into(), render);
        self.stickiness.push(format!("(name={name}, time={time})"));
    }

    fn modulator(&mut self, node: &Node) {
        let Value::Record(fields) = &node.value else { return };
        let s = renamed(
            fields,
            &[
                ("nam", "name"),
                ("clr", "clearance_time"),
                ("rad", "reach"),
                ("spd", "speed"),
                ("lat", "latency"),
                ("len", "falloff"),
            ],
            &mut self.errors,
        );
        self.modulators.push(s);
    }

    fn output(&self) -> String {
        let mut out = String::from("(\n");
        let mut section = |key: &str, items: &[String]| {
            if items.is_empty() {
                return;
            }
            let _ = writeln!(out, "{key}=[");
            for item in items {
                let _ = writeln!(out, "    {item},");
            }
            out.push_str("],\n");
        };
        let types = |types: &[(String, Settings)]| -> Vec<String> {
            types
                .iter()
                .map(|(n, s)| format!("(name={n}, {})", joined(s)))
                .collect()
        };
        let synapse_types: Vec<String> = self
            .synapse_types
            .iter()
            .map(|(n, s)| {
                if s.is_empty() {
                    format!("(name={n})")
                } else {
                    format!("(name={n}, {})", joined(s))
                }
            })
            .collect();
        section("neuron_types", &types(&self.neuron_types));
        section("synapse_types", &synapse_types);
        section("neurons", &self.neurons);
        section("populations", &self.populations);
        section("sets", &self.sets);
        section("connections", &self.connections);
        section("learning_rules", &self.learning);
        section("stickiness", &self.stickiness);
        section("modulators", &self.modulators);
        out.push_str(")\n");
        out
    }
}

/// A name for a neuron type, from what its members' names share (`hidden-101`, `hidden-102` → `hidden`).
fn type_name(members: &[String]) -> String {
    let first = &members[0];
    let mut len = first.len();
    for m in &members[1..] {
        len = len.min(first.bytes().zip(m.bytes()).take_while(|(a, b)| a == b).count());
    }
    let prefix = first[..len].trim_end_matches(|c: char| c.is_ascii_digit() || c == '-' || c == '_');
    if prefix.is_empty() || !prefix.starts_with(|c: char| c.is_ascii_alphabetic()) {
        "neuron".into()
    } else {
        prefix.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_compress_into_ranges() {
        let w = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(names(&w(&["a"])), "a");
        assert_eq!(names(&w(&["in-1", "in-2", "in-3"])), "in-{1:3:1}");
        assert_eq!(names(&w(&["in-1", "in-2"])), "[in-1, in-2]");
        assert_eq!(names(&w(&["h-101", "h-103", "h-105", "x"])), "[h-{101:105:2}, x]");
        assert_eq!(type_name(&w(&["hidden-101", "hidden-205"])), "hidden");
    }

    #[test]
    fn a_small_network_migrates_and_parses() {
        let old = "// two neurons\n(
NRN=[
    (nid=a, mst=5 mV, LOC=(cst=ct, px1=0, px2=0, px3=0)),
    (nid=b, mst=5 mV, WDF=(fnc=zer), WLF=(fnc=bnd, lwb=0, upb=5), LOC=(cst=cl, px1=40, px2=0, px3=10)),
],
CON=[(prn=a, psn=b, cnw=r0:2, lrn=stdp)],
LRN=[(fnc=stdp, ina=0.01, inp=3, exa=0.01, exp=3)],
)";
        let new = migrate(old).unwrap();
        assert!(new.starts_with("// two neurons\n("), "{new}");
        assert!(new.contains("limits=(min=0, max=5)"), "{new}");
        assert!(new.contains("weight=uniform(0, 2)"), "{new}");
        assert!(new.contains("rule=stdp_hard"), "{new}");
        let d = crate::parse(&new).unwrap_or_else(|e| panic!("{e:?}\n{new}"));
        assert_eq!(d.neurons.len(), 2);
        assert_eq!(d.connections.len(), 1);
        let s = &d.connections[0].synapse;
        assert_eq!(s.decay, spikes_model::WeightDecay::None);
        assert!(s.release.is_some(), "the old default: release on");
        assert_eq!(s.learning, "stdp");
    }
}
