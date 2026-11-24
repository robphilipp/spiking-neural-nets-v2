//! Building a runtime network from a [`NetworkDescription`].

use std::collections::HashMap;
use std::sync::Arc;

use rand::{RngExt, SeedableRng};
use rand_xoshiro::Xoshiro256PlusPlus;
use spikes_dna::{InDegree, NetworkDescription, WeightSpec};
use spikes_model::{NeuronParams, Receptor, Sign, Synapse, SynapseParams};
use tempo_runtime::placement::spatial_partitions;
use tempo_runtime::{ActorId, EdgeIdx, Jitter, RouteEdge, RouteId, SimDuration};

use crate::Runtime;
use crate::messages::{ModulatorRef, NeuronRef};
use crate::modulators::{ModulatorField, ModulatorKind, ModulatorRelease};
use crate::neuron::{NetworkParams, NeuronActor};

/// How to build a network.
#[derive(Clone, Debug)]
pub struct BuildOptions {
    /// Seed for random initial weights (`cnw=r0:1`).
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

#[derive(Clone, Debug)]
struct Wire {
    location: [f64; 3],
    synaptic_delay_ms: f64,
    conduction_m_per_s: f64,
    inhibitory: bool,
    /// Which kind of synapse defaults the neuron gives its incoming synapses.
    defaults: u32,
}

#[derive(Clone, Debug)]
struct Wiring {
    neurons: Vec<Wire>,
    /// (synapse defaults kind, learning rule name) → parameter set.
    params: HashMap<(u32, String), u16>,
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
    wiring: Wiring,
    by_name: HashMap<String, NeuronRef>,
    names: HashMap<ActorId, String>,
}

impl Network {
    /// The neuron with this name.
    pub fn neuron(&self, name: &str) -> Option<NeuronRef> {
        self.by_name.get(name).copied()
    }

    /// The name of the neuron with this ID.
    pub fn name(&self, id: ActorId) -> Option<&str> {
        self.names.get(&id).map(String::as_str)
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

    /// What a new connection from `pre` to `post` (description indices) would be: its delay, its default sign, and
    /// its synapse parameter set for `learning` (a rule defined in the description; `None` for `flat`).
    pub fn connection(&self, pre: usize, post: usize, learning: Option<&str>) -> Option<NewConnection> {
        let w = &self.wiring;
        let (from, to) = (w.neurons.get(pre)?, w.neurons.get(post)?);
        let distance_um = from
            .location
            .iter()
            .zip(&to.location)
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f64>()
            .sqrt();
        let delay = (SimDuration::from_millis_f64(from.synaptic_delay_ms)
            + SimDuration::from_micros_f64(distance_um / from.conduction_m_per_s))
        .max(w.min_delay);
        let params = *w.params.get(&(to.defaults, learning.unwrap_or("flat").to_string()))?;
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
        let mut names: Vec<String> = self.wiring.params.keys().map(|(_, l)| l.clone()).collect();
        names.sort();
        names.dedup();
        names
    }

    /// The neurons whose names satisfy `matches`, in description order.
    pub fn matching(&self, matches: impl Fn(&str) -> bool) -> Vec<NeuronRef> {
        self.neurons
            .iter()
            .copied()
            .filter(|n| self.names.get(&n.id()).is_some_and(|name| matches(name)))
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

fn draw(spec: WeightSpec, rng: &mut Xoshiro256PlusPlus) -> f64 {
    match spec {
        WeightSpec::Fixed(w) => w,
        WeightSpec::Random { low, high } => low + (high - low) * rng.random::<f64>(),
    }
}

/// A seed for one rule's connections to one receiving neuron.
fn mix(seed: u64, rule: u64, post: u64) -> u64 {
    let mut z = seed ^ rule.rotate_left(40) ^ post.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Picks senders from `pool` (ascending neuron indices) for receiver `post`: `k` different ones at random, or each
/// with probability `p`; `post` itself only with autapses. The result is ascending.
fn choose(pool: &[u32], post: u32, autapses: bool, degree: InDegree, rng: &mut Xoshiro256PlusPlus, out: &mut Vec<u32>) {
    out.clear();
    // Skip the receiver by index arithmetic, without copying the pool.
    let own = if autapses { None } else { pool.binary_search(&post).ok() };
    let len = pool.len() - usize::from(own.is_some());
    let at = |i: usize| match own {
        Some(o) if i >= o => pool[i + 1],
        _ => pool[i],
    };
    match degree {
        InDegree::Fixed(k) if k as usize >= len => out.extend((0..len).map(at)),
        InDegree::Fixed(k) => {
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
        InDegree::Probability(p) if p >= 1.0 => out.extend((0..len).map(at)),
        InDegree::Probability(p) => {
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
    let index: HashMap<&str, usize> = description
        .neurons
        .iter()
        .enumerate()
        .map(|(i, d)| (d.id.as_str(), i))
        .collect();

    // Parameter tables, shared by all neurons. A population is a distinct combination of dynamics, receptors and
    // modulator release.
    let mut population_table: Table<Population> = Table::new();
    let populations: Vec<u32> = description
        .neurons
        .iter()
        .map(|d| population_table.insert((d.params.clone(), d.receptors.clone(), d.release)))
        .collect();
    let mut synapse_table: Table<SynapseParams> = Table::new();
    // A parameter set for each kind of receiving neuron and learning rule, for connections made during a run.
    let mut defaults_table: Table<spikes_dna::SynapseDefaults> = Table::new();
    let defaults: Vec<u32> = description
        .neurons
        .iter()
        .map(|d| defaults_table.insert(d.synapses.clone()))
        .collect();
    let mut new_params: HashMap<(u32, String), u16> = HashMap::new();
    for (kind, d) in defaults_table.items.iter().enumerate() {
        for (name, learning) in &description.learning {
            let params = SynapseParams {
                decay: d.decay,
                limit: d.limit,
                stickiness: spikes_model::Stickiness::None,
                release: d.release,
                learning: learning.clone(),
                weight_noise: d.weight_noise,
            };
            let index =
                u16::try_from(synapse_table.insert(params)).expect("fewer than 65,536 distinct synapse parameter sets");
            new_params.insert((kind as u32, name.clone()), index);
        }
    }
    let connection_params: Vec<u16> = description
        .connections
        .iter()
        .map(|c| {
            let post = &description.neurons[index[c.post.as_str()]];
            let params = SynapseParams {
                decay: post.synapses.decay,
                limit: post.synapses.limit,
                stickiness: description.stickiness[&c.stickiness],
                release: post.synapses.release,
                learning: description.learning[&c.learning].clone(),
                weight_noise: post.synapses.weight_noise,
            };
            u16::try_from(synapse_table.insert(params)).expect("fewer than 65,536 distinct synapse parameter sets")
        })
        .collect();
    // Each rule's parameter set for each kind of receiving neuron.
    let rule_params: Vec<HashMap<u32, u16>> = description
        .rules
        .iter()
        .map(|rule| {
            defaults_table
                .items
                .iter()
                .enumerate()
                .map(|(kind, post)| {
                    let params = SynapseParams {
                        decay: post.decay,
                        limit: post.limit,
                        stickiness: description.stickiness[&rule.stickiness],
                        release: post.release,
                        learning: description.learning[&rule.learning].clone(),
                        weight_noise: post.weight_noise,
                    };
                    let index = u16::try_from(synapse_table.insert(params))
                        .expect("fewer than 65,536 distinct synapse parameter sets");
                    (kind as u32, index)
                })
                .collect()
        })
        .collect();
    let direct: Vec<bool> = (0..description.modulators.len())
        .map(|k| {
            description
                .neurons
                .iter()
                .flat_map(|n| &n.receptors)
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

    // Neurons, placed so that nearby neurons share a partition.
    let locations: Vec<[f64; 3]> = description.neurons.iter().map(|d| d.location_um).collect();
    let partitions = spatial_partitions(&locations, rt.partitions());
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
    let mut add = |from: usize, to: usize, synapse: Synapse, synaptic_delay_ms: Option<f64>| {
        let edge = EdgeIdx(incoming[to]);
        incoming[to] = incoming[to].checked_add(1).expect("fewer than 2³² synapses per neuron");
        if local[to] {
            synapses[to].push(synapse);
            pre[to].push(ids[from]);
        }
        if local[from] {
            let distance_um = locations[from]
                .iter()
                .zip(&locations[to])
                .map(|(a, b)| (a - b).powi(2))
                .sum::<f64>()
                .sqrt();
            // Synaptic delay plus conduction along the axon.
            let neuron = &description.neurons[from];
            let synaptic = SimDuration::from_millis_f64(synaptic_delay_ms.unwrap_or(neuron.synaptic_delay_ms));
            let conduction = SimDuration::from_micros_f64(distance_um / neuron.conduction_m_per_s); // µm/µs
            edges[from].push(RouteEdge {
                target: ids[to],
                delay: (synaptic + conduction).max(options.min_delay),
                edge,
            });
        }
    };
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(options.seed);
    for (c, &synapse_params) in description.connections.iter().zip(&connection_params) {
        let (from, to) = (index[c.pre.as_str()], index[c.post.as_str()]);
        let weight = draw(c.weight, &mut rng);
        let equilibrium = if c.equilibrium == c.weight {
            weight
        } else {
            draw(c.equilibrium, &mut rng)
        };
        add(
            from,
            to,
            Synapse::new(weight, equilibrium, c.sign, synapse_params),
            c.synaptic_delay_ms,
        );
    }

    // Rules, receiving neuron by receiving neuron, each with its own random stream: the same connections on every
    // node, whichever neurons it keeps.
    for (r, rule) in description.rules.iter().enumerate() {
        let senders = description.members(&rule.pre).unwrap_or_default();
        let receivers = description.members(&rule.post).unwrap_or_default();
        let grid = rule.radius_um.map(|radius| Grid::new(&senders, &locations, radius));
        let mut candidates: Vec<u32> = Vec::new();
        let mut chosen: Vec<u32> = Vec::new();
        for &post in &receivers {
            let to = post as usize;
            let mut rng = Xoshiro256PlusPlus::seed_from_u64(mix(options.seed, r as u64, u64::from(post)));
            let pool: &[u32] = match &grid {
                Some(grid) => {
                    candidates.clear();
                    grid.within(locations[to], &locations, &mut candidates);
                    &candidates
                }
                None => &senders,
            };
            choose(pool, post, rule.autapses, rule.in_degree, &mut rng, &mut chosen);
            if chosen.is_empty() {
                continue;
            }
            let params = rule_params[r][&defaults[to]];
            for &from in &chosen {
                let from = from as usize;
                let weight = draw(rule.weight, &mut rng);
                let equilibrium = if rule.equilibrium == rule.weight {
                    weight
                } else {
                    draw(rule.equilibrium, &mut rng)
                };
                let sign = rule.sign.unwrap_or(if description.neurons[from].inhibitory {
                    Sign::Inhibitory
                } else {
                    Sign::Excitatory
                });
                add(
                    from,
                    to,
                    Synapse::new(weight, equilibrium, sign, params),
                    rule.synaptic_delay_ms,
                );
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
        neurons: description
            .neurons
            .iter()
            .zip(&defaults)
            .map(|(d, &defaults)| Wire {
                location: d.location_um,
                synaptic_delay_ms: d.synaptic_delay_ms,
                conduction_m_per_s: d.conduction_m_per_s,
                inhibitory: d.inhibitory,
                defaults,
            })
            .collect(),
        params: new_params,
        min_delay: options.min_delay,
    };

    let neurons: Vec<NeuronRef> = ids.iter().map(|&id| NeuronRef::new(id)).collect();
    Network {
        params,
        routes,
        synapse_count,
        wiring,
        by_name: description
            .neurons
            .iter()
            .zip(&neurons)
            .map(|(d, &r)| (d.id.clone(), r))
            .collect(),
        names: description
            .neurons
            .iter()
            .zip(&ids)
            .map(|(d, &id)| (id, d.id.clone()))
            .collect(),
        neurons,
    }
}
