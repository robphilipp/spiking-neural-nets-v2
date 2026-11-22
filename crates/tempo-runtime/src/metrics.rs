//! Counters and latency histograms.

use std::time::Duration;

use tempo_core::SimDuration;

/// Counters describing what the runtime has done (summed over shards).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// Messages delivered to actors (direct and routed).
    pub delivered: u64,
    /// Of those, messages delivered through routes.
    pub routed_delivered: u64,
    /// Route edges sent (each fan-out send counts once per edge).
    pub routed_sends: u64,
    /// Messages sent to an actor on another shard.
    pub remote_sends: u64,
    /// Actors started.
    pub started: u64,
    /// Messages dropped because the recipient doesn't exist.
    pub dropped_unknown: u64,
    /// Synchronization windows processed (per shard, summed).
    pub windows: u64,
    /// Cross-shard messages sent with less delay than the lookahead. Each was delivered at the start of the next
    /// window instead, i.e. slightly late. Zero means the run was exact.
    pub lookahead_violations: u64,
    /// The largest such delay error.
    pub max_lookahead_violation: SimDuration,
}

impl Stats {
    pub(crate) fn merge(&mut self, other: &Self) {
        self.delivered += other.delivered;
        self.routed_delivered += other.routed_delivered;
        self.routed_sends += other.routed_sends;
        self.remote_sends += other.remote_sends;
        self.started += other.started;
        self.dropped_unknown += other.dropped_unknown;
        self.windows += other.windows;
        self.lookahead_violations += other.lookahead_violations;
        self.max_lookahead_violation = self.max_lookahead_violation.max(other.max_lookahead_violation);
    }
}

/// How well a paced run kept to its clock.
#[derive(Clone, Debug, Default)]
pub struct PacingStats {
    /// Windows started while the clock was running in real time.
    pub windows: u64,
    /// How late each window started relative to the wall-clock time its simulation time maps to. Small values
    /// mean the simulation tracked the clock; large ones mean it couldn't keep up.
    pub lateness: LatencyHistogram,
}

/// A histogram of durations: 1 µs resolution up to 1 ms, then one bucket per doubling.
#[derive(Clone, Debug)]
pub struct LatencyHistogram {
    fine: Vec<u64>,
    coarse: [u64; COARSE],
    count: u64,
    sum_ns: u128,
    max: Duration,
}

const FINE_US: usize = 1_000;
const COARSE: usize = 40;

impl Default for LatencyHistogram {
    fn default() -> Self {
        Self {
            fine: vec![0; FINE_US],
            coarse: [0; COARSE],
            count: 0,
            sum_ns: 0,
            max: Duration::ZERO,
        }
    }
}

impl LatencyHistogram {
    /// Adds one sample.
    pub fn record(&mut self, d: Duration) {
        let us = d.as_micros();
        if us < FINE_US as u128 {
            self.fine[us as usize] += 1;
        } else {
            let octave = 127 - (us / FINE_US as u128).leading_zeros() as usize;
            self.coarse[octave.min(COARSE - 1)] += 1;
        }
        self.count += 1;
        self.sum_ns += d.as_nanos();
        self.max = self.max.max(d);
    }

    /// Number of samples.
    pub fn count(&self) -> u64 {
        self.count
    }

    /// The largest sample.
    pub fn max(&self) -> Duration {
        self.max
    }

    /// The mean sample.
    pub fn mean(&self) -> Duration {
        if self.count == 0 {
            Duration::ZERO
        } else {
            Duration::from_nanos((self.sum_ns / u128::from(self.count)) as u64)
        }
    }

    /// An upper bound on the `q` quantile (e.g. `0.99`), to the bucket resolution.
    pub fn quantile(&self, q: f64) -> Duration {
        if self.count == 0 {
            return Duration::ZERO;
        }
        let target = ((q.clamp(0.0, 1.0) * self.count as f64).ceil() as u64).max(1);
        let mut seen = 0;
        for (us, &n) in self.fine.iter().enumerate() {
            seen += n;
            if seen >= target {
                return Duration::from_micros(us as u64 + 1).min(self.max);
            }
        }
        for (octave, &n) in self.coarse.iter().enumerate() {
            seen += n;
            if seen >= target {
                return Duration::from_micros((FINE_US as u64) << (octave + 1)).min(self.max);
            }
        }
        self.max
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantiles() {
        let mut h = LatencyHistogram::default();
        for us in 1..=100u64 {
            h.record(Duration::from_micros(us));
        }
        assert_eq!(h.count(), 100);
        assert_eq!(h.quantile(0.5), Duration::from_micros(51));
        assert_eq!(h.quantile(0.99), Duration::from_micros(100));
        assert_eq!(h.max(), Duration::from_micros(100));
        h.record(Duration::from_millis(5));
        assert_eq!(h.quantile(1.0), Duration::from_millis(5));
    }
}
