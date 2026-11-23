//! Spike-timing-dependent plasticity (STDP), following v1's learning functions (`dna-defs.md`, `LRN=[...]`).
//!
//! **Potentiation** happens when the post-synaptic neuron fires: every pre-synaptic signal received since its
//! previous spike strengthens the synapse, by an amount that shrinks with how long before the spike it arrived.
//! **Depression** happens when a signal arrives after a spike without causing a new one: the synapse weakens, by
//! an amount that shrinks with how long after the spike the signal arrived.
//!
//! v1 kept a list of every signal since the last spike and summed over it. Here each synapse keeps exponential
//! **traces** instead, updated as signals arrive and reset at each spike:
//!
//! - `T0 = Σ exp(-dᵢ/τ)` and `T1 = Σ dᵢ·exp(-dᵢ/τ)`, where `dᵢ` is how long ago signal `i` arrived, plus a count.
//!
//! For the hard- and soft-limit rules, `A·T0` is *exactly* v1's sum. For the alpha rule, `T0`, `T1` and the count
//! give its sum exactly too, except that v1 clamps each term at the baseline and here the clamp is applied to the
//! total (they differ only when the alpha function's zero offset is negative and signals arrive very close to the
//! spike). The traces use constant memory per synapse, however busy the neuron.

use crate::units::heaviside;

/// How a synapse learns.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum LearningRule {
    /// No learning (`fnc=flat`).
    None,
    /// STDP with hard weight limits (`fnc=stdp_hard`): changes stop at the bounds.
    HardLimit(ExponentialStdp),
    /// STDP with soft weight limits (`fnc=stdp_soft`): changes shrink as the weight nears a bound.
    SoftLimit(ExponentialStdp),
    /// Alpha-function learning (`fnc=stdp_alpha`).
    Alpha(AlphaStdp),
}

/// Parameters of the exponential STDP windows.
#[derive(Copy, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ExponentialStdp {
    /// Depression amplitude, `ina`.
    pub inhibition_amplitude: f64,
    /// Depression time constant in ms, `inp`.
    pub inhibition_period_ms: f64,
    /// Potentiation amplitude, `exa`.
    pub excitation_amplitude: f64,
    /// Potentiation time constant in ms, `exp`.
    pub excitation_period_ms: f64,
}

/// Alpha-function learning: `α(t) = max(β, e·(1−β)·x·e^(−x) + β)` with `x = (t₀ − t)/τ`, offset by `t₀` so that
/// `α(0) = 0`.
#[derive(Copy, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlphaStdp {
    /// Baseline `β`.
    pub baseline: f64,
    /// Time constant `τ` in ms.
    pub time_constant_ms: f64,
    /// Learning rate `η`.
    pub learning_rate: f64,
    /// The offset `t₀` (ms) at which `α(0) = 0`.
    zero_offset_ms: f64,
}

impl AlphaStdp {
    /// An alpha rule; computes the zero offset.
    pub fn new(baseline: f64, time_constant_ms: f64, learning_rate: f64) -> Self {
        Self {
            baseline,
            time_constant_ms,
            learning_rate,
            zero_offset_ms: time_constant_ms * alpha_zero(baseline),
        }
    }

    /// The zero offset `t₀` in ms.
    pub fn zero_offset_ms(&self) -> f64 {
        self.zero_offset_ms
    }

    /// `α(t)`.
    pub fn alpha(&self, t_ms: f64) -> f64 {
        let x = (self.zero_offset_ms - t_ms) / self.time_constant_ms;
        let beta = self.baseline;
        (std::f64::consts::E * (1.0 - beta) * x * (-x).exp() + beta).max(beta)
    }
}

/// Solves `e·(1−β)·y·e^(−y) + β = 0` for `y` (the zero offset in units of τ), by bisection.
fn alpha_zero(beta: f64) -> f64 {
    if beta >= 1.0 || beta == 0.0 {
        return 0.0;
    }
    let g = |y: f64| std::f64::consts::E * (1.0 - beta) * y * (-y).exp() + beta;
    // For β > 0 the root is negative; for β < 0 it lies in (0, 1].
    let (mut lo, mut hi) = if beta > 0.0 { (-50.0, 0.0) } else { (0.0, 1.0) };
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if (g(mid) > 0.0) == (g(hi) > 0.0) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    0.5 * (lo + hi)
}

impl LearningRule {
    /// The time constant of this rule's potentiation trace, in ms (infinite for no learning).
    pub fn trace_time_constant_ms(&self) -> f64 {
        match self {
            Self::None => f64::INFINITY,
            Self::HardLimit(p) | Self::SoftLimit(p) => p.excitation_period_ms,
            Self::Alpha(a) => a.time_constant_ms,
        }
    }

    /// `true` if this rule never changes weights.
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    /// Potentiation when the post-synaptic neuron fires, from the synapse's traces (see the module docs).
    /// `t0`, `t1` are the traces at the spike time, `count` the number of signals since the previous spike.
    pub fn potentiation(&self, t0: f64, t1: f64, count: u32, weight: f64, max: f64) -> f64 {
        match self {
            Self::None => 0.0,
            Self::HardLimit(p) => heaviside(max - weight) * p.excitation_amplitude * t0,
            Self::SoftLimit(p) => soft_room(max - weight) * p.excitation_amplitude * t0,
            Self::Alpha(a) => {
                let tau = a.time_constant_ms;
                let t_zero = a.zero_offset_ms;
                let beta = a.baseline;
                // Σ [e·(1−β)·xᵢ·e^(−xᵢ) + β] with xᵢ = (t₀ + dᵢ)/τ
                //   = e·(1−β)/τ · e^(−t₀/τ) · (t₀·T0 + T1) + β·count
                let sum = std::f64::consts::E * (1.0 - beta) / tau * (-t_zero / tau).exp() * (t_zero * t0 + t1)
                    + beta * f64::from(count);
                (a.learning_rate * heaviside(max - weight) * sum.max(beta * f64::from(count))).max(0.0)
            }
        }
    }

    /// Depression when a signal arrives `delta_ms` after the post-synaptic neuron's last spike.
    pub fn depression(&self, delta_ms: f64, weight: f64, min: f64) -> f64 {
        match self {
            Self::None => 0.0,
            Self::HardLimit(p) => {
                -heaviside(weight - min) * p.inhibition_amplitude * (-delta_ms / p.inhibition_period_ms).exp()
            }
            Self::SoftLimit(p) => {
                -soft_room(weight - min) * p.inhibition_amplitude * (-delta_ms / p.inhibition_period_ms).exp()
            }
            Self::Alpha(a) => {
                (a.learning_rate * heaviside(weight - min) * a.alpha(delta_ms) * (-delta_ms / a.time_constant_ms).exp())
                    .min(0.0)
            }
        }
    }
}

/// How much room a soft-limited weight has before its bound. With an infinite bound (an unbounded weight), the
/// factor is 1, so soft limits behave like hard limits (v1 multiplied by `f64::MAX` here).
fn soft_room(distance: f64) -> f64 {
    if distance.is_finite() { distance.max(0.0) } else { 1.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exp_rule() -> ExponentialStdp {
        ExponentialStdp {
            inhibition_amplitude: 0.035,
            inhibition_period_ms: 10.0,
            excitation_amplitude: 0.015,
            excitation_period_ms: 10.0,
        }
    }

    #[test]
    fn hard_limit_matches_v1_formulas() {
        let rule = LearningRule::HardLimit(exp_rule());
        // One signal 5 ms before the spike: T0 = e^(-5/10).
        let t0 = (-0.5f64).exp();
        assert!((rule.potentiation(t0, 0.0, 1, 1.0, 5.0) - 0.015 * t0).abs() < 1e-15);
        assert_eq!(rule.potentiation(t0, 0.0, 1, 5.0, 5.0), 0.0, "at the upper bound");
        let dep = rule.depression(5.0, 1.0, 0.0);
        assert!((dep + 0.035 * (-0.5f64).exp()).abs() < 1e-15);
        assert_eq!(rule.depression(5.0, 0.0, 0.0), 0.0, "at the lower bound");
    }

    #[test]
    fn soft_limit_scales_with_room_to_the_bound() {
        let rule = LearningRule::SoftLimit(exp_rule());
        assert!((rule.potentiation(1.0, 0.0, 1, 4.0, 5.0) - 0.015).abs() < 1e-15);
        assert!((rule.potentiation(1.0, 0.0, 1, 3.0, 5.0) - 0.03).abs() < 1e-15);
        assert!((rule.depression(0.0, 2.0, 0.0) + 0.07).abs() < 1e-15);
        assert_eq!(rule.potentiation(1.0, 0.0, 1, 3.0, f64::INFINITY), 0.015);
    }

    #[test]
    fn alpha_zero_offset_makes_alpha_vanish_at_zero() {
        for beta in [-0.5, -0.1, 0.1, 0.3, 0.6] {
            let a = AlphaStdp::new(beta, 20.0, 1.0);
            assert!(
                a.alpha(0.0).abs() < 1e-9 || a.alpha(0.0) == beta.max(0.0),
                "β={beta}: α(0)={}",
                a.alpha(0.0)
            );
        }
    }

    #[test]
    fn alpha_potentiation_matches_the_direct_sum_when_unclamped() {
        // A negative baseline gives a non-negative zero offset, where the trace formula is exact.
        let a = AlphaStdp::new(-0.2, 15.0, 0.5);
        assert!(a.zero_offset_ms() >= 0.0);
        let rule = LearningRule::Alpha(a);
        let ds: [f64; 4] = [1.0, 4.0, 9.5, 22.0];
        let tau = a.time_constant_ms;
        let t0: f64 = ds.iter().map(|d| (-d / tau).exp()).sum();
        let t1: f64 = ds.iter().map(|d| d * (-d / tau).exp()).sum();
        let direct: f64 = ds.iter().map(|d| (a.learning_rate * a.alpha(-d)).max(0.0)).sum();
        let traced = rule.potentiation(t0, t1, ds.len() as u32, 1.0, 5.0);
        assert!((traced - direct).abs() < 1e-12, "{traced} vs {direct}");
    }
}
