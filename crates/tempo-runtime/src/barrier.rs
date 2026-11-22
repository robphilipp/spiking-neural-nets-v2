//! The window barrier: all shards meet here at the end of each window, agree on the earliest pending event time
//! across all of them (a min-reduction), and agree on whether to stop.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

/// A reusable barrier with a min-reduction. Each crossing ("generation") uses one of two result slots, so a slot
/// is never reset while a slow shard may still be reading it.
#[derive(Debug)]
pub(crate) struct WindowBarrier {
    n: usize,
    arrived: AtomicUsize,
    generation: AtomicU64,
    mins: [AtomicU64; 2],
    stops: [AtomicBool; 2],
    poisoned: AtomicBool,
}

impl WindowBarrier {
    pub(crate) fn new(n: usize) -> Self {
        Self {
            n,
            arrived: AtomicUsize::new(0),
            generation: AtomicU64::new(0),
            mins: [AtomicU64::new(u64::MAX), AtomicU64::new(u64::MAX)],
            stops: [AtomicBool::new(false), AtomicBool::new(false)],
            poisoned: AtomicBool::new(false),
        }
    }

    /// Waits for every shard. Returns the minimum of all shards' `local_min` values, and whether to stop. The
    /// stop decision is made once, by the last shard to arrive, so every shard gets the same answer.
    pub(crate) fn arrive(&self, local_min: u64, stop_requested: impl Fn() -> bool) -> (u64, bool) {
        if self.n == 1 {
            return (local_min, stop_requested() || self.is_poisoned());
        }
        let generation = self.generation.load(Ordering::Acquire);
        let slot = (generation & 1) as usize;
        self.mins[slot].fetch_min(local_min, Ordering::AcqRel);
        if self.arrived.fetch_add(1, Ordering::AcqRel) + 1 == self.n {
            let stop = stop_requested() || self.is_poisoned();
            self.stops[slot].store(stop, Ordering::Relaxed);
            // Prepare the other slot for the next crossing. Every shard has finished reading it: they all had to
            // arrive here, after reading their previous result.
            self.mins[1 - slot].store(u64::MAX, Ordering::Relaxed);
            self.arrived.store(0, Ordering::Relaxed);
            self.generation.store(generation + 1, Ordering::Release);
        } else {
            let mut spins = 0u32;
            while self.generation.load(Ordering::Acquire) == generation {
                if self.is_poisoned() {
                    return (u64::MAX, true);
                }
                backoff(&mut spins);
            }
        }
        (
            self.mins[slot].load(Ordering::Acquire),
            self.stops[slot].load(Ordering::Acquire),
        )
    }

    /// Releases every waiting shard with "stop" (used when a shard panics, so the others don't wait forever).
    pub(crate) fn poison(&self) {
        self.poisoned.store(true, Ordering::Release);
    }

    pub(crate) fn is_poisoned(&self) -> bool {
        self.poisoned.load(Ordering::Acquire)
    }
}

/// Spin briefly, then yield the CPU (so oversubscribed runs, with more shards than cores, still make progress).
fn backoff(spins: &mut u32) {
    if *spins < 128 {
        std::hint::spin_loop();
        *spins += 1;
    } else {
        std::thread::yield_now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shard_gets_the_global_minimum_every_round() {
        let n = 6;
        let rounds = 2_000u64;
        let barrier = WindowBarrier::new(n);
        std::thread::scope(|s| {
            for shard in 0..n as u64 {
                let barrier = &barrier;
                s.spawn(move || {
                    for round in 0..rounds {
                        // The minimum moves between shards from round to round.
                        let local = 1_000 * round + (shard + round) % n as u64;
                        let (min, stop) = barrier.arrive(local, || false);
                        assert_eq!(min, 1_000 * round);
                        assert!(!stop);
                    }
                });
            }
        });
    }

    #[test]
    fn poisoning_releases_waiters() {
        let barrier = WindowBarrier::new(2);
        std::thread::scope(|s| {
            let waiter = s.spawn(|| barrier.arrive(5, || false));
            std::thread::sleep(std::time::Duration::from_millis(10));
            barrier.poison();
            assert_eq!(waiter.join().unwrap(), (u64::MAX, true));
        });
    }
}
