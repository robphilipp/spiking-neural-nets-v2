//! Neurons: the per-event update for the four dynamics classes.
//!
//! | Class | Below threshold | At threshold |
//! |-------|-----------------|--------------|
//! | Monostable integrator | potential integrates inputs and decays | one spike, reset, refractory |
//! | Bistable integrator | as above | tonic firing at a fixed rate until the potential decays below a resting threshold |
//! | Monostable resonator | damped oscillation (resonate-and-fire): inputs in phase add up, out of phase cancel | one spike, reset |
//! | Bistable resonator | damped oscillation | a stable limit cycle (tonic firing at the resonant frequency) until a pulse knocks it inside the separatrix |
//!
//! All four share the membrane-potential noise, the refractoriness kernel, intrinsic plasticity (with optional
//! homeostasis), and the synapse machinery (weights, stochastic release, STDP). Everything between events is
//! computed in closed form. When the dynamics will cross threshold, or fire tonically, *between* events, the
//! update asks for a **wake-up** at that time ([`Wakeup::In`]); the actor layer schedules it as a self-message
//! and must cancel stale ones (a later request replaces an earlier one).
//!
//! Potentials are in mV and times in ms. The refractoriness kernel is evaluated, not stored (v1 accumulated it,
//! discrepancy 8): the potential tested against the threshold is `u + η(t) + b`, with
//! `η(s) = −η₀/(s − τr)` after the absolute refractory period `τr` and no firing during it.

use std::f64::consts::{FRAC_PI_2, TAU};

use rand_core::Rng;
use tempo_core::{SimDuration, SimTime};

use crate::crossing;
use crate::intrinsic::IntrinsicParams;
use crate::modulation::{Effects, Modulators, NoModulators, Receptor, ThreeFactor};
use crate::synapse::{Synapse, SynapseParams};
use crate::units::{decay, duration_ms, since_ms, wiener};

/// Sub-threshold resonance: a damped oscillation at `frequency_hz` with amplitude time constant `damping_ms`.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Resonance {
    /// Oscillation frequency in Hz.
    pub frequency_hz: f64,
    /// Amplitude decay time constant in ms (infinite: undamped).
    pub damping_ms: f64,
}

impl Resonance {
    /// Angular frequency in rad/ms.
    pub fn omega(&self) -> f64 {
        TAU * self.frequency_hz / 1e3
    }

    /// Growth rate of the amplitude in 1/ms (≤ 0).
    pub fn rate(&self) -> f64 {
        if self.damping_ms.is_finite() && self.damping_ms > 0.0 {
            -1.0 / self.damping_ms
        } else {
            0.0
        }
    }

    /// The oscillation period in ms.
    pub fn period_ms(&self) -> f64 {
        1e3 / self.frequency_hz
    }
}

/// The dynamics class and its own parameters.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum NeuronKind {
    /// Monostable integrator (`nty=mi`).
    MonostableIntegrator,
    /// Bistable integrator (`nty=bi`): crossing the threshold starts tonic firing.
    BistableIntegrator {
        /// Tonic firing stops when the potential decays below this (`rst`).
        resting_threshold_mv: f64,
        /// Tonic firing rate in Hz (`tfr`).
        tonic_rate_hz: f64,
    },
    /// Monostable resonator: resonate-and-fire.
    MonostableResonator(Resonance),
    /// Bistable resonator: a resting focus and a stable limit cycle.
    BistableResonator {
        /// Sub-threshold resonance.
        resonance: Resonance,
        /// Radius of the limit cycle (its peak potential) in mV.
        cycle_mv: f64,
        /// Radius of the separatrix in mV: a pulse that leaves the state inside it returns the neuron to rest.
        separatrix_mv: f64,
    },
}

impl NeuronKind {
    /// A short, stable name: `monostable_integrator`, `bistable_integrator`, `monostable_resonator` or
    /// `bistable_resonator`.
    pub fn name(&self) -> &'static str {
        match self {
            Self::MonostableIntegrator => "monostable_integrator",
            Self::BistableIntegrator { .. } => "bistable_integrator",
            Self::MonostableResonator(_) => "monostable_resonator",
            Self::BistableResonator { .. } => "bistable_resonator",
        }
    }
}

/// Parameters shared by a population of neurons.
#[derive(Clone, Debug, PartialEq)]
pub struct NeuronParams {
    /// Dynamics class.
    pub kind: NeuronKind,
    /// Firing threshold in mV (`mst`, or `lct` for bistable neurons).
    pub threshold_mv: f64,
    /// Absolute refractory period in ms (`rfp`).
    pub refractory_ms: f64,
    /// Base refractoriness `η₀` in mV·ms (`rfb`; 1 µWb = 1 mV·ms).
    pub base_refractoriness: f64,
    /// Minimum and reset potential in mV (`mnp`).
    pub min_potential_mv: f64,
    /// Membrane decay time constant in ms (`mpd`); integrators only.
    pub decay_ms: f64,
    /// Rise time constant in ms (`mpr`): delays an integrator's spike by its rise through the threshold.
    pub rise_ms: f64,
    /// Membrane noise magnitude in mV per √ms (`mpn`).
    pub noise_mv: f64,
    /// Amplitude of outgoing spikes in mV (`spp`).
    pub spike_mv: f64,
    /// Intrinsic plasticity.
    pub intrinsic: IntrinsicParams,
}

impl NeuronParams {
    /// v1's monostable-integrator defaults with the given threshold.
    pub fn monostable_integrator(threshold_mv: f64) -> Self {
        Self {
            kind: NeuronKind::MonostableIntegrator,
            threshold_mv,
            refractory_ms: 20.0,
            base_refractoriness: 0.08,
            min_potential_mv: 0.0,
            decay_ms: 250.0,
            rise_ms: 2.0,
            noise_mv: 0.0,
            spike_mv: 1.0,
            intrinsic: IntrinsicParams::OFF,
        }
    }
}

/// Tonic or not.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Below threshold (or the only phase, for monostable neurons).
    Rest,
    /// Firing tonically (bistable neurons).
    Tonic,
}

/// A neuron's state.
#[derive(Clone, Debug, PartialEq)]
pub struct NeuronState {
    /// Membrane potential in mV (for resonators, the imaginary part of the oscillator state).
    pub u: f64,
    /// Resonators: the real part of the oscillator state, in mV.
    pub x: f64,
    /// Intrinsic-plasticity offset in mV.
    pub b: f64,
    /// Firing-rate estimate in Hz (for homeostasis).
    pub rate_hz: f64,
    /// Time of the last update.
    pub last_event: SimTime,
    /// Time of the last spike, if any.
    pub last_fire: Option<SimTime>,
    /// Tonic or resting.
    pub phase: Phase,
    /// Bistable resonators while tonic: the phase angle on the limit cycle at `last_event` (radians).
    pub angle: f64,
    /// Spikes fired so far.
    pub spikes: u64,
}

impl NeuronState {
    /// A neuron at rest at time zero.
    pub fn at_rest(p: &NeuronParams) -> Self {
        Self {
            u: p.min_potential_mv,
            x: 0.0,
            b: 0.0,
            rate_hz: 0.0,
            last_event: SimTime::ZERO,
            last_fire: None,
            phase: Phase::Rest,
            angle: 0.0,
            spikes: 0,
        }
    }
}

/// What arrives at a neuron.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Input {
    /// From outside the network (a sensor): weight 1, no synapse.
    External {
        /// Signal in mV.
        mv: f64,
    },
    /// A spike through one of the neuron's synapses.
    Synaptic {
        /// Index of the synapse in the neuron's synapse table.
        synapse: u32,
        /// The spike's amplitude in mV (the sender's `spp`); the synapse's sign and weight are applied here.
        mv: f64,
    },
}

/// Whether the neuron needs a wake-up message.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Wakeup {
    /// No change to any pending wake-up.
    Keep,
    /// Cancel any pending wake-up.
    Cancel,
    /// Replace any pending wake-up with one this long from now.
    In(SimDuration),
}

/// A change of phase (bistable neurons).
#[derive(Copy, Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Transition {
    /// Started firing tonically.
    StartedTonic,
    /// Returned to rest.
    Rested,
}

/// A weight changed by learning.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct WeightChange {
    /// The synapse.
    pub synapse: u32,
    /// Weight before.
    pub before: f32,
    /// Weight after.
    pub after: f32,
}

/// The result of an update.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Outcome {
    /// A spike, emitted this long after the event (the rise time).
    pub spike: Option<SimDuration>,
    /// A phase change.
    pub transition: Option<Transition>,
    /// What to do about wake-ups.
    pub wakeup: Wakeup,
    /// The membrane potential after the update (as tested against the threshold, less intrinsic plasticity).
    pub membrane_mv: f64,
    /// For synaptic input: whether the synapse released (if not, nothing else happened).
    pub released: bool,
}

impl Outcome {
    fn quiet(membrane_mv: f64) -> Self {
        Self {
            spike: None,
            transition: None,
            wakeup: Wakeup::Keep,
            membrane_mv,
            released: true,
        }
    }

    fn spike(delay: SimDuration, membrane_mv: f64) -> Self {
        Self {
            spike: Some(delay),
            ..Self::quiet(membrane_mv)
        }
    }
}

/// The spike delay due to the membrane potential's finite rise time (v1's linear approximation of the alpha
/// kernel): `τrise·(min(θ, u) − u₀)/θ`, or 0 if the input didn't raise the potential or it was already at θ.
pub fn rise_delay_ms(before: f64, after: f64, threshold: f64, rise_ms: f64) -> f64 {
    if after <= before || before >= threshold || threshold <= 0.0 {
        0.0
    } else {
        rise_ms * (threshold.min(after) - before) / threshold
    }
}

/// A neuron population's parameters plus the synapse parameter table, its receptors and access to modulator
/// concentrations: everything needed to update a neuron.
#[derive(Copy, Clone)]
pub struct Model<'a> {
    /// The neuron's population parameters.
    pub neuron: &'a NeuronParams,
    /// Synapse parameters, indexed by [`Synapse::params`].
    pub synapses: &'a [SynapseParams],
    /// The neuron's receptors.
    pub receptors: &'a [Receptor],
    /// Modulator concentrations at the neuron.
    pub modulators: &'a dyn Modulators,
}

impl std::fmt::Debug for Model<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Model")
            .field("neuron", &self.neuron)
            .field("receptors", &self.receptors)
            .finish_non_exhaustive()
    }
}

impl<'a> Model<'a> {
    /// A model without neuromodulation.
    pub fn new(neuron: &'a NeuronParams, synapses: &'a [SynapseParams]) -> Self {
        Self {
            neuron,
            synapses,
            receptors: &[],
            modulators: &NoModulators,
        }
    }

    /// The same model with receptors and modulator concentrations.
    pub fn with_modulation(self, receptors: &'a [Receptor], modulators: &'a dyn Modulators) -> Self {
        Self {
            receptors,
            modulators,
            ..self
        }
    }
}

impl Model<'_> {
    /// Handles an input. Weight changes from learning are appended to `changes`.
    pub fn on_input<R: Rng + ?Sized>(
        &self,
        st: &mut NeuronState,
        syns: &mut [Synapse],
        now: SimTime,
        input: Input,
        rng: &mut R,
        changes: &mut Vec<WeightChange>,
    ) -> Outcome {
        let fx = Effects::at(self.receptors, self.modulators, now);
        let (value, synapse) = match input {
            Input::External { mv } => (mv, None),
            Input::Synaptic { synapse, mv } => {
                let syn = &mut syns[synapse as usize];
                let params = &self.synapses[usize::from(syn.params)];
                self.catch_up(syn, synapse, now, rng, changes);
                if !syn.arrive(params, now, rng) {
                    return Outcome {
                        released: false,
                        ..Outcome::quiet(st.u)
                    };
                }
                syn.record_signal();
                // Modulators scale synaptic input by the gain for its sign.
                (fx.gate(syn.efficacy() * mv), Some(synapse))
            }
        };
        let event = Event {
            now,
            value: Some(value),
            synapse,
            fx,
        };
        match self.neuron.kind {
            NeuronKind::MonostableIntegrator => self.integrator(st, syns, event, rng, changes),
            NeuronKind::BistableIntegrator { tonic_rate_hz, .. } => match st.phase {
                Phase::Rest => self.integrator(st, syns, event, rng, changes),
                Phase::Tonic => self.bistable_integrator_tonic(st, syns, event, tonic_rate_hz, rng, changes),
            },
            NeuronKind::MonostableResonator(_) => self.resonator_rest(st, syns, event, rng, changes),
            NeuronKind::BistableResonator { .. } => match st.phase {
                Phase::Rest => self.resonator_rest(st, syns, event, rng, changes),
                Phase::Tonic => self.bistable_resonator_tonic(st, syns, event, rng, changes),
            },
        }
    }

    /// Handles a wake-up the neuron asked for (a tonic spike, or a predicted threshold crossing).
    pub fn on_wakeup<R: Rng + ?Sized>(
        &self,
        st: &mut NeuronState,
        syns: &mut [Synapse],
        now: SimTime,
        rng: &mut R,
        changes: &mut Vec<WeightChange>,
    ) -> Outcome {
        let event = Event {
            now,
            value: None,
            synapse: None,
            fx: Effects::at(self.receptors, self.modulators, now),
        };
        match (self.neuron.kind, st.phase) {
            (NeuronKind::BistableIntegrator { tonic_rate_hz, .. }, Phase::Tonic) => {
                self.bistable_integrator_tonic(st, syns, event, tonic_rate_hz, rng, changes)
            }
            (NeuronKind::MonostableResonator(_), _) | (NeuronKind::BistableResonator { .. }, Phase::Rest) => {
                self.resonator_rest(st, syns, event, rng, changes)
            }
            (NeuronKind::BistableResonator { .. }, Phase::Tonic) => {
                self.bistable_resonator_tonic(st, syns, event, rng, changes)
            }
            // An integrator at rest asked to be woken for a modulator-driven threshold crossing.
            _ => self.integrator(st, syns, event, rng, changes),
        }
    }

    /// A modulator has just reached the neuron (for modulators with direct-potential receptors): re-evaluates the
    /// threshold and any predicted crossing. Unlike [`Self::on_wakeup`], it never produces a tonic spike.
    pub fn on_modulation<R: Rng + ?Sized>(
        &self,
        st: &mut NeuronState,
        syns: &mut [Synapse],
        now: SimTime,
        rng: &mut R,
        changes: &mut Vec<WeightChange>,
    ) -> Outcome {
        let event = Event {
            now,
            value: None,
            synapse: None,
            fx: Effects::at(self.receptors, self.modulators, now),
        };
        match (self.neuron.kind, st.phase) {
            (_, Phase::Tonic) => Outcome::quiet(st.u),
            (NeuronKind::MonostableResonator(_) | NeuronKind::BistableResonator { .. }, _) => {
                self.resonator_rest(st, syns, event, rng, changes)
            }
            _ => self.integrator(st, syns, event, rng, changes),
        }
    }

    // ----- shared pieces -----

    /// Advances intrinsic plasticity to `now` and returns the time since the last update in ms.
    fn elapse(&self, st: &mut NeuronState, now: SimTime) -> f64 {
        let dt = since_ms(now, st.last_event);
        self.neuron.intrinsic.advance(&mut st.b, &mut st.rate_hz, dt);
        st.last_event = now;
        dt
    }

    /// The neuron's three-factor learning receptor, if any.
    fn three_factor(&self) -> Option<(u16, ThreeFactor)> {
        self.receptors
            .iter()
            .find_map(|r| r.three_factor.map(|tf| (r.modulator, tf)))
    }

    /// Brings a synapse up to `now`: first the three-factor weight change accumulated since its last update
    /// (`rate·e₀·∫ e^(−(s−t₀)/τe)·c(s) ds`, exact), then decay, noise, release sums and traces.
    fn catch_up<R: Rng + ?Sized>(
        &self,
        syn: &mut Synapse,
        index: u32,
        now: SimTime,
        rng: &mut R,
        changes: &mut Vec<WeightChange>,
    ) {
        let params = &self.synapses[usize::from(syn.params)];
        if let Some((kind, tf)) = self.three_factor()
            && syn.eligibility != 0.0
        {
            let e0 = f64::from(syn.eligibility);
            let integral = self.modulators.decayed_integral(kind, syn.last, now, tf.eligibility_ms);
            syn.eligibility = (e0 * decay(since_ms(now, syn.last), tf.eligibility_ms)) as f32;
            let dw = tf.rate * e0 * integral / 1e3; // integral in concentration·ms; rate per second
            if dw != 0.0 {
                let before = syn.weight;
                syn.weight = params.limit.apply(f64::from(before) + dw) as f32;
                changes.push(WeightChange {
                    synapse: index,
                    before,
                    after: syn.weight,
                });
            }
        }
        syn.advance(params, now, rng);
    }

    /// Applies an STDP change: to the eligibility trace for three-factor learning, otherwise to the weight
    /// (scaled by the plasticity modulation).
    fn learn(&self, syn: &mut Synapse, index: u32, dw: f64, fx: &Effects, changes: &mut Vec<WeightChange>) {
        if dw == 0.0 {
            return;
        }
        if self.three_factor().is_some() {
            syn.eligibility += dw as f32;
            return;
        }
        let params = &self.synapses[usize::from(syn.params)];
        let before = syn.weight;
        syn.weight = params.limit.apply(f64::from(before) + dw * fx.plasticity) as f32;
        if syn.weight != before {
            changes.push(WeightChange {
                synapse: index,
                before,
                after: syn.weight,
            });
        }
    }

    /// The refractoriness kernel at `now`: `None` during the absolute refractory period, otherwise `η ≤ 0`.
    fn refractoriness(&self, st: &NeuronState, now: SimTime) -> Option<f64> {
        let Some(fired) = st.last_fire else {
            return Some(0.0);
        };
        let s = since_ms(now, fired) - self.neuron.refractory_ms;
        (s > 0.0).then(|| -self.neuron.base_refractoriness / s)
    }

    /// When the absolute refractory period ends.
    fn refractory_end(&self, st: &NeuronState) -> Option<SimTime> {
        st.last_fire
            .map(|t| t.saturating_add(duration_ms(self.neuron.refractory_ms)))
    }

    /// A spike: counts it, updates intrinsic plasticity, and applies STDP potentiation to every synapse that
    /// carried a signal since the previous spike.
    fn fire<R: Rng + ?Sized>(
        &self,
        st: &mut NeuronState,
        syns: &mut [Synapse],
        event: &Event,
        rng: &mut R,
        changes: &mut Vec<WeightChange>,
    ) {
        let now = event.now;
        st.spikes += 1;
        st.last_fire = Some(now);
        self.neuron.intrinsic.on_spike(&mut st.b, &mut st.rate_hz);
        for (i, syn) in syns.iter_mut().enumerate() {
            if syn.count == 0 {
                continue;
            }
            self.catch_up(syn, i as u32, now, rng, changes);
            let params = &self.synapses[usize::from(syn.params)];
            let (_, upper) = params.limit.bounds();
            let dw = params.learning.potentiation(
                f64::from(syn.trace[0]),
                f64::from(syn.trace[1]),
                syn.count,
                f64::from(syn.weight),
                upper,
            );
            syn.reset_traces();
            self.learn(syn, i as u32, dw, &event.fx, changes);
        }
    }

    /// STDP depression for a signal that arrived after the last spike without causing a new one.
    fn depress(&self, st: &NeuronState, syns: &mut [Synapse], event: Event, changes: &mut Vec<WeightChange>) {
        let (Some(index), Some(fired)) = (event.synapse, st.last_fire) else {
            return;
        };
        let syn = &mut syns[index as usize];
        let params = &self.synapses[usize::from(syn.params)];
        let (lower, _) = params.limit.bounds();
        let dw = params
            .learning
            .depression(since_ms(event.now, fired), f64::from(syn.weight), lower);
        self.learn(syn, index, dw, &event.fx, changes);
    }

    /// For an integrator below threshold: when will direct-potential modulators carry `u + m(t)` to `target`?
    /// `u` decays with the membrane time constant and each modulator's contribution with its clearance time
    /// constant; future releases arrive as their own events, so they needn't be predicted.
    fn predict_direct_crossing(&self, u: f64, now: SimTime, target: f64) -> Wakeup {
        let terms: Vec<(f64, f64)> = self
            .receptors
            .iter()
            .filter(|r| r.direct_mv != 0.0)
            .map(|r| {
                (
                    r.direct_mv * self.modulators.level(r.modulator, now),
                    self.modulators.clearance_ms(r.modulator),
                )
            })
            .filter(|(m, _)| *m != 0.0)
            .collect();
        let crossing = match terms.as_slice() {
            [] => return Wakeup::Keep,
            [(m, tau_m)] => crossing::two_exponentials(u, self.neuron.decay_ms, *m, *tau_m, target),
            _ => {
                let f = |t: f64| {
                    u * decay(t, self.neuron.decay_ms) + terms.iter().map(|(m, tau)| m * decay(t, *tau)).sum::<f64>()
                };
                let horizon = 5.0 * terms.iter().map(|(_, tau)| *tau).fold(self.neuron.decay_ms, f64::max);
                crossing::first_crossing(f, target, horizon)
            }
        };
        match crossing {
            Some(t) => Wakeup::In(duration_ms(t).max(SimDuration::NANOSECOND)),
            None => Wakeup::Cancel,
        }
    }

    // ----- integrators -----

    /// Monostable integrator, and bistable integrator at rest.
    fn integrator<R: Rng + ?Sized>(
        &self,
        st: &mut NeuronState,
        syns: &mut [Synapse],
        event: Event,
        rng: &mut R,
        changes: &mut Vec<WeightChange>,
    ) -> Outcome {
        let p = self.neuron;
        let now = event.now;
        let dt = self.elapse(st, now);
        let decayed = (st.u + wiener(p.noise_mv, dt, rng)) * decay(dt, p.decay_ms);
        let threshold = p.threshold_mv * event.fx.threshold;
        let direct = event.fx.direct_mv;
        let Some(eta) = self.refractoriness(st, now) else {
            // Absolute refractory period: the potential is held at its minimum and input is lost (v1). A direct
            // modulator may still push it over when the period ends, so look again then.
            st.u = p.min_potential_mv;
            self.depress(st, syns, event, changes);
            let wakeup = match self.refractory_end(st) {
                Some(end) if direct > 0.0 => {
                    Wakeup::In(end.saturating_duration_since(now).max(SimDuration::NANOSECOND))
                }
                _ => Wakeup::Keep,
            };
            return Outcome {
                wakeup,
                ..Outcome::quiet(st.u)
            };
        };
        let mut updated = (event.value.unwrap_or(0.0) + decayed).max(p.min_potential_mv);
        let bistable = matches!(p.kind, NeuronKind::BistableIntegrator { .. });
        if bistable {
            updated = updated.min(threshold); // v1 caps a bistable neuron's potential at its threshold
        }
        let effective = updated + eta + st.b + direct;
        if effective < threshold {
            st.u = updated;
            self.depress(st, syns, event, changes);
            return Outcome {
                wakeup: self.predict_direct_crossing(updated, now, threshold - eta - st.b),
                ..Outcome::quiet((updated + eta).max(p.min_potential_mv))
            };
        }

        let rise = duration_ms(rise_delay_ms(
            decayed + eta + st.b + direct,
            effective,
            threshold,
            p.rise_ms,
        ));
        self.fire(st, syns, &event, rng, changes);
        match p.kind {
            NeuronKind::BistableIntegrator { tonic_rate_hz, .. } => {
                // Into the limit cycle: the potential stays, tonic spikes follow at the tonic rate.
                st.u = updated;
                st.phase = Phase::Tonic;
                Outcome {
                    transition: Some(Transition::StartedTonic),
                    wakeup: Wakeup::In(duration_ms(1e3 / tonic_rate_hz)),
                    ..Outcome::spike(rise, updated)
                }
            }
            _ => {
                st.u = p.min_potential_mv;
                Outcome::spike(rise, st.u)
            }
        }
    }

    /// Bistable integrator while tonic. Inputs keep adding to the potential (v1 dropped them, discrepancy 4);
    /// each wake-up is a tonic spike, after which firing stops if the potential has decayed below the resting
    /// threshold.
    fn bistable_integrator_tonic<R: Rng + ?Sized>(
        &self,
        st: &mut NeuronState,
        syns: &mut [Synapse],
        event: Event,
        tonic_rate_hz: f64,
        rng: &mut R,
        changes: &mut Vec<WeightChange>,
    ) -> Outcome {
        let p = self.neuron;
        let NeuronKind::BistableIntegrator {
            resting_threshold_mv, ..
        } = p.kind
        else {
            unreachable!()
        };
        let now = event.now;
        let dt = self.elapse(st, now);
        let decayed = (st.u + wiener(p.noise_mv, dt, rng)) * decay(dt, p.decay_ms);
        st.u = (event.value.unwrap_or(0.0) + decayed)
            .max(p.min_potential_mv)
            .min(p.threshold_mv * event.fx.threshold);
        if event.value.is_some() {
            self.depress(st, syns, event, changes);
            return Outcome::quiet(st.u);
        }
        // A tonic spike.
        self.fire(st, syns, &event, rng, changes);
        if st.u + st.b + event.fx.direct_mv < resting_threshold_mv * event.fx.threshold {
            st.phase = Phase::Rest;
            Outcome {
                transition: Some(Transition::Rested),
                wakeup: Wakeup::Cancel,
                ..Outcome::spike(SimDuration::ZERO, st.u)
            }
        } else {
            Outcome {
                wakeup: Wakeup::In(duration_ms(1e3 / tonic_rate_hz)),
                ..Outcome::spike(SimDuration::ZERO, st.u)
            }
        }
    }

    // ----- resonators -----

    /// Advances the damped oscillator `z = x + i·u` by `dt` ms: `z ← z·e^((b + iω)·dt)`.
    fn rotate(st: &mut NeuronState, dt: f64, r: Resonance) {
        let k = (r.rate() * dt).exp();
        let (s, c) = (r.omega() * dt).sin_cos();
        let (x, y) = (st.x, st.u);
        st.x = k * (x * c - y * s);
        st.u = k * (x * s + y * c);
    }

    fn resonance(&self) -> Resonance {
        match self.neuron.kind {
            NeuronKind::MonostableResonator(r) | NeuronKind::BistableResonator { resonance: r, .. } => r,
            _ => unreachable!("not a resonator"),
        }
    }

    /// A resonator below threshold (monostable resonators always; bistable resonators at rest).
    fn resonator_rest<R: Rng + ?Sized>(
        &self,
        st: &mut NeuronState,
        syns: &mut [Synapse],
        event: Event,
        rng: &mut R,
        changes: &mut Vec<WeightChange>,
    ) -> Outcome {
        let p = self.neuron;
        let r = self.resonance();
        let now = event.now;
        let dt = self.elapse(st, now);
        Self::rotate(st, dt, r);
        st.u += wiener(p.noise_mv, dt, rng);

        let Some(eta) = self.refractoriness(st, now) else {
            // Absolute refractory period: inputs are lost; look again when it ends.
            self.depress(st, syns, event, changes);
            let end = self.refractory_end(st).unwrap_or(now);
            return Outcome {
                wakeup: Wakeup::In(end.saturating_duration_since(now).max(SimDuration::NANOSECOND)),
                ..Outcome::quiet(st.u)
            };
        };
        if let Some(value) = event.value {
            st.u += value; // a pulse kicks the voltage-like component
        }

        let target = p.threshold_mv * event.fx.threshold - eta - st.b - event.fx.direct_mv;
        if st.u < target {
            self.depress(st, syns, event, changes);
            // Will the oscillation carry it over the threshold before the next input? (The refractoriness and
            // intrinsic plasticity are held at their current values for the prediction.)
            let wakeup = match crossing::damped_oscillator(st.x, st.u, r.rate(), r.omega(), target) {
                Some(t) => Wakeup::In(duration_ms(t).max(SimDuration::NANOSECOND)),
                None => Wakeup::Cancel,
            };
            return Outcome {
                wakeup,
                ..Outcome::quiet(st.u + eta)
            };
        }

        self.fire(st, syns, &event, rng, changes);
        match p.kind {
            NeuronKind::BistableResonator { cycle_mv, .. } => {
                // Onto the limit cycle, at its peak; the next tonic spike is one period later.
                st.phase = Phase::Tonic;
                st.angle = FRAC_PI_2;
                st.x = 0.0;
                st.u = cycle_mv;
                Outcome {
                    transition: Some(Transition::StartedTonic),
                    wakeup: Wakeup::In(duration_ms(r.period_ms())),
                    ..Outcome::spike(SimDuration::ZERO, cycle_mv)
                }
            }
            _ => {
                st.x = 0.0;
                st.u = p.min_potential_mv;
                Outcome {
                    wakeup: Wakeup::Cancel,
                    ..Outcome::spike(SimDuration::ZERO, st.u)
                }
            }
        }
    }

    /// A bistable resonator on its limit cycle. Wake-ups are tonic spikes at the cycle's peak. A pulse moves the
    /// state off the cycle: if it ends up inside the separatrix the neuron returns to rest; otherwise it is pulled
    /// back onto the cycle at the new phase (the cycle attracts much faster than an oscillation period).
    fn bistable_resonator_tonic<R: Rng + ?Sized>(
        &self,
        st: &mut NeuronState,
        syns: &mut [Synapse],
        event: Event,
        rng: &mut R,
        changes: &mut Vec<WeightChange>,
    ) -> Outcome {
        let NeuronKind::BistableResonator {
            resonance: r,
            cycle_mv,
            separatrix_mv,
        } = self.neuron.kind
        else {
            unreachable!()
        };
        let now = event.now;
        let dt = self.elapse(st, now);
        st.angle = (st.angle + r.omega() * dt).rem_euclid(TAU);

        let Some(value) = event.value else {
            // A tonic spike at the peak of the cycle.
            st.angle = FRAC_PI_2;
            st.x = 0.0;
            st.u = cycle_mv;
            self.fire(st, syns, &event, rng, changes);
            return Outcome {
                wakeup: Wakeup::In(duration_ms(r.period_ms())),
                ..Outcome::spike(SimDuration::ZERO, cycle_mv)
            };
        };

        let (s, c) = st.angle.sin_cos();
        let x = cycle_mv * c;
        let y = cycle_mv * s + value;
        self.depress(st, syns, event, changes);
        if x.hypot(y) < separatrix_mv {
            // Knocked inside the separatrix: back to the resting focus, as a damped oscillation from here.
            st.phase = Phase::Rest;
            st.x = x;
            st.u = y;
            let mut outcome = self.resonator_rest(
                st,
                syns,
                Event {
                    value: None,
                    synapse: None,
                    ..event
                },
                rng,
                changes,
            );
            outcome.transition = Some(Transition::Rested);
            if outcome.spike.is_none() && outcome.wakeup == Wakeup::Keep {
                outcome.wakeup = Wakeup::Cancel;
            }
            return outcome;
        }
        st.angle = y.atan2(x).rem_euclid(TAU);
        st.x = cycle_mv * st.angle.cos();
        st.u = cycle_mv * st.angle.sin();
        let mut to_peak = (FRAC_PI_2 - st.angle).rem_euclid(TAU);
        if to_peak < 1e-12 {
            to_peak = TAU;
        }
        Outcome {
            wakeup: Wakeup::In(duration_ms(to_peak / r.omega())),
            ..Outcome::quiet(st.u)
        }
    }
}

/// One event being processed.
#[derive(Copy, Clone, Debug)]
struct Event {
    now: SimTime,
    /// The (signed, weighted) input in mV, or `None` for a wake-up.
    value: Option<f64>,
    /// The synapse it came through, if any.
    synapse: Option<u32>,
    /// Neuromodulator effects at this moment.
    fx: Effects,
}
