//! Building a runtime network from a [`NetworkDescription`].

use std::collections::HashMap;
use std::sync::Arc;

use rand::{RngExt, SeedableRng};
use rand_xoshiro::Xoshiro256PlusPlus;
use spikes_dna::{NetworkDescription, Pattern, SynapseSpec, WeightSpec};
use spikes_model::{NeuronParams, Receptor, Sign, Synapse, SynapseParams};
use tempo_runtime::placement::spatial_partitions_weighted;
use tempo_runtime::{ActorId, EdgeIdx, Jitter, RouteEdge, RouteId, SimDuration};

use crate::Runtime;
use crate::messages::{ModulatorRef, NeuronRef};
use crate::modulators::{ModulatorField, ModulatorKind, ModulatorRelease};
use crate::neuron::{NetworkParams, NeuronActor};

/// How to build a network.
#[derive(Clone, Debug)]
pub struct BuildOptions {
    /// Seed for random initial weights (`weight=uniform(0, 1)`) and random connection patterns.
    pub seed: u64,
    /// The shortest allowed connection delay. Neurons closer than `min_delay × speed` get this delay; it also keeps
    /// the lookahead (and so parallelism) from collapsing when neurons sit very close together.
    pub min_delay: SimDuration,
    /// Intentional timing noise on every connection.
    pub jitter: Jitter,
    /// Report every weight change from learning as an event.
    pub record_weights: bool,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            seed: 0,
            min_delay: SimDuration::ZERO,
            jitter: Jitter::None,
            record_weights: true,
        }
    }
}

/// A connection to be made during a run.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct NewConnection {
    /// Synaptic delay plus conduction time (at least the minimum delay).
    pub delay: SimDuration,
    /// The sending neuron's default sign.
    pub sign: Sign,
    /// The synapse parameter set.
    pub params: u16,
}

/// What a kind of neuron (a population, or a neuron listed on its own) gives the connections it makes.
#[derive(Clone, Debug)]
struct Wire {
    conduction_m_per_s: f64,
    inhibitory: bool,
}

#[derive(Clone, Debug)]
struct Wiring {
    kinds: Vec<Wire>,
    /// Each neuron's kind and position.
    neurons: Vec<(u32, [f64; 3])>,
    /// Learning rule name → the parameter set of a plain synapse with it (for connections made during a run).
    params: HashMap<String, u16>,
    min_delay: SimDuration,
}

/// A built network: its neurons, findable by the names in the description.
#[derive(Clone, Debug)]
pub struct Network {
    /// Neurons in description order.
    pub neurons: Vec<NeuronRef>,
    /// The shared parameter tables.
    pub params: Arc<NetworkParams>,
    /// Each neuron's outgoing route (every neuron has one, possibly empty, so connections can be added during a
    /// run).
    pub routes: Vec<RouteId>,
    /// Connections in all (on every node, not only this one's).
    pub synapse_count: u64,
    /// What new connections need: positions, delays, signs and synapse parameter sets.
    wiring: Arc<Wiring>,
    names: Arc<spikes_dna::NeuronNames>,
    /// (actor ID, neuron index), by ID.
    indices: Arc<Vec<(ActorId, u32)>>,
}

impl Network {
    /// The neuron with this name.
    pub fn neuron(&self, name: &str) -> Option<NeuronRef> {
        self.neurons.get(self.names.index(name)? as usize).copied()
    }

    /// The index (in description order) of the neuron with this ID.
    pub fn index(&self, id: ActorId) -> Option<u32> {
        let i = self.indices.binary_search_by_key(&id, |&(id, _)| id).ok()?;
        Some(self.indices[i].1)
    }

    /// The name of the neuron with this ID.
    pub fn name(&self, id: ActorId) -> Option<std::borrow::Cow<'_, str>> {
        self.names.name(self.index(id)?)
    }

    /// Every neuron's name, by index.
    pub fn names(&self) -> &Arc<spikes_dna::NeuronNames> {
        &self.names
    }

    /// The name of neuron `index`.
    pub fn name_of(&self, index: u32) -> Option<std::borrow::Cow<'_, str>> {
        self.names.name(index)
    }

    /// Adds a modulator release site at `position` (µm), e.g. for a UI "reward" button. Send it
    /// `ModulatorMsg::Release` to release a modulator there.
    pub fn add_release_site(&self, rt: &mut Runtime, position: [f64; 3]) -> ModulatorRef {
        let id = rt.spawn(ModulatorRelease {
            position,
            kinds: Arc::clone(&self.params.modulators),
        });
        ModulatorRef::new(id)
    }

    /// The index of the modulator with this name (as used in `ModulatorMsg::Release`).
    pub fn modulator(&self, name: &str) -> Option<u16> {
        self.params
            .modulators
            .iter()
            .position(|m| m.name == name)
            .map(|i| i as u16)
    }

    /// What a new connection from `pre` to `post` (description indices) would be: its delay (the default synaptic
    /// delay plus conduction), its default sign, and the parameter set of a plain synapse (no decay, limits or
    /// release) with `learning` (a rule defined in the description; `None` for `flat`).
    pub fn connection(&self, pre: usize, post: usize, learning: Option<&str>) -> Option<NewConnection> {
        let w = &self.wiring;
        let (&(from_kind, from_at), &(to_kind, to_at)) = (w.neurons.get(pre)?, w.neurons.get(post)?);
        let from = &w.kinds[from_kind as usize];
        let _ = to_kind;
        let distance_um = from_at
            .iter()
            .zip(&to_at)
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f64>()
            .sqrt();
        let delay = (SimDuration::from_millis_f64(SynapseSpec::DEFAULT_DELAY_MS)
            + SimDuration::from_micros_f64(distance_um / from.conduction_m_per_s))
        .max(w.min_delay);
        let params = *w.params.get(learning.unwrap_or("flat"))?;
        Some(NewConnection {
            delay,
            sign: if from.inhibitory {
                Sign::Inhibitory
            } else {
                Sign::Excitatory
            },
            params,
        })
    }

    /// The learning rules new connections can use.
    pub fn learning_rules(&self) -> Vec<String> {
        let mut names: Vec<String> = self.wiring.params.keys().cloned().collect();
        names.sort();
        names
    }

    /// The neurons whose names satisfy `matches`, in description order.
    pub fn matching(&self, matches: impl Fn(&str) -> bool) -> Vec<NeuronRef> {
        self.neurons
            .iter()
            .zip(0u32..)
            .filter(|&(_, i)| self.names.name(i).is_some_and(|name| matches(&name)))
            .map(|(&n, _)| n)
            .collect()
    }
}

/// Deduplicates parameter sets (which hold floats, so they can't be hashed directly) by their debug form.
struct Table<T> {
    items: Vec<T>,
    index: HashMap<String, u32>,
}

impl<T: std::fmt::Debug> Table<T> {
    fn new() -> Self {
        Self {
            items: Vec::new(),
            index: HashMap::new(),
        }
    }

    fn insert(&mut self, item: T) -> u32 {
        let key = format!("{item:?}");
        if let Some(&i) = self.index.get(&key) {
            return i;
        }
        let i = u32::try_from(self.items.len()).expect("parameter table fits u32");
        self.items.push(item);
        self.index.insert(key, i);
        i
    }
}

/// A population: dynamics, receptors, and modulator release on spiking.
type Population = (NeuronParams, Vec<Receptor>, Option<(u16, f64)>);

/// Each neuron's expected work, for placement: 1, plus its expected incoming connections (a connection with
/// `within` is counted as if the radius took in every sender).
fn expected_work(description: &NetworkDescription) -> Vec<u64> {
    let mut work = vec![1u64; description.neurons.len()];
    for c in &description.connections {
        let senders = c.from.len() as u64;
        let each = match c.pattern {
            Pattern::AllToAll => senders,
            Pattern::OneToOne => 1,
            Pattern::FanIn(k) => u64::from(k).min(senders),
            Pattern::Random(p) => (p * senders as f64).round() as u64,
        };
        for &post in &c.to {
            work[post as usize] += each;
        }
    }
    work
}

fn draw(spec: WeightSpec, rng: &mut Xoshiro256PlusPlus) -> f64 {
    match spec {
        WeightSpec::Fixed(w) => w,
        WeightSpec::Uniform { low, high } => low + (high - low) * rng.random::<f64>(),
        WeightSpec::Normal { mean, sd } => {
            // Box–Muller.
            let u1: f64 = 1.0 - rng.random::<f64>();
            let u2: f64 = rng.random();
            mean + sd * (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
        }
    }
}

/// A seed for one connection's synapses onto one receiving neuron.
fn mix(seed: u64, connection: u64, post: u64) -> u64 {
    let mut z = seed ^ connection.rotate_left(40) ^ post.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// How many senders a receiver picks at random.
#[derive(Copy, Clone)]
enum Pick {
    /// This many different ones.
    Count(u32),
    /// Each with this probability.
    Probability(f64),
}

/// Picks senders from `pool` (ascending neuron indices) for receiver `post`: `k` different ones at random, or each
/// with probability `p`; `post` itself only with autapses. The result is ascending.
fn choose(pool: &[u32], post: u32, autapses: bool, pick: Pick, rng: &mut Xoshiro256PlusPlus, out: &mut Vec<u32>) {
    out.clear();
    // Skip the receiver by index arithmetic, without copying the pool.
    let own = if autapses { None } else { pool.binary_search(&post).ok() };
    let len = pool.len() - usize::from(own.is_some());
    let at = |i: usize| match own {
        Some(o) if i >= o => pool[i + 1],
        _ => pool[i],
    };
    match pick {
        Pick::Count(k) if k as usize >= len => out.extend((0..len).map(at)),
        Pick::Count(k) => {
            // Floyd's algorithm: k distinct indices in O(k).
            let mut picked = std::collections::HashSet::with_capacity(k as usize);
            for j in len - k as usize..len {
                let t = rng.random_range(0..=j);
                if !picked.insert(t) {
                    picked.insert(j);
                }
            }
            out.extend(picked.into_iter().map(at));
            out.sort_unstable();
        }
        Pick::Probability(p) if p >= 1.0 => out.extend((0..len).map(at)),
        Pick::Probability(p) => {
            // Geometric skips: O(chosen), not O(pool).
            let log_q = (1.0 - p).ln();
            let mut i = 0usize;
            loop {
                let u: f64 = 1.0 - rng.random::<f64>(); // (0, 1]
                i += (u.ln() / log_q).floor() as usize;
                if i >= len {
                    break;
                }
                out.push(at(i));
                i += 1;
            }
        }
    }
}

/// The model's synapse parameters for a synapse spec.
fn synapse_params(spec: &SynapseSpec, description: &NetworkDescription) -> SynapseParams {
    SynapseParams {
        decay: spec.decay,
        limit: spec.limit,
        stickiness: description.stickiness[&spec.stickiness],
        release: spec.release,
        learning: description.learning[&spec.learning].clone(),
        weight_noise: spec.weight_noise,
    }
}

/// A uniform grid over some neurons' positions, for finding those within a radius.
struct Grid {
    cell: f64,
    cells: HashMap<[i64; 3], Vec<u32>>,
    radius: f64,
}

impl Grid {
    fn new(members: &[u32], locations: &[[f64; 3]], radius: f64) -> Self {
        let mut cells: HashMap<[i64; 3], Vec<u32>> = HashMap::new();
        for &m in members {
            cells
                .entry(Self::key(locations[m as usize], radius))
                .or_default()
                .push(m);
        }
        Self {
            cell: radius,
            cells,
            radius,
        }
    }

    fn key(p: [f64; 3], cell: f64) -> [i64; 3] {
        p.map(|v| (v / cell).floor() as i64)
    }

    /// Appends the members within the radius of `p`, ascending.
    fn within(&self, p: [f64; 3], locations: &[[f64; 3]], out: &mut Vec<u32>) {
        let c = Self::key(p, self.cell);
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if let Some(list) = self.cells.get(&[c[0] + dx, c[1] + dy, c[2] + dz]) {
                        out.extend(list.iter().copied().filter(|&m| {
                            let q = locations[m as usize];
                            (0..3).map(|d| (q[d] - p[d]).powi(2)).sum::<f64>() <= self.radius * self.radius
                        }));
                    }
                }
            }
        }
        out.sort_unstable();
    }
}

/// Builds `description` into `rt`: one actor per neuron, placed by location; one route per neuron with
/// connection delays `distance / conduction speed` (at least `min_delay`); synapses on the receiving neurons.
///
/// # Panics
/// If the description refers to undefined neurons, learning functions or stickiness functions (a description
/// from [`spikes_dna::parse`] never does).
pub fn build(rt: &mut Runtime, description: &NetworkDescription, options: &BuildOptions) -> Network {
    let n = description.neurons.len();
    let names = description.neurons.names();

    // Parameter tables, shared by all neurons. A population is a distinct combination of dynamics, receptors and
    // modulator release. Each neuron of a description population shares its template: tables are filled once per
    // template (by address), not once per neuron.
    let mut population_table: Table<Population> = Table::new();
    let mut synapse_table: Table<SynapseParams> = Table::new();
    let mut kinds: Vec<Wire> = Vec::new();
    // Per template: (population, kind).
    let mut seen: HashMap<*const spikes_dna::NeuronDescription, (u32, u32)> = HashMap::new();
    let mut populations: Vec<u32> = Vec::with_capacity(n);
    let mut neuron_kinds: Vec<u32> = Vec::with_capacity(n);
    for d in description.neurons.iter() {
        let &mut (population, kind) = seen.entry(std::ptr::from_ref(d.template())).or_insert_with(|| {
            kinds.push(Wire {
                conduction_m_per_s: d.conduction_m_per_s(),
                inhibitory: d.inhibitory(),
            });
            (
                population_table.insert((d.params().clone(), d.receptors().to_vec(), d.release())),
                kinds.len() as u32 - 1,
            )
        });
        populations.push(population);
        neuron_kinds.push(kind);
    }
    let mut table = |params: SynapseParams| {
        u16::try_from(synapse_table.insert(params)).expect("fewer than 65,536 distinct synapse parameter sets")
    };
    // Each connection's synapse parameters; and a plain synapse with each learning rule, for connections made
    // during a run.
    let connection_params: Vec<u16> = description
        .connections
        .iter()
        .map(|c| table(synapse_params(&c.synapse, description)))
        .collect();
    let new_params: HashMap<String, u16> = description
        .learning
        .keys()
        .map(|name| {
            let mut plain = SynapseSpec::plain(1.0);
            plain.learning.clone_from(name);
            (name.clone(), table(synapse_params(&plain, description)))
        })
        .collect();
    let direct: Vec<bool> = (0..description.modulators.len())
        .map(|k| {
            population_table
                .items
                .iter()
                .flat_map(|(_, receptors, _)| receptors)
                .any(|r| usize::from(r.modulator) == k && r.direct_mv != 0.0)
        })
        .collect();
    let modulators: Arc<Vec<ModulatorKind>> = Arc::new(
        description
            .modulators
            .iter()
            .zip(&direct)
            .map(|(m, &direct)| ModulatorKind::from_description(m, direct))
            .collect(),
    );
    if !modulators.is_empty() {
        rt.add_field(ModulatorField::new(Arc::clone(&modulators)));
    }
    let (neurons, receptors, releases) = population_table.items.into_iter().fold(
        (Vec::new(), Vec::new(), Vec::new()),
        |(mut n, mut r, mut l), (neuron, receptors, release)| {
            n.push(neuron);
            r.push(receptors);
            l.push(release);
            (n, r, l)
        },
    );
    let params = Arc::new(NetworkParams {
        neurons,
        receptors,
        releases,
        modulators,
        synapses: synapse_table.items,
        record_weights: options.record_weights,
    });

    // Neurons, placed so that nearby neurons share a partition, and partitions get equal work: a neuron's work is
    // mostly its incoming connections.
    let locations: Vec<[f64; 3]> = description.neurons.iter().map(|d| d.location_um()).collect();
    let partitions = spatial_partitions_weighted(&locations, &expected_work(description), rt.partitions());
    let ids: Vec<ActorId> = (0..n)
        .map(|i| {
            let actor = NeuronActor::new(Arc::clone(&params), populations[i], locations[i]);
            let id = rt.spawn_in(partitions[i], actor);
            rt.set_position(id, locations[i]);
            id
        })
        .collect();

    // Connections: synapses on the receiver, route edges on the sender. On a node of a cluster only this node's
    // neurons are stored, so only their synapses and routes are kept; but every connection is numbered on its
    // receiver (its edge index) the same way on every node.
    let local: Vec<bool> = ids.iter().map(|&id| rt.is_local(id)).collect();
    let mut synapses: Vec<Vec<Synapse>> = vec![Vec::new(); n];
    let mut pre: Vec<Vec<ActorId>> = vec![Vec::new(); n];
    let mut edges: Vec<Vec<RouteEdge>> = vec![Vec::new(); n];
    let mut incoming: Vec<u32> = vec![0; n];
    let distance = |a: usize, b: usize| {
        locations[a]
            .iter()
            .zip(&locations[b])
            .map(|(x, y)| (x - y).powi(2))
            .sum::<f64>()
            .sqrt()
    };
    let mut add = |from: usize, to: usize, synapse: Synapse, synaptic_delay_ms: f64| {
        let edge = EdgeIdx(incoming[to]);
        incoming[to] = incoming[to].checked_add(1).expect("fewer than 2³² synapses per neuron");
        if local[to] {
            synapses[to].push(synapse);
            pre[to].push(ids[from]);
        }
        if local[from] {
            // Synaptic delay plus conduction along the axon.
            let neuron = &kinds[neuron_kinds[from] as usize];
            let synaptic = SimDuration::from_millis_f64(synaptic_delay_ms);
            let conduction = SimDuration::from_micros_f64(distance(from, to) / neuron.conduction_m_per_s); // µm/µs
            edges[from].push(RouteEdge {
                target: ids[to],
                delay: (synaptic + conduction).max(options.min_delay),
                edge,
            });
        }
    };

    // Each connection receiving neuron by receiving neuron, each with its own random stream (for weights and
    // random patterns): the same connections on every node, whichever neurons it keeps.
    let mut chosen: Vec<u32> = Vec::new();
    let mut candidates: Vec<u32> = Vec::new();
    for (c, (connection, &params)) in description.connections.iter().zip(&connection_params).enumerate() {
        let spec = &connection.synapse;
        // Random patterns pick from an ascending pool; the others take the senders in the order written.
        let mut pool = connection.from.clone();
        if matches!(connection.pattern, Pattern::FanIn(_) | Pattern::Random(_)) {
            pool.sort_unstable();
            pool.dedup();
        }
        let grid = match (connection.pattern, connection.within_um) {
            (Pattern::FanIn(_) | Pattern::Random(_), Some(radius)) => Some(Grid::new(&pool, &locations, radius)),
            _ => None,
        };
        for (k, &post) in connection.to.iter().enumerate() {
            let to = post as usize;
            let mut rng = Xoshiro256PlusPlus::seed_from_u64(mix(options.seed, c as u64, u64::from(post)));
            chosen.clear();
            match connection.pattern {
                Pattern::AllToAll => chosen.extend(connection.from.iter().copied().filter(|&f| {
                    (connection.autapses || f != post)
                        && connection.within_um.is_none_or(|r| distance(f as usize, to) <= r)
                })),
                Pattern::OneToOne => {
                    let f = connection.from[k];
                    if (connection.autapses || f != post)
                        && connection.within_um.is_none_or(|r| distance(f as usize, to) <= r)
                    {
                        chosen.push(f);
                    }
                }
                Pattern::FanIn(_) | Pattern::Random(_) => {
                    let pick = match connection.pattern {
                        Pattern::FanIn(count) => Pick::Count(count),
                        Pattern::Random(p) => Pick::Probability(p),
                        _ => unreachable!(),
                    };
                    let from: &[u32] = match &grid {
                        Some(grid) => {
                            candidates.clear();
                            grid.within(locations[to], &locations, &mut candidates);
                            &candidates
                        }
                        None => &pool,
                    };
                    let mut picked = Vec::new();
                    choose(from, post, connection.autapses, pick, &mut rng, &mut picked);
                    chosen.extend(picked);
                }
            }
            for &from in &chosen {
                let from = from as usize;
                let weight = draw(spec.weight, &mut rng);
                let equilibrium = match spec.equilibrium {
                    None => weight,
                    Some(e) => draw(e, &mut rng),
                };
                let sign = spec.sign.unwrap_or(if kinds[neuron_kinds[from] as usize].inhibitory {
                    Sign::Inhibitory
                } else {
                    Sign::Excitatory
                });
                add(from, to, Synapse::new(weight, equilibrium, sign, params), spec.delay_ms);
            }
        }
    }
    let synapse_count: u64 = incoming.iter().map(|&k| u64::from(k)).sum();
    let mut routes = Vec::with_capacity(n);
    for (i, ((synapses, pre), edges)) in synapses.into_iter().zip(pre).zip(edges).enumerate() {
        let connected = !edges.is_empty();
        let route = rt.add_route(ids[i], edges, options.jitter);
        routes.push(route);
        // On a node of a cluster, only this node's neurons are stored here.
        let Some(actor) = rt.actor_mut::<NeuronActor>(ids[i]) else {
            continue;
        };
        // Built by pushing, so up to half their capacity is spare: trim it (it's most of the network's memory).
        let (mut synapses, mut pre) = (synapses, pre);
        synapses.shrink_to_fit();
        pre.shrink_to_fit();
        actor.synapses = synapses;
        actor.pre = pre;
        // A neuron without outgoing connections doesn't look its (empty) route up at every spike; it's switched on
        // when it gains a connection.
        actor.out = connected.then_some(route);
    }
    let wiring = Wiring {
        kinds,
        neurons: neuron_kinds.into_iter().zip(locations).collect(),
        params: new_params,
        min_delay: options.min_delay,
    };

    let neurons: Vec<NeuronRef> = ids.iter().map(|&id| NeuronRef::new(id)).collect();
    let mut indices: Vec<(ActorId, u32)> = ids.iter().copied().zip(0u32..).collect();
    indices.sort_unstable();
    Network {
        params,
        routes,
        synapse_count,
        wiring: Arc::new(wiring),
        names: Arc::new(names),
        indices: Arc::new(indices),
        neurons,
    }
}
