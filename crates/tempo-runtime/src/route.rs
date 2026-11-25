//! Fan-out routes: one send that reaches many actors, each after its own delay.

use tempo_core::{ActorId, SimDuration};

use crate::EdgeIdx;

/// One connection in a route.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct RouteEdge {
    /// The receiving actor.
    pub target: ActorId,
    /// Delay from the send time to delivery, e.g. axon propagation delay.
    pub delay: SimDuration,
    /// The receiver's index for this connection, passed to its `on_route_message`.
    pub edge: EdgeIdx,
}

/// Optional random variation added to every edge delay of a route (intentional timing noise).
///
/// The variation is a deterministic function of `(seed, sender, sequence number, edge position)`, so it is
/// reproducible and does not depend on where the route is expanded. Delays never go below zero.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Jitter {
    /// No variation.
    #[default]
    None,
    /// Uniform in `[-half_width, +half_width]`.
    Uniform {
        /// Half the width of the interval.
        half_width: SimDuration,
    },
    /// Normal with mean 0 and this standard deviation.
    Normal {
        /// Standard deviation.
        sigma: SimDuration,
    },
}

/// The longest delay a route edge can have: 2³² − 1 ns, about 4.29 s (edges are stored in 16 bytes).
pub const MAX_ROUTE_DELAY: SimDuration = SimDuration::from_nanos(u32::MAX as u64);

/// A route edge as stored: 16 bytes (a route can hold billions of them across a cluster).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct Edge {
    pub(crate) target: ActorId,
    delay_ns: u32,
    pub(crate) edge: EdgeIdx,
}

const _: () = assert!(size_of::<Edge>() == 16);

impl Edge {
    /// # Panics
    /// If the delay is longer than [`MAX_ROUTE_DELAY`].
    pub(crate) fn new(e: RouteEdge) -> Self {
        let delay_ns = u32::try_from(e.delay.as_nanos()).unwrap_or_else(|_| {
            panic!(
                "route edge to {} has a delay of {}, longer than the longest a route can hold ({MAX_ROUTE_DELAY})",
                e.target, e.delay
            )
        });
        Self {
            target: e.target,
            delay_ns,
            edge: e.edge,
        }
    }

    pub(crate) fn delay(self) -> SimDuration {
        SimDuration::from_nanos(u64::from(self.delay_ns))
    }
}

#[derive(Debug)]
pub(crate) struct Route {
    pub(crate) owner: ActorId,
    pub(crate) edges: Vec<Edge>,
    pub(crate) jitter: Jitter,
}

impl Jitter {
    /// How much the jitter can shorten a delay, for computing the lookahead. Normal jitter is unbounded, so this
    /// uses 5σ; the rare larger reductions are counted as lookahead violations.
    pub(crate) fn max_reduction(self) -> SimDuration {
        match self {
            Self::None => SimDuration::ZERO,
            Self::Uniform { half_width } => half_width,
            Self::Normal { sigma } => sigma.saturating_mul(5),
        }
    }

    /// Applies the jitter to `delay` for edge position `sub` of message `(src, seq)`.
    pub(crate) fn apply(self, delay: SimDuration, seed: u64, src: ActorId, seq: u64, sub: u32) -> SimDuration {
        let offset_ns = match self {
            Self::None => return delay,
            Self::Uniform { half_width } => {
                let u = unit_f64(mix(seed, src, seq, sub, 0));
                (2.0 * u - 1.0) * half_width.as_nanos() as f64
            }
            Self::Normal { sigma } => {
                // Box–Muller transform from two independent uniforms; 1 - u keeps the log argument in (0, 1].
                let u1 = 1.0 - unit_f64(mix(seed, src, seq, sub, 1));
                let u2 = unit_f64(mix(seed, src, seq, sub, 2));
                let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
                z * sigma.as_nanos() as f64
            }
        };
        let ns = (delay.as_nanos() as f64 + offset_ns).round().max(0.0);
        SimDuration::from_nanos(ns as u64)
    }
}

/// SplitMix64 finalizer: a fast, well-mixed hash of a `u64`.
pub(crate) fn splitmix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn mix(seed: u64, src: ActorId, seq: u64, sub: u32, stream: u64) -> u64 {
    let h = splitmix(seed ^ splitmix(src.raw()));
    let h = splitmix(h ^ seq);
    splitmix(h ^ (u64::from(sub) << 2 | stream))
}

/// A uniform value in `[0, 1)` from 53 random bits.
fn unit_f64(bits: u64) -> f64 {
    (bits >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jitter_is_deterministic_and_never_negative() {
        let j = Jitter::Normal {
            sigma: SimDuration::from_micros(50),
        };
        let d = SimDuration::from_micros(20);
        let a: Vec<_> = (0..1000).map(|i| j.apply(d, 7, ActorId::from_raw(3), i, 0)).collect();
        let b: Vec<_> = (0..1000).map(|i| j.apply(d, 7, ActorId::from_raw(3), i, 0)).collect();
        assert_eq!(a, b);
        assert!(
            a.contains(&SimDuration::ZERO),
            "large sigma should clamp some delays to zero"
        );
    }

    #[test]
    fn normal_jitter_has_the_requested_spread() {
        let sigma_ns = 1_000.0;
        let j = Jitter::Normal {
            sigma: SimDuration::from_nanos(sigma_ns as u64),
        };
        let base = SimDuration::from_millis(1);
        let n = 20_000;
        let samples: Vec<f64> = (0..n)
            .map(|i| j.apply(base, 1, ActorId::from_raw(0), i, 0).as_nanos() as f64 - base.as_nanos() as f64)
            .collect();
        let mean = samples.iter().sum::<f64>() / n as f64;
        let sd = (samples.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n as f64).sqrt();
        assert!(mean.abs() < 30.0, "mean {mean}");
        assert!((sd - sigma_ns).abs() < 30.0, "sd {sd}");
    }

    #[test]
    fn uniform_jitter_stays_in_bounds() {
        let half = SimDuration::from_micros(5);
        let j = Jitter::Uniform { half_width: half };
        let base = SimDuration::from_micros(100);
        for i in 0..5_000 {
            let d = j.apply(base, 9, ActorId::from_raw(1), i, 3);
            assert!(d >= base - half && d <= base + half);
        }
    }
}
