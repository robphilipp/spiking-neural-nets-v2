//! Turning a channel's samples into neuron input: shared by sensors (samples streamed in) and signal sources
//! (samples computed in simulated time).

use serde::{Deserialize, Serialize};

/// How samples become input events.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SampleEncoding {
    /// Each sample `v` sends `gain_mv × v` (zero samples send nothing).
    Current {
        /// mV per unit of sample value.
        gain_mv: f32,
    },
    /// A Poisson process at `clamp(v, 0, 1) × max_hz`, held until the next sample; each event sends `mv`. Event
    /// times are exact within each sample interval.
    Rate {
        /// Rate at `v = 1`.
        max_hz: f64,
        /// Input per event.
        mv: f32,
    },
    /// Delta modulation: each time the value has risen by `level` since the last event, `+mv`; fallen, `−mv`.
    Threshold {
        /// Change per event.
        level: f32,
        /// Input per event.
        mv: f32,
    },
}

/// One channel's encoder state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Channel {
    /// Rate encoding: the remaining exponential "mass" until the next event.
    need: f64,
    /// Threshold encoding: the reference level.
    reference: Option<f32>,
}

/// An exponential(1) draw from a uniform in [0, 1).
fn exp1(uniform: &mut dyn FnMut() -> f64) -> f64 {
    -(1.0 - uniform()).ln()
}

impl Channel {
    /// A fresh channel (draws its first exponential from `uniform`).
    pub fn new(uniform: &mut dyn FnMut() -> f64) -> Self {
        Self {
            need: exp1(uniform),
            reference: None,
        }
    }

    /// Encodes sample `v`, held for `dt_s` simulated seconds: `emit(offset_s, mv)` for each input event, `offset_s`
    /// from the sample's time. `uniform` gives fresh uniforms in [0, 1) (rate encoding only).
    pub fn encode(
        &mut self,
        encoding: SampleEncoding,
        v: f32,
        dt_s: f64,
        uniform: &mut dyn FnMut() -> f64,
        emit: &mut dyn FnMut(f64, f32),
    ) {
        match encoding {
            SampleEncoding::Current { gain_mv } => {
                let mv = gain_mv * v;
                if mv != 0.0 {
                    emit(0.0, mv);
                }
            }
            SampleEncoding::Rate { max_hz, mv } => {
                let rate = f64::from(v.clamp(0.0, 1.0)) * max_hz;
                // Time rescaling: an event whenever the integrated rate reaches the channel's next Exp(1) draw,
                // so the process is exact across samples of different rates.
                let mut start = 0.0;
                if rate > 0.0 {
                    loop {
                        let mass = rate * (dt_s - start);
                        if self.need > mass {
                            self.need -= mass;
                            break;
                        }
                        start += self.need / rate;
                        emit(start, mv);
                        self.need = exp1(uniform);
                    }
                }
            }
            SampleEncoding::Threshold { level, mv } => {
                let reference = self.reference.get_or_insert(v);
                while v - *reference >= level {
                    *reference += level;
                    emit(0.0, mv);
                }
                while *reference - v >= level {
                    *reference -= level;
                    emit(0.0, -mv);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lcg() -> impl FnMut() -> f64 {
        let mut s = 1u64;
        move || {
            s = s.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            (s >> 11) as f64 / (1u64 << 53) as f64
        }
    }

    #[test]
    fn rate_matches_its_mean() {
        let mut u = lcg();
        let mut ch = Channel::new(&mut u);
        let mut events = 0;
        for _ in 0..10_000 {
            ch.encode(SampleEncoding::Rate { max_hz: 100.0, mv: 1.0 }, 0.5, 0.001, &mut u, &mut |_, _| events += 1);
        }
        // 50 Hz for 10 s: 500 ± a few √500.
        assert!((430..570).contains(&events), "{events}");
    }

    #[test]
    fn threshold_steps_up_and_down() {
        let mut u = lcg();
        let mut ch = Channel::new(&mut u);
        let mut out = Vec::new();
        for v in [0.0, 0.25, 0.5, 0.2] {
            ch.encode(SampleEncoding::Threshold { level: 0.2, mv: 3.0 }, v, 0.001, &mut u, &mut |_, mv| out.push(mv));
        }
        assert_eq!(out, vec![3.0, 3.0, -3.0]);
    }

    #[test]
    fn current_sends_nonzero_samples() {
        let mut u = lcg();
        let mut ch = Channel::new(&mut u);
        let mut out = Vec::new();
        for v in [0.0, 2.0] {
            ch.encode(SampleEncoding::Current { gain_mv: 1.5 }, v, 0.001, &mut u, &mut |dt, mv| out.push((dt, mv)));
        }
        assert_eq!(out, vec![(0.0, 3.0)]);
    }
}
