//! A random network of leaky integrate-and-fire neurons driven by Poisson inputs.
//!
//! This is a *toy* stand-in for the real neuron model, with the features that stress the runtime:
//! - neurons placed at random in a cube; each connection's delay = minimum delay + distance / conduction speed,
//! - fan-out routes with per-edge delays (optionally jittered),
//! - per-synapse sign (a neuron can have both excitatory and inhibitory outgoing connections),
//! - stochastic release decided on the receiving side, using the receiver's random stream,
//! - analytic (tick-free) membrane decay between events.

use rand::{RngExt, SeedableRng};
use rand_xoshiro::Xoshiro256PlusPlus;
use tempo_runtime::{Actor, ActorId, Ctx, EdgeIdx, Jitter, RouteEdge, RouteId, Runtime, SimDuration, SimTime};

/// Messages in the LIF network.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum LifMsg {
    /// A spike from a pre-synaptic neuron (delivered through a route).
    Spike,
    /// External input of this many millivolts.
    Input(f32),
    /// A Poisson source's self-scheduled tick.
    Tick,
}

/// Events emitted by the LIF network.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LifEvent {
    /// A neuron fired.
    Spike,
}

/// An incoming synapse, stored on the receiving neuron.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Synapse {
    /// Effect on the membrane potential in mV: positive is excitatory, negative inhibitory.
    pub weight_mv: f32,
    /// Probability that an arriving spike is released.
    pub release_p: f32,
}

/// A leaky integrate-and-fire neuron.
#[derive(Clone, Debug)]
pub struct LifNeuron {
    /// Membrane potential, mV.
    pub u_mv: f64,
    /// Time of the last update.
    pub last: SimTime,
    /// No integration before this time (absolute refractory period).
    pub refractory_until: SimTime,
    /// Membrane time constant, ms.
    pub tau_ms: f32,
    /// Firing threshold, mV.
    pub threshold_mv: f32,
    /// Absolute refractory period.
    pub refractory: SimDuration,
    /// Incoming synapses, indexed by [`EdgeIdx`].
    pub synapses: Vec<Synapse>,
    /// Outgoing route.
    pub out: Option<RouteId>,
    /// Spikes fired.
    pub spikes: u32,
}

impl LifNeuron {
    fn integrate(&mut self, ctx: &mut Ctx<'_, LifMsg, LifEvent>, mv: f32) {
        let now = ctx.now();
        if now < self.refractory_until {
            self.last = now;
            return;
        }
        let dt_ms = (now - self.last).as_millis_f64();
        self.u_mv = self.u_mv * (-dt_ms / f64::from(self.tau_ms)).exp() + f64::from(mv);
        self.u_mv = self.u_mv.max(-f64::from(self.threshold_mv));
        self.last = now;
        if self.u_mv >= f64::from(self.threshold_mv) {
            self.u_mv = 0.0;
            self.refractory_until = now + self.refractory;
            self.spikes += 1;
            ctx.emit(LifEvent::Spike);
            if let Some(route) = self.out {
                ctx.fanout(route, LifMsg::Spike);
            }
        }
    }
}

impl Actor<LifMsg, LifEvent> for LifNeuron {
    fn on_message(&mut self, ctx: &mut Ctx<'_, LifMsg, LifEvent>, _from: ActorId, msg: LifMsg) {
        if let LifMsg::Input(mv) = msg {
            self.integrate(ctx, mv);
        }
    }

    fn on_route_message(&mut self, ctx: &mut Ctx<'_, LifMsg, LifEvent>, _from: ActorId, edge: EdgeIdx, _msg: LifMsg) {
        let synapse = self.synapses[edge.0 as usize];
        // Release is decided at the synapse, on the receiving side (decision 4 in the plan).
        if ctx.rng().random::<f32>() < synapse.release_p {
            self.integrate(ctx, synapse.weight_mv);
        }
    }
}

/// Sends input to random targets at Poisson-distributed times until `until`.
#[derive(Clone, Debug)]
pub struct PoissonSource {
    /// Mean event rate, Hz.
    pub rate_hz: f64,
    /// Neurons to choose targets from.
    pub targets: Vec<ActorId>,
    /// Input strength, mV.
    pub input_mv: f32,
    /// Stop after this time.
    pub until: SimTime,
}

impl PoissonSource {
    fn schedule_next(&mut self, ctx: &mut Ctx<'_, LifMsg, LifEvent>) {
        // Exponential inter-arrival time; 1 - u keeps the log argument in (0, 1].
        let u: f64 = 1.0 - ctx.rng().random::<f64>();
        let wait = SimDuration::from_secs_f64(-u.ln() / self.rate_hz).max(SimDuration::NANOSECOND);
        if ctx.now() + wait <= self.until {
            ctx.schedule_self(wait, LifMsg::Tick);
        }
    }
}

impl Actor<LifMsg, LifEvent> for PoissonSource {
    fn on_start(&mut self, ctx: &mut Ctx<'_, LifMsg, LifEvent>) {
        self.schedule_next(ctx);
    }

    fn on_message(&mut self, ctx: &mut Ctx<'_, LifMsg, LifEvent>, _from: ActorId, msg: LifMsg) {
        if msg == LifMsg::Tick && !self.targets.is_empty() {
            let target = self.targets[ctx.rng().random_range(0..self.targets.len())];
            ctx.send(target, LifMsg::Input(self.input_mv));
            self.schedule_next(ctx);
        }
    }
}

/// Network parameters.
#[derive(Clone, Debug)]
pub struct LifConfig {
    /// Number of neurons.
    pub neurons: usize,
    /// Outgoing connections per neuron.
    pub synapses_per_neuron: usize,
    /// Fraction of synapses that are inhibitory.
    pub inhibitory_fraction: f64,
    /// Excitatory synapse weight, mV (inhibitory synapses use `-inhibitory_scale * weight_mv`).
    pub weight_mv: f32,
    /// Inhibitory weight as a multiple of the excitatory weight.
    pub inhibitory_scale: f32,
    /// Release probability of every synapse.
    pub release_p: f32,
    /// Number of Poisson sources.
    pub sources: usize,
    /// Rate of each source, Hz.
    pub source_rate_hz: f64,
    /// Strength of each source input, mV.
    pub input_mv: f32,
    /// Sources stop after this time.
    pub input_until: SimTime,
    /// Side of the cube neurons are placed in, µm.
    pub cube_um: f64,
    /// Axon conduction speed, m/s (numerically equal to µm/µs).
    pub conduction_m_per_s: f64,
    /// Added to every connection delay.
    pub min_delay: SimDuration,
    /// Intentional delay noise on every route.
    pub jitter: Jitter,
    /// Seed for the network's structure (positions, connections).
    pub topology_seed: u64,
}

impl Default for LifConfig {
    fn default() -> Self {
        Self {
            neurons: 1_000,
            synapses_per_neuron: 100,
            inhibitory_fraction: 0.2,
            weight_mv: 0.6,
            inhibitory_scale: 3.0,
            release_p: 0.8,
            sources: 20,
            source_rate_hz: 2_000.0,
            input_mv: 6.0,
            input_until: SimTime::from_start(SimDuration::from_millis(100)),
            cube_um: 500.0,
            conduction_m_per_s: 1.0,
            min_delay: SimDuration::from_micros(100),
            jitter: Jitter::None,
            topology_seed: 1,
        }
    }
}

/// The IDs of a built network.
#[derive(Clone, Debug)]
pub struct LifNetwork {
    /// The neurons.
    pub neurons: Vec<ActorId>,
    /// The Poisson sources.
    pub sources: Vec<ActorId>,
}

/// Builds the network described by `config` in `rt`.
pub fn build(rt: &mut Runtime<LifMsg, LifEvent>, config: &LifConfig) -> LifNetwork {
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(config.topology_seed);
    let n = config.neurons;

    let neuron = LifNeuron {
        u_mv: 0.0,
        last: SimTime::ZERO,
        refractory_until: SimTime::ZERO,
        tau_ms: 20.0,
        threshold_mv: 10.0,
        refractory: SimDuration::from_millis(2),
        synapses: Vec::new(),
        out: None,
        spikes: 0,
    };
    let neurons: Vec<ActorId> = (0..n).map(|_| rt.spawn(neuron.clone())).collect();
    let positions: Vec<[f64; 3]> = (0..n)
        .map(|_| std::array::from_fn(|_| rng.random::<f64>() * config.cube_um))
        .collect();

    // Choose each neuron's targets; the synapse lives on the target, and its index there is the edge index.
    let mut incoming: Vec<Vec<Synapse>> = vec![Vec::new(); n];
    let mut outgoing: Vec<Vec<RouteEdge>> = vec![Vec::new(); n];
    for pre in 0..n {
        for _ in 0..config.synapses_per_neuron.min(n.saturating_sub(1)) {
            let mut post = rng.random_range(0..n);
            while post == pre {
                post = rng.random_range(0..n);
            }
            let inhibitory = rng.random::<f64>() < config.inhibitory_fraction;
            let weight_mv = if inhibitory {
                -config.inhibitory_scale * config.weight_mv
            } else {
                config.weight_mv
            };
            let distance_um = positions[pre]
                .iter()
                .zip(&positions[post])
                .map(|(a, b)| (a - b).powi(2))
                .sum::<f64>()
                .sqrt();
            let delay = config.min_delay + SimDuration::from_micros_f64(distance_um / config.conduction_m_per_s);
            let edge = EdgeIdx(u32::try_from(incoming[post].len()).expect("edge index fits u32"));
            incoming[post].push(Synapse {
                weight_mv,
                release_p: config.release_p,
            });
            outgoing[pre].push(RouteEdge {
                target: neurons[post],
                delay,
                edge,
            });
        }
    }

    for (i, (synapses, edges)) in incoming.into_iter().zip(outgoing).enumerate() {
        let route = rt.add_route(neurons[i], edges, config.jitter);
        let neuron = rt.actor_mut::<LifNeuron>(neurons[i]).expect("just spawned");
        neuron.synapses = synapses;
        neuron.out = Some(route);
    }

    let sources = (0..config.sources)
        .map(|_| {
            rt.spawn(PoissonSource {
                rate_hz: config.source_rate_hz,
                targets: neurons.clone(),
                input_mv: config.input_mv,
                until: config.input_until,
            })
        })
        .collect();

    LifNetwork { neurons, sources }
}
