//! Units and small numeric helpers.
//!
//! The model works in plain `f64` with fixed units: **millivolts** for potentials, **milliseconds** for times
//! and time constants, **hertz** for rates, and dimensionless weights. Event times come from the runtime as
//! [`SimTime`] (exact nanoseconds); the model converts the time between two events to milliseconds once.

use rand_core::Rng;
use tempo_core::{SimDuration, SimTime};

/// Milliseconds from `then` to `now` (zero if `then` is later).
pub fn since_ms(now: SimTime, then: SimTime) -> f64 {
    now.saturating_duration_since(then).as_millis_f64()
}

/// A duration from milliseconds, rounded to the nearest nanosecond; negative values become zero and values too
/// large for a `SimDuration` become `SimDuration::MAX`.
pub fn duration_ms(ms: f64) -> SimDuration {
    if ms.is_nan() || ms <= 0.0 {
        SimDuration::ZERO
    } else {
        SimDuration::try_from_millis_f64(ms).unwrap_or(SimDuration::MAX)
    }
}

/// `exp(-dt / tau)`, treating a non-positive or infinite `tau` as "no decay".
pub fn decay(dt_ms: f64, tau_ms: f64) -> f64 {
    if tau_ms.is_finite() && tau_ms > 0.0 {
        (-dt_ms / tau_ms).exp()
    } else {
        1.0
    }
}

/// A uniform sample in `[0, 1)`.
pub fn uniform<R: Rng + ?Sized>(rng: &mut R) -> f64 {
    (rng.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
}

/// A standard normal sample (Box–Muller).
pub fn standard_normal<R: Rng + ?Sized>(rng: &mut R) -> f64 {
    // Shift u1 into (0, 1] so the logarithm is finite.
    let u1 = 1.0 - uniform(rng);
    let u2 = uniform(rng);
    (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
}

/// Wiener-process noise accumulated over `dt_ms`: `sigma · sqrt(dt / 1 ms) · N(0, 1)`. Draws nothing (so the
/// random stream is untouched) when `sigma` is zero.
pub fn wiener<R: Rng + ?Sized>(sigma: f64, dt_ms: f64, rng: &mut R) -> f64 {
    if sigma == 0.0 || dt_ms <= 0.0 {
        0.0
    } else {
        sigma * dt_ms.sqrt() * standard_normal(rng)
    }
}

/// The Heaviside step: 1 for positive values, 0 otherwise (as in v1).
pub fn heaviside(x: f64) -> f64 {
    if x > 0.0 { 1.0 } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_core::SeedableRng;

    #[test]
    fn conversions() {
        assert_eq!(duration_ms(1.5), SimDuration::from_micros(1_500));
        assert_eq!(duration_ms(-3.0), SimDuration::ZERO);
        assert_eq!(duration_ms(f64::INFINITY), SimDuration::MAX);
        assert_eq!(
            since_ms(SimTime::from_nanos(2_500_000), SimTime::from_nanos(500_000)),
            2.0
        );
        assert_eq!(decay(10.0, f64::INFINITY), 1.0);
        assert!((decay(10.0, 10.0) - (-1.0f64).exp()).abs() < 1e-15);
    }

    #[test]
    fn normal_samples_have_unit_variance() {
        let mut rng = rand_xoshiro::Xoshiro256PlusPlus::seed_from_u64(1);
        let n = 100_000;
        let xs: Vec<f64> = (0..n).map(|_| standard_normal(&mut rng)).collect();
        let mean = xs.iter().sum::<f64>() / n as f64;
        let var = xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n as f64;
        assert!(mean.abs() < 0.01 && (var - 1.0).abs() < 0.02, "mean {mean} var {var}");
    }
}
