//! Neuromodulation: how modulator concentrations change a neuron's response (actors plan 09).
//!
//! A neuron's **receptors** map a modulator to effects, each linear in the concentration `c`:
//!
//! | Effect | Applied as |
//! |--------|-----------|
//! | excitatory gain `α_exc` | excitatory synaptic input × `max(0, 1 + α_exc·c)` |
//! | inhibitory gain `α_inh` | inhibitory synaptic input × `max(0, 1 + α_inh·c)` |
//! | threshold `α_θ` | threshold × `max(0, 1 + α_θ·c)` |
//! | plasticity `α_η` | STDP weight changes × `max(0, 1 + α_η·c)` |
//! | three-factor learning | STDP changes go to an eligibility trace `e` (time constant `τ_e`); the weight changes by `rate·∫ e(s)·c(s) ds` |
//! | direct potential `m` (mV) | adds `m·c` to the potential tested against the threshold |
//!
//! Negative coefficients reduce the effect, so the same modulator can excite one population and inhibit another
//! (like dopamine's D1- and D2-type receptors). Effects of several receptors multiply (gains) or add (direct).
//!
//! Concentrations come from the [`Modulators`] trait, so the model doesn't depend on how they are stored. The
//! network layer stores releases as **pulses**: each reaches a neuron at some time with some amplitude and then
//! decays exponentially. [`pulse_level`] and [`pulse_integral`] evaluate them exactly, with no clock ticks.

use tempo_core::SimTime;

use crate::units::since_ms;

/// Read access to modulator concentrations at a neuron's position.
pub trait Modulators {
    /// Concentration of modulator `kind` at time `t`.
    fn level(&self, kind: u16, t: SimTime) -> f64;

    /// `∫ c(s)·exp(−(s − t0)/τ) ds` over `[t0, t1]`, in concentration·ms: used for three-factor learning, where the
    /// exponential is the eligibility trace decaying from `t0`.
    fn decayed_integral(&self, kind: u16, t0: SimTime, t1: SimTime, tau_ms: f64) -> f64;

    /// The clearance time constant of modulator `kind` in ms (for predicting direct-potential threshold crossings).
    fn clearance_ms(&self, kind: u16) -> f64;
}

/// No modulators: every concentration is zero.
#[derive(Copy, Clone, Debug, Default)]
pub struct NoModulators;

impl Modulators for NoModulators {
    fn level(&self, _: u16, _: SimTime) -> f64 {
        0.0
    }

    fn decayed_integral(&self, _: u16, _: SimTime, _: SimTime, _: f64) -> f64 {
        0.0
    }

    fn clearance_ms(&self, _: u16) -> f64 {
        f64::INFINITY
    }
}

/// Three-factor (reward-modulated) learning: STDP marks synapses, the modulator turns marks into weight changes.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ThreeFactor {
    /// Eligibility trace time constant `τ_e` in ms.
    pub eligibility_ms: f64,
    /// Learning rate: weight change per unit of eligibility × concentration × second.
    pub rate: f64,
}

/// One receptor on a neuron.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Receptor {
    /// Index of the modulator it responds to.
    pub modulator: u16,
    /// Excitatory-input gain coefficient `α_exc`.
    pub excitatory_gain: f64,
    /// Inhibitory-input gain coefficient `α_inh`.
    pub inhibitory_gain: f64,
    /// Threshold coefficient `α_θ`.
    pub threshold: f64,
    /// STDP coefficient `α_η`.
    pub plasticity: f64,
    /// Three-factor learning.
    pub three_factor: Option<ThreeFactor>,
    /// Direct potential per unit concentration, in mV.
    pub direct_mv: f64,
}

impl Receptor {
    /// A receptor with no effects, to fill in.
    pub fn new(modulator: u16) -> Self {
        Self {
            modulator,
            excitatory_gain: 0.0,
            inhibitory_gain: 0.0,
            threshold: 0.0,
            plasticity: 0.0,
            three_factor: None,
            direct_mv: 0.0,
        }
    }
}

/// The combined effect of a neuron's receptors at one moment.
#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) struct Effects {
    pub(crate) excitatory: f64,
    pub(crate) inhibitory: f64,
    pub(crate) threshold: f64,
    pub(crate) plasticity: f64,
    pub(crate) direct_mv: f64,
}

impl Effects {
    pub(crate) const NONE: Self = Self {
        excitatory: 1.0,
        inhibitory: 1.0,
        threshold: 1.0,
        plasticity: 1.0,
        direct_mv: 0.0,
    };

    pub(crate) fn at(receptors: &[Receptor], modulators: &dyn Modulators, t: SimTime) -> Self {
        let mut fx = Self::NONE;
        for r in receptors {
            let c = modulators.level(r.modulator, t);
            if c == 0.0 {
                continue;
            }
            let gain = |alpha: f64| (1.0 + alpha * c).max(0.0);
            fx.excitatory *= gain(r.excitatory_gain);
            fx.inhibitory *= gain(r.inhibitory_gain);
            fx.threshold *= gain(r.threshold);
            fx.plasticity *= gain(r.plasticity);
            fx.direct_mv += r.direct_mv * c;
        }
        fx
    }

    /// Scales a synaptic input by the gain for its sign.
    pub(crate) fn gate(&self, value: f64) -> f64 {
        if value >= 0.0 {
            value * self.excitatory
        } else {
            value * self.inhibitory
        }
    }
}

/// A modulator pulse at a neuron: arrives at `arrival` with `amplitude`, then decays with the modulator's clearance
/// time constant.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Pulse {
    /// When it reaches the neuron.
    pub arrival: SimTime,
    /// Concentration on arrival.
    pub amplitude: f64,
}

/// The concentration from `pulses` at time `t`.
pub fn pulse_level(pulses: impl IntoIterator<Item = Pulse>, clearance_ms: f64, t: SimTime) -> f64 {
    pulses
        .into_iter()
        .filter(|p| p.arrival <= t)
        .map(|p| p.amplitude * (-since_ms(t, p.arrival) / clearance_ms).exp())
        .sum()
}

/// `∫ c(s)·exp(−(s − t0)/τ) ds` over `[t0, t1]` for the concentration from `pulses`, exactly: each pulse
/// contributes `a·e^(−(s−tₐ)/τc)·e^(−(s−t0)/τ)` from `max(t0, tₐ)`, a single exponential with time constant
/// `(1/τc + 1/τ)⁻¹`.
pub fn pulse_integral(
    pulses: impl IntoIterator<Item = Pulse>,
    clearance_ms: f64,
    t0: SimTime,
    t1: SimTime,
    tau_ms: f64,
) -> f64 {
    if t1 <= t0 {
        return 0.0;
    }
    let combined = 1.0 / (1.0 / clearance_ms + 1.0 / tau_ms);
    pulses
        .into_iter()
        .filter(|p| p.arrival < t1)
        .map(|p| {
            let start = p.arrival.max(t0);
            let at_start = p.amplitude
                * (-since_ms(start, p.arrival) / clearance_ms).exp()
                * (-since_ms(start, t0) / tau_ms).exp();
            at_start * combined * (1.0 - (-since_ms(t1, start) / combined).exp())
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempo_core::SimDuration;

    fn ms(v: f64) -> SimTime {
        SimTime::from_start(SimDuration::from_millis_f64(v))
    }

    #[test]
    fn pulse_integral_matches_numerical_integration() {
        let pulses = [
            Pulse {
                arrival: ms(5.0),
                amplitude: 2.0,
            },
            Pulse {
                arrival: ms(40.0),
                amplitude: 1.0,
            },
        ];
        let (clearance, tau, t0, t1) = (30.0, 80.0, 10.0, 120.0);
        let exact = pulse_integral(pulses, clearance, ms(t0), ms(t1), tau);
        let steps = 200_000;
        let dt = (t1 - t0) / steps as f64;
        let numeric: f64 = (0..steps)
            .map(|i| {
                let s = t0 + (i as f64 + 0.5) * dt;
                pulse_level(pulses, clearance, ms(s)) * (-(s - t0) / tau).exp() * dt
            })
            .sum();
        // The midpoint rule errs by about amplitude·Δt/2 at the abrupt arrival of the second pulse (~2e-4 here).
        assert!((exact - numeric).abs() < 1e-5 * numeric, "{exact} vs {numeric}");
    }

    struct Constant(f64);
    impl Modulators for Constant {
        fn level(&self, _: u16, _: SimTime) -> f64 {
            self.0
        }
        fn decayed_integral(&self, _: u16, _: SimTime, _: SimTime, _: f64) -> f64 {
            0.0
        }
        fn clearance_ms(&self, _: u16) -> f64 {
            f64::INFINITY
        }
    }

    #[test]
    fn effects_combine_receptors() {
        let receptors = [
            Receptor {
                excitatory_gain: 0.5,
                threshold: -0.25,
                direct_mv: 2.0,
                ..Receptor::new(0)
            },
            Receptor {
                excitatory_gain: 1.0,
                inhibitory_gain: -2.0, // strongly suppresses inhibition; clamped at zero
                ..Receptor::new(1)
            },
        ];
        let fx = Effects::at(&receptors, &Constant(1.0), SimTime::ZERO);
        assert_eq!(fx.excitatory, 1.5 * 2.0);
        assert_eq!(fx.inhibitory, 0.0);
        assert_eq!(fx.threshold, 0.75);
        assert_eq!(fx.direct_mv, 2.0);
        assert_eq!(fx.gate(2.0), 6.0);
        assert_eq!(fx.gate(-2.0), 0.0);
        assert_eq!(Effects::at(&receptors, &NoModulators, SimTime::ZERO), Effects::NONE);
    }
}
