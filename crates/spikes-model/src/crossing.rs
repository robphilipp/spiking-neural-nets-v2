//! Predicting when a membrane potential will cross its threshold *between* events.
//!
//! Integrators only cross at an input, but some dynamics keep moving on their own: a resonator's damped
//! oscillation can carry it over threshold some time after the last input, and so can a slowly rising
//! direct-potential modulator. The model computes when that will happen, in closed form plus bisection, and the
//! neuron schedules one wake-up message for that moment. There are no clock ticks.

/// Bisection iterations: brackets shorter than a millisecond end below 1e-15 ms.
const ITERATIONS: usize = 80;

/// For the damped oscillation `f(t) = e^(b·t)·(y₀·cos ωt + x₀·sin ωt)` (the imaginary part of `z₀·e^((b+iω)t)`,
/// with `b ≤ 0` and `ω > 0`, in units of ms), returns the first `t > 0` at which `f(t) ≥ c`, if any.
///
/// The caller handles `f(0) ≥ c` (a crossing now). Because `b ≤ 0`, successive maxima never grow, so only the
/// first maximum after `t = 0` can reach `c`.
pub fn damped_oscillator(x0: f64, y0: f64, b: f64, omega: f64, c: f64) -> Option<f64> {
    debug_assert!(b <= 0.0 && omega > 0.0);
    let f = |t: f64| (b * t).exp() * (y0 * (omega * t).cos() + x0 * (omega * t).sin());
    if f(0.0) >= c {
        return Some(0.0);
    }
    // f'(t) = e^(bt)·R·cos(ωt − ψ), with R·cos ψ = b·y₀ + ω·x₀ and R·sin ψ = b·x₀ − ω·y₀.
    let psi = (b * x0 - omega * y0).atan2(b * y0 + omega * x0);
    // Maxima where ωt − ψ = π/2 + 2πk; take the first after t = 0.
    let two_pi = std::f64::consts::TAU;
    let mut phase = psi + std::f64::consts::FRAC_PI_2;
    phase -= two_pi * (phase / two_pi).floor(); // into [0, 2π)
    if phase <= 0.0 {
        phase += two_pi;
    }
    let t_max = phase / omega;
    if f(t_max) < c {
        return None;
    }
    // f rises monotonically from the preceding minimum (or from t = 0) to t_max.
    let lo = (t_max - std::f64::consts::PI / omega).max(0.0);
    Some(bisect(f, lo, t_max, c))
}

/// For `f(t) = u₀·e^(−t/τu) + m₀·e^(−t/τm)` (a decaying potential plus a decaying direct-potential modulator),
/// returns the first `t > 0` at which `f(t) ≥ c`, if any. Such a sum has at most one turning point.
pub fn two_exponentials(u0: f64, tau_u: f64, m0: f64, tau_m: f64, c: f64) -> Option<f64> {
    let f = |t: f64| u0 * (-t / tau_u).exp() + m0 * (-t / tau_m).exp();
    if f(0.0) >= c {
        return Some(0.0);
    }
    // f'(t) = 0 where e^(−t·(1/τu − 1/τm)) = −(m₀/τm)/(u₀/τu): needs opposite signs.
    let ratio = -(m0 / tau_m) / (u0 / tau_u);
    let rate = 1.0 / tau_u - 1.0 / tau_m;
    let turning = if ratio > 0.0 && rate != 0.0 {
        Some(-ratio.ln() / rate).filter(|t| *t > 0.0 && t.is_finite())
    } else {
        None
    };
    match turning {
        Some(t_star) if f(t_star) >= c => Some(bisect(f, 0.0, t_star, c)),
        Some(t_star) if c < 0.0 => {
            // A minimum below c, then f returns toward 0 > c: find where it climbs back up through c.
            let mut hi = t_star.max(1.0);
            while f(hi) < c {
                hi *= 2.0;
                if hi > 1e15 {
                    return None;
                }
            }
            Some(bisect(f, t_star, hi, c))
        }
        _ => None,
    }
}

/// The point in `[lo, hi]` where an increasing `f` reaches `c`, given `f(lo) < c ≤ f(hi)`.
fn bisect(f: impl Fn(f64) -> f64, mut lo: f64, mut hi: f64, c: f64) -> f64 {
    for _ in 0..ITERATIONS {
        let mid = 0.5 * (lo + hi);
        if f(mid) >= c {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    hi
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The first sample at or above `c`, scanning finely: a reference for the closed-form answers.
    fn scan(f: impl Fn(f64) -> f64, c: f64, horizon: f64) -> Option<f64> {
        let steps = 2_000_000;
        (1..=steps)
            .map(|i| horizon * i as f64 / steps as f64)
            .find(|&t| f(t) >= c)
    }

    #[test]
    fn oscillator_crossings_match_a_fine_scan() {
        let omega = std::f64::consts::TAU * 0.04; // 40 Hz, in rad/ms
        for (x0, y0, b, c) in [
            (3.0, 0.0, -0.01, 2.0),   // rising from zero
            (-3.0, 1.0, -0.01, 2.0),  // first falls, then rises
            (5.0, -4.0, -0.002, 4.0), // slow damping
            (1.0, 0.0, -0.05, 2.0),   // never reaches c
            (2.0, 2.0, 0.0, 2.5),     // undamped
        ] {
            let f = |t: f64| (b * t).exp() * (y0 * (omega * t).cos() + x0 * (omega * t).sin());
            assert!(f(0.0) < c, "test case must start below the threshold");
            let expected = scan(f, c, 100.0);
            let got = damped_oscillator(x0, y0, b, omega, c);
            match (expected, got) {
                (None, None) => {}
                (Some(e), Some(g)) => assert!((e - g).abs() < 1e-3, "{x0},{y0}: expected {e}, got {g}"),
                _ => panic!("{x0},{y0},{b},{c}: expected {expected:?}, got {got:?}"),
            }
        }
    }

    #[test]
    fn two_exponential_crossings_match_a_fine_scan() {
        for (u0, tu, m0, tm, c) in [
            (-6.0, 20.0, 9.0, 200.0, 5.0), // slow modulator lifts the potential over threshold
            (-6.0, 10.0, 8.0, 100.0, 3.0), // starts low, recovers
            (-6.0, 10.0, 8.0, 100.0, 7.0), // peaks below threshold
            (2.0, 20.0, 1.0, 200.0, 5.0),  // never
        ] {
            let f = |t: f64| u0 * (-t / tu).exp() + m0 * (-t / tm).exp();
            assert!(f(0.0) < c, "test case must start below the threshold");
            let expected = scan(f, c, 1_000.0);
            let got = two_exponentials(u0, tu, m0, tm, c);
            match (expected, got) {
                (None, None) => {}
                (Some(e), Some(g)) => assert!((e - g).abs() < 1e-2, "expected {e}, got {g}"),
                _ => panic!("({u0},{m0},{c}): expected {expected:?}, got {got:?}"),
            }
        }
    }
}
