//! The window barrier: all shards meet here at the end of each window. Two values are min-reduced across shards
//! (the earliest pending event time and the proposed lookahead), and the last shard to arrive turns them into a
//! [`Decision`] about the next window. Every shard receives that same decision, so they all agree on window
//! boundaries even when the decision depends on the wall clock.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

/// What all shards do next.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct Decision {
    /// Start of the next window (ns).
    pub(crate) start: u64,
    /// End of the next window (ns, exclusive).
    pub(crate) end: u64,
    /// The agreed lookahead (ns).
    pub(crate) lookahead: u64,
    /// Stop the run.
    pub(crate) stop: bool,
}

impl Decision {
    const STOP: Self = Self {
        start: u64::MAX,
        end: u64::MAX,
        lookahead: u64::MAX,
        stop: true,
    };
}

/// One crossing's reduced inputs and published decision.
#[derive(Debug)]
struct Slot {
    next: AtomicU64,
    lookahead: AtomicU64,
    start: AtomicU64,
    end: AtomicU64,
    decided_lookahead: AtomicU64,
    stop: AtomicBool,
}

impl Slot {
    fn new() -> Self {
        Self {
            next: AtomicU64::new(u64::MAX),
            lookahead: AtomicU64::new(u64::MAX),
            start: AtomicU64::new(0),
            end: AtomicU64::new(0),
            decided_lookahead: AtomicU64::new(0),
            stop: AtomicBool::new(false),
        }
    }
}

/// A reusable barrier with min-reductions. Each crossing ("generation") uses one of two slots, so a slot is never
/// reset while a slow shard may still be reading it.
#[derive(Debug)]
pub(crate) struct WindowBarrier {
    n: usize,
    arrived: AtomicUsize,
    generation: AtomicU64,
    slots: [Slot; 2],
    poisoned: AtomicBool,
}

impl WindowBarrier {
    pub(crate) fn new(n: usize) -> Self {
        Self {
            n,
            arrived: AtomicUsize::new(0),
            generation: AtomicU64::new(0),
            slots: [Slot::new(), Slot::new()],
            poisoned: AtomicBool::new(false),
        }
    }

    /// Waits for every shard. The last to arrive calls `decide(min next, min lookahead, idle)`; everyone gets its
    /// result. Shards that wait call `idle` now and then (and `decide` may too, if it waits).
    pub(crate) fn arrive(
        &self,
        next: u64,
        lookahead: u64,
        idle: &mut dyn FnMut(),
        decide: impl FnOnce(u64, u64, &mut dyn FnMut()) -> Decision,
    ) -> Decision {
        if self.n == 1 {
            return self.finish(decide(next, lookahead, idle));
        }
        let generation = self.generation.load(Ordering::Acquire);
        let slot = &self.slots[(generation & 1) as usize];
        slot.next.fetch_min(next, Ordering::AcqRel);
        slot.lookahead.fetch_min(lookahead, Ordering::AcqRel);
        if self.arrived.fetch_add(1, Ordering::AcqRel) + 1 == self.n {
            let decision = self.finish(decide(
                slot.next.load(Ordering::Acquire),
                slot.lookahead.load(Ordering::Acquire),
                idle,
            ));
            slot.start.store(decision.start, Ordering::Relaxed);
            slot.end.store(decision.end, Ordering::Relaxed);
            slot.decided_lookahead.store(decision.lookahead, Ordering::Relaxed);
            slot.stop.store(decision.stop, Ordering::Relaxed);
            // Prepare the other slot for the next crossing. Every shard has finished reading it: they all had to
            // arrive here, after reading their previous decision.
            let other = &self.slots[1 - (generation & 1) as usize];
            other.next.store(u64::MAX, Ordering::Relaxed);
            other.lookahead.store(u64::MAX, Ordering::Relaxed);
            self.arrived.store(0, Ordering::Relaxed);
            self.generation.store(generation + 1, Ordering::Release);
            decision
        } else {
            let mut spins = 0u32;
            while self.generation.load(Ordering::Acquire) == generation {
                if self.is_poisoned() {
                    return Decision::STOP;
                }
                if backoff(&mut spins) {
                    idle();
                }
            }
            Decision {
                start: slot.start.load(Ordering::Relaxed),
                end: slot.end.load(Ordering::Relaxed),
                lookahead: slot.decided_lookahead.load(Ordering::Relaxed),
                stop: slot.stop.load(Ordering::Relaxed),
            }
        }
    }

    fn finish(&self, mut decision: Decision) -> Decision {
        decision.stop |= self.is_poisoned();
        decision
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
/// Returns `true` once spinning has given way to yielding.
fn backoff(spins: &mut u32) -> bool {
    if *spins < 128 {
        std::hint::spin_loop();
        *spins += 1;
        false
    } else {
        std::thread::yield_now();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decide(next: u64, lookahead: u64, _: &mut dyn FnMut()) -> Decision {
        Decision {
            start: next,
            end: next + lookahead,
            lookahead,
            stop: false,
        }
    }

    #[test]
    fn every_shard_gets_the_same_decision_every_round() {
        let n = 6;
        let rounds = 2_000u64;
        let barrier = WindowBarrier::new(n);
        std::thread::scope(|s| {
            for shard in 0..n as u64 {
                let barrier = &barrier;
                s.spawn(move || {
                    for round in 0..rounds {
                        // The minima move between shards from round to round.
                        let next = 1_000 * round + (shard + round) % n as u64;
                        let lookahead = 50 + (shard + 2 * round) % n as u64;
                        let d = barrier.arrive(next, lookahead, &mut || {}, decide);
                        assert_eq!(d, decide(1_000 * round, 50, &mut || {}));
                    }
                });
            }
        });
    }

    #[test]
    fn poisoning_releases_waiters() {
        let barrier = WindowBarrier::new(2);
        std::thread::scope(|s| {
            let waiter = s.spawn(|| barrier.arrive(5, 5, &mut || {}, decide));
            std::thread::sleep(std::time::Duration::from_millis(10));
            barrier.poison();
            assert_eq!(waiter.join().unwrap(), Decision::STOP);
        });
    }
}
