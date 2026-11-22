//! Synapses: weights, their decay and limits, stochastic release, and STDP traces.
//!
//! A synapse lives with the **receiving** neuron (decision 4). Everything about it is brought up to date lazily,
//! when a signal arrives or the neuron fires, using the time since *this synapse's* last update (v1 used the time
//! since the neuron's last input from any synapse, discrepancy 7).

use rand_core::Rng;
use tempo_core::SimTime;

use crate::stdp::LearningRule;
use crate::units::{decay, since_ms, uniform, wiener};

/// Excitatory or inhibitory: a property of the synapse, so one neuron can have both kinds (decision 13).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Sign {
    /// Raises the receiver's membrane potential.
    Excitatory,
    /// Lowers it.
    Inhibitory,
}

impl Sign {
    /// `+1` or `-1`.
    pub fn factor(self) -> f64 {
        match self {
            Self::Excitatory => 1.0,
            Self::Inhibitory => -1.0,
        }
    }
}

/// How a weight relaxes toward its equilibrium between events (`WDF`).
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum WeightDecay {
    /// No decay (`fnc=zer`).
    None,
    /// `factor = exp(-Δt/τ)` (`fnc=exp, dhl=τ`; v1 calls τ the "half-life", but uses it as a time constant).
    Exponential {
        /// Time constant in ms.
        tau_ms: f64,
    },
}

impl WeightDecay {
    /// The fraction of the distance from equilibrium that remains after `dt_ms`.
    pub fn factor(self, dt_ms: f64) -> f64 {
        match self {
            Self::None => 1.0,
            Self::Exponential { tau_ms } => decay(dt_ms, tau_ms),
        }
    }
}

/// Bounds on a weight (`WLF`).
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum WeightLimit {
    /// No bounds (`fnc=unb`).
    Unbounded,
    /// Clamp to `[lower, upper]` (`fnc=bnd`).
    Bounded {
        /// Lower bound.
        lower: f64,
        /// Upper bound.
        upper: f64,
    },
}

impl WeightLimit {
    /// The weight clamped to the bounds.
    pub fn apply(self, w: f64) -> f64 {
        match self {
            Self::Unbounded => w,
            Self::Bounded { lower, upper } => w.clamp(lower, upper),
        }
    }

    /// `(lower, upper)`, infinite when unbounded.
    pub fn bounds(self) -> (f64, f64) {
        match self {
            Self::Unbounded => (f64::NEG_INFINITY, f64::INFINITY),
            Self::Bounded { lower, upper } => (lower, upper),
        }
    }
}

/// Weight stickiness (`CST`): slows the decay toward equilibrium after long gaps between updates, so learned
/// weights are retained (v1 parsed this but never applied it, discrepancy 6). The adjusted decay factor is
/// `d + (1 − d)·s(Δt)` with `s(Δt) = 2·(1/(1 + exp(−Δt/τ)) − ½)`, as v1 defined it.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Stickiness {
    /// No stickiness (`fnc=none`).
    None,
    /// Sigmoid stickiness (`fnc=sig, dhl=τ`).
    Sigmoid {
        /// Time constant in ms.
        tau_ms: f64,
    },
}

impl Stickiness {
    /// The decay factor after stickiness.
    pub fn adjust(self, decay_factor: f64, dt_ms: f64) -> f64 {
        match self {
            Self::None => decay_factor,
            Self::Sigmoid { tau_ms } => {
                let s = 2.0 * (1.0 / (1.0 + (-dt_ms.max(0.0) / tau_ms).exp()) - 0.5);
                decay_factor + (1.0 - decay_factor) * s
            }
        }
    }
}

/// Stochastic release (`SRP`), after Maass: `p = 1 − exp(−C·V)` with facilitation
/// `C = C₀ + α·Σ exp(−(t−tᵢ)/τc)` over earlier action potentials and depletion
/// `V = max(0, V₀ − β·Σ exp(−(t−tⱼ)/τv))` over earlier releases.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Release {
    /// `C₀` (`fcb`).
    pub facilitation_base: f64,
    /// `α` (`fcm`).
    pub facilitation_magnitude: f64,
    /// `τc` in ms (`fct`).
    pub facilitation_tau_ms: f64,
    /// `V₀` (`dpb`).
    pub depletion_base: f64,
    /// `β` (`dpm`).
    pub depletion_magnitude: f64,
    /// `τv` in ms (`dpt`).
    pub depletion_tau_ms: f64,
}

impl Release {
    /// v1's defaults: `C₀ = 5, α = 0.1, τc = 100 ms, V₀ = 10, β = 1, τv = 100 ms`.
    pub const V1_DEFAULT: Self = Self {
        facilitation_base: 5.0,
        facilitation_magnitude: 0.1,
        facilitation_tau_ms: 100.0,
        depletion_base: 10.0,
        depletion_magnitude: 1.0,
        depletion_tau_ms: 100.0,
    };
}

/// Parameters shared by a class of synapses (indexed by [`Synapse::params`]).
#[derive(Clone, Debug, PartialEq)]
pub struct SynapseParams {
    /// Decay toward equilibrium.
    pub decay: WeightDecay,
    /// Weight bounds.
    pub limit: WeightLimit,
    /// Stickiness.
    pub stickiness: Stickiness,
    /// Stochastic release; `None` always releases.
    pub release: Option<Release>,
    /// Learning rule.
    pub learning: LearningRule,
    /// Weight noise magnitude `Ω_w` (Wiener process, per √ms; `wnm`).
    pub weight_noise: f64,
}

impl SynapseParams {
    /// A synapse that never changes and always releases.
    pub fn fixed() -> Self {
        Self {
            decay: WeightDecay::None,
            limit: WeightLimit::Unbounded,
            stickiness: Stickiness::None,
            release: None,
            learning: LearningRule::None,
            weight_noise: 0.0,
        }
    }
}

/// One incoming synapse's state.
#[derive(Clone, Debug, PartialEq)]
pub struct Synapse {
    /// Current weight (efficacy).
    pub weight: f32,
    /// The weight it relaxes toward.
    pub equilibrium: f32,
    /// Excitatory or inhibitory.
    pub sign: Sign,
    /// Index of its [`SynapseParams`].
    pub params: u16,
    /// When this synapse was last brought up to date.
    pub last: SimTime,
    /// Facilitation sum `Σ exp(−(t−tᵢ)/τc)·α` (without the base).
    pub facilitation: f32,
    /// Depletion sum `Σ exp(−(t−tⱼ)/τv)·β` (without the base).
    pub depletion: f32,
    /// STDP traces `[T0, T1]` since the receiver's last spike.
    pub trace: [f32; 2],
    /// Signals since the receiver's last spike.
    pub count: u32,
}

impl Synapse {
    /// A new synapse at rest.
    pub fn new(weight: f64, equilibrium: f64, sign: Sign, params: u16) -> Self {
        Self {
            weight: weight as f32,
            equilibrium: equilibrium as f32,
            sign,
            params,
            last: SimTime::ZERO,
            facilitation: 0.0,
            depletion: 0.0,
            trace: [0.0, 0.0],
            count: 0,
        }
    }

    /// The signed weight.
    pub fn efficacy(&self) -> f64 {
        self.sign.factor() * f64::from(self.weight)
    }

    /// Brings the synapse up to `now`: weight noise and decay toward equilibrium, release sums and STDP traces.
    pub fn advance<R: Rng + ?Sized>(&mut self, p: &SynapseParams, now: SimTime, rng: &mut R) {
        let dt = since_ms(now, self.last);
        if dt <= 0.0 {
            return;
        }
        // Weight: w ← w_eq + (w + noise − w_eq)·d, then bounded (paper eq. for w(t), without the learning term).
        let factor = p.stickiness.adjust(p.decay.factor(dt), dt);
        let noise = wiener(p.weight_noise, dt, rng);
        let eq = f64::from(self.equilibrium);
        let w = eq + (f64::from(self.weight) + noise - eq) * factor;
        self.weight = p.limit.apply(w) as f32;
        // Release sums decay with their own time constants.
        if let Some(r) = &p.release {
            self.facilitation *= decay(dt, r.facilitation_tau_ms) as f32;
            self.depletion *= decay(dt, r.depletion_tau_ms) as f32;
        }
        // STDP traces: every signal is dt older. T1 = Σ d·e^(−d/τ) needs T0 before it decays.
        if self.count > 0 {
            let tau = p.learning.trace_time_constant_ms();
            let k = decay(dt, tau);
            let [t0, t1] = self.trace;
            self.trace = [t0 * k as f32, ((f64::from(t1) + dt * f64::from(t0)) * k) as f32];
        }
        self.last = now;
    }

    /// The release probability for an action potential arriving now (call after [`Self::advance`]). Uses only
    /// earlier action potentials and releases, as in Maass' definition.
    pub fn release_probability(&self, p: &SynapseParams) -> f64 {
        match &p.release {
            None => 1.0,
            Some(r) => {
                let c = r.facilitation_base + f64::from(self.facilitation);
                let v = (r.depletion_base - f64::from(self.depletion)).max(0.0);
                1.0 - (-c * v).exp()
            }
        }
    }

    /// An action potential arrives at `now`: brings the synapse up to date, decides whether it releases
    /// (drawing from `rng` only when release is stochastic), and updates facilitation and depletion.
    pub fn arrive<R: Rng + ?Sized>(&mut self, p: &SynapseParams, now: SimTime, rng: &mut R) -> bool {
        self.advance(p, now, rng);
        let Some(r) = &p.release else {
            return true;
        };
        let released = uniform(rng) < self.release_probability(p);
        self.facilitation += r.facilitation_magnitude as f32;
        if released {
            self.depletion += r.depletion_magnitude as f32;
        }
        released
    }

    /// Records a released signal for STDP potentiation at the receiver's next spike (d = 0: adds 1 to T0).
    pub fn record_signal(&mut self) {
        self.trace[0] += 1.0;
        self.count += 1;
    }

    /// Clears the STDP traces (after the receiver fires).
    pub fn reset_traces(&mut self) {
        self.trace = [0.0, 0.0];
        self.count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_core::SeedableRng;
    use tempo_core::SimDuration;

    fn at_ms(ms: u64) -> SimTime {
        SimTime::from_start(SimDuration::from_millis(ms))
    }

    #[test]
    fn weight_decays_toward_equilibrium_by_its_own_elapsed_time() {
        let p = SynapseParams {
            decay: WeightDecay::Exponential { tau_ms: 100.0 },
            ..SynapseParams::fixed()
        };
        let mut s = Synapse::new(3.0, 1.0, Sign::Excitatory, 0);
        let mut rng = rand_xoshiro::Xoshiro256PlusPlus::seed_from_u64(0);
        s.advance(&p, at_ms(100), &mut rng);
        let expected = 1.0 + 2.0 * (-1.0f64).exp();
        assert!((f64::from(s.weight) - expected).abs() < 1e-6);
    }

    #[test]
    fn limits_and_stickiness() {
        assert_eq!(WeightLimit::Bounded { lower: 0.0, upper: 5.0 }.apply(7.0), 5.0);
        let sticky = Stickiness::Sigmoid { tau_ms: 10.0 };
        assert_eq!(sticky.adjust(0.5, 0.0), 0.5, "no effect for a zero gap");
        assert!(sticky.adjust(0.5, 1_000.0) > 0.999, "long gaps: almost no decay");
    }

    #[test]
    fn release_probability_follows_facilitation_and_depletion() {
        let p = SynapseParams {
            release: Some(Release {
                facilitation_base: 0.1,
                facilitation_magnitude: 0.5,
                facilitation_tau_ms: 50.0,
                depletion_base: 1.0,
                depletion_magnitude: 0.5,
                depletion_tau_ms: 200.0,
            }),
            ..SynapseParams::fixed()
        };
        let mut s = Synapse::new(1.0, 1.0, Sign::Excitatory, 0);
        let p0 = s.release_probability(&p);
        assert!(
            (p0 - (1.0 - (-0.1f64).exp())).abs() < 1e-12,
            "resting probability 1 − e^(−C₀V₀)"
        );
        // Facilitation without release raises the probability.
        s.facilitation = 0.5;
        let p1 = s.release_probability(&p);
        assert!(p1 > p0);
        // Depletion after a release lowers it.
        s.depletion = 0.5;
        assert!(s.release_probability(&p) < p1);
        // Both decay back toward rest.
        let mut rng = rand_xoshiro::Xoshiro256PlusPlus::seed_from_u64(0);
        s.advance(&p, at_ms(10_000), &mut rng);
        assert!((s.release_probability(&p) - p0).abs() < 1e-6);
    }

    #[test]
    fn traces_decay_exactly() {
        let p = SynapseParams {
            learning: LearningRule::HardLimit(crate::stdp::ExponentialStdp {
                inhibition_amplitude: 0.0,
                inhibition_period_ms: 10.0,
                excitation_amplitude: 0.0,
                excitation_period_ms: 10.0,
            }),
            ..SynapseParams::fixed()
        };
        let mut rng = rand_xoshiro::Xoshiro256PlusPlus::seed_from_u64(0);
        let mut s = Synapse::new(1.0, 1.0, Sign::Excitatory, 0);
        s.record_signal(); // at t = 0
        s.advance(&p, at_ms(5), &mut rng);
        s.record_signal(); // at t = 5
        s.advance(&p, at_ms(12), &mut rng);
        // Signals are 12 ms and 7 ms old.
        let t0 = (-1.2f64).exp() + (-0.7f64).exp();
        let t1 = 12.0 * (-1.2f64).exp() + 7.0 * (-0.7f64).exp();
        assert!((f64::from(s.trace[0]) - t0).abs() < 1e-6);
        assert!((f64::from(s.trace[1]) - t1).abs() < 1e-5);
        assert_eq!(s.count, 2);
    }
}
