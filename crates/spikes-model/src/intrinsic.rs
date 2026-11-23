//! Intrinsic plasticity: an excitability offset `b` (mV) added to the membrane potential when testing against the
//! threshold.
//!
//! - **LTP-IE** (as in v1): each spike raises `b` by `η·exp(−(b + b₀))`; between spikes `b` decays with time
//!   constant `τ_b`.
//! - **Homeostasis** (new, optional): the neuron keeps a firing-rate estimate `r(t)` (an exponential trace of its
//!   spikes with window `τ_r`) and `b` is pushed toward the value that keeps `r` at a target: sustained firing
//!   above the target lowers excitability, inactivity raises it. `db/dt = κ·(r* − r(t))`, integrated exactly
//!   between events (no ticks):
//!   `∫₀^Δ (r* − r₀·e^(−s/τ_r)) ds = r*·Δ − r₀·τ_r·(1 − e^(−Δ/τ_r))`.

use crate::units::decay;

/// Intrinsic-plasticity parameters (`ipb`, `ipl`, `ipd`, plus the optional homeostasis).
#[derive(Copy, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct IntrinsicParams {
    /// `b₀` in mV (`ipb`).
    pub base_mv: f64,
    /// Learning rate `η` in mV (`ipl`). Zero turns LTP-IE off.
    pub learning_mv: f64,
    /// Decay time constant `τ_b` in ms (`ipd`).
    pub decay_ms: f64,
    /// Homeostatic regulation; `None` keeps v1's LTP-IE only.
    pub homeostasis: Option<Homeostasis>,
}

impl IntrinsicParams {
    /// No intrinsic plasticity.
    pub const OFF: Self = Self {
        base_mv: 0.0,
        learning_mv: 0.0,
        decay_ms: f64::INFINITY,
        homeostasis: None,
    };
}

/// Homeostatic intrinsic plasticity.
#[derive(Copy, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Homeostasis {
    /// Target firing rate `r*` in Hz.
    pub target_hz: f64,
    /// Regulation strength `κ` in mV per (Hz·s): how fast `b` moves per unit of rate error.
    pub strength: f64,
    /// Rate-estimate window `τ_r` in ms.
    pub window_ms: f64,
    /// Lower bound on `b` in mV.
    pub min_mv: f64,
    /// Upper bound on `b` in mV.
    pub max_mv: f64,
}

impl IntrinsicParams {
    /// Advances `b` and the rate estimate over `dt_ms` without spikes.
    pub fn advance(&self, b: &mut f64, rate_hz: &mut f64, dt_ms: f64) {
        if dt_ms <= 0.0 {
            return;
        }
        // (Zero stays zero: skip the exp when intrinsic plasticity hasn't moved `b`.)
        if *b != 0.0 {
            *b *= decay(dt_ms, self.decay_ms);
        }
        if let Some(h) = &self.homeostasis {
            let window_s = h.window_ms / 1e3;
            let integral = h.target_hz * dt_ms / 1e3 - *rate_hz * window_s * (1.0 - decay(dt_ms, h.window_ms));
            *b = (*b + h.strength * integral).clamp(h.min_mv, h.max_mv);
            *rate_hz *= decay(dt_ms, h.window_ms);
        }
    }

    /// Updates `b` and the rate estimate for a spike.
    pub fn on_spike(&self, b: &mut f64, rate_hz: &mut f64) {
        if self.learning_mv != 0.0 {
            *b += self.learning_mv * (-(*b + self.base_mv)).exp();
        }
        if let Some(h) = &self.homeostasis {
            *rate_hz += 1e3 / h.window_ms;
            *b = b.clamp(h.min_mv, h.max_mv);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ltp_ie_matches_v1() {
        let p = IntrinsicParams {
            base_mv: 0.5,
            learning_mv: 0.01,
            decay_ms: 1_000.0,
            homeostasis: None,
        };
        let (mut b, mut r) = (0.2, 0.0);
        p.on_spike(&mut b, &mut r);
        assert!((b - (0.2 + 0.01 * (-0.7f64).exp())).abs() < 1e-15);
        let before = b;
        p.advance(&mut b, &mut r, 1_000.0);
        assert!((b - before * (-1.0f64).exp()).abs() < 1e-15);
    }

    fn homeostatic() -> IntrinsicParams {
        IntrinsicParams {
            base_mv: 0.0,
            learning_mv: 0.0,
            decay_ms: f64::INFINITY,
            homeostasis: Some(Homeostasis {
                target_hz: 10.0,
                strength: 0.5,
                window_ms: 1_000.0,
                min_mv: -5.0,
                max_mv: 5.0,
            }),
        }
    }

    /// Fires regularly at `hz` for `seconds`, starting with the rate estimate already at `hz` (its steady state),
    /// and returns the final `b`. The rate is imposed, not affected by `b`, so this checks the regulation signal
    /// alone.
    fn drive(p: &IntrinsicParams, hz: f64, seconds: f64) -> f64 {
        let (mut b, mut r) = (0.0, hz);
        let step = 1e3 / hz;
        let mut t = 0.0;
        while t < seconds * 1e3 {
            p.advance(&mut b, &mut r, step);
            p.on_spike(&mut b, &mut r);
            t += step;
        }
        b
    }

    #[test]
    fn homeostasis_lowers_excitability_when_firing_fast_and_raises_it_when_quiet() {
        let p = homeostatic();
        assert!(drive(&p, 50.0, 2.0) < -1.0, "firing above the target lowers b");
        assert!(
            drive(&p, 10.0, 10.0).abs() < 0.5,
            "firing at the target holds b near where it was"
        );
        // Silence: b rises at κ·r* per second until it hits the bound.
        let (mut b, mut r) = (0.0, 0.0);
        p.advance(&mut b, &mut r, 400.0);
        assert!((b - 0.5 * 10.0 * 0.4).abs() < 1e-12);
        p.advance(&mut b, &mut r, 10_000.0);
        assert_eq!(b, 5.0, "clamped at the upper bound");
    }

    #[test]
    fn homeostasis_integration_is_exact_across_splits() {
        let p = homeostatic();
        let (mut b1, mut r1) = (0.3, 25.0);
        p.advance(&mut b1, &mut r1, 700.0);
        let (mut b2, mut r2) = (0.3, 25.0);
        for _ in 0..7 {
            p.advance(&mut b2, &mut r2, 100.0);
        }
        assert!((b1 - b2).abs() < 1e-12 && (r1 - r2).abs() < 1e-12);
    }
}
