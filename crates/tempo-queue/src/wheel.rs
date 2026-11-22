//! Hierarchical timing wheel.
//!
//! Time is divided into *ticks* of a configurable bucket width (1 µs by default). The wheel has [`LEVELS`] levels
//! of 64 slots; each level's slots are 64× wider than the level below. With 1 µs buckets:
//!
//! | level | slot width | level span |
//! |-------|-----------|------------|
//! | 0 | 1 µs | 64 µs |
//! | 1 | 64 µs | 4.1 ms |
//! | 2 | 4.1 ms | 262 ms |
//! | 3 | 262 ms | 16.8 s |
//! | 4 | 16.8 s | 17.9 min |
//! | 5 | 17.9 min | 19.1 h |
//!
//! Events further out go to an overflow heap. An event is placed at the level of the highest 6-bit digit in which
//! its tick differs from the wheel's current tick (`elapsed`), so its slot is always ahead of the current position
//! at that level. When nothing earlier remains, the wheel advances `elapsed` to the start of the earliest occupied
//! slot and re-inserts ("cascades") that slot's events, which then land in lower levels. Events whose tick is at
//! or before `elapsed` go to a small `ready` heap that orders them by full key, so ties within one bucket keep the
//! exact `(time, sender, sequence, sub)` order.
//!
//! Invariant: every event in `ready` has tick ≤ `elapsed`, and every event in the levels or the overflow has
//! tick > `elapsed`. So the smallest key is always in `ready` once `ready` is non-empty.

use std::collections::BinaryHeap;
use std::mem;

use tempo_core::{OrderKey, SimDuration};

use crate::{Entry, TimeQueue};

const SLOT_BITS: u32 = 6;
const SLOTS: usize = 1 << SLOT_BITS;
/// Number of wheel levels. Ticks that differ from `elapsed` beyond `SLOT_BITS * LEVELS` bits go to the overflow heap.
const LEVELS: usize = 6;

/// A [`TimeQueue`] backed by a hierarchical timing wheel in simulation time.
#[derive(Debug)]
pub struct WheelQueue<T> {
    bucket_ns: u64,
    /// The wheel's current tick.
    elapsed: u64,
    ready: BinaryHeap<Entry<T>>,
    levels: Vec<Level<T>>,
    overflow: BinaryHeap<Entry<T>>,
    len: usize,
}

#[derive(Debug)]
struct Level<T> {
    /// Bit `i` is set when slot `i` holds events.
    occupied: u64,
    slots: Vec<Vec<Entry<T>>>,
}

impl<T> Level<T> {
    fn new() -> Self {
        Self {
            occupied: 0,
            slots: (0..SLOTS).map(|_| Vec::new()).collect(),
        }
    }
}

impl<T> WheelQueue<T> {
    /// The default bucket width: 1 µs.
    pub const DEFAULT_BUCKET: SimDuration = SimDuration::MICROSECOND;

    /// An empty wheel with 1 µs buckets.
    pub fn new() -> Self {
        Self::with_bucket(Self::DEFAULT_BUCKET)
    }

    /// An empty wheel with the given bucket width. Ordering is exact for any width; the width only affects speed.
    ///
    /// # Panics
    /// If `bucket` is zero.
    pub fn with_bucket(bucket: SimDuration) -> Self {
        assert!(!bucket.is_zero(), "bucket width must be non-zero");
        Self {
            bucket_ns: bucket.as_nanos(),
            elapsed: 0,
            ready: BinaryHeap::new(),
            levels: (0..LEVELS).map(|_| Level::new()).collect(),
            overflow: BinaryHeap::new(),
            len: 0,
        }
    }

    fn tick(&self, key: &OrderKey) -> u64 {
        key.at.as_nanos() / self.bucket_ns
    }

    /// The level for a tick strictly after `elapsed`, or `LEVELS` if it belongs in the overflow heap.
    fn level_for(&self, tick: u64) -> usize {
        let masked = (self.elapsed ^ tick) | (SLOTS as u64 - 1);
        let significant = 63 - masked.leading_zeros();
        (significant / SLOT_BITS) as usize
    }

    /// Puts an entry where it belongs relative to the current `elapsed` tick.
    fn place(&mut self, entry: Entry<T>) {
        let tick = self.tick(&entry.key);
        if tick <= self.elapsed {
            self.ready.push(entry);
            return;
        }
        let level = self.level_for(tick);
        if level >= LEVELS {
            self.overflow.push(entry);
        } else {
            let slot = ((tick >> (level as u32 * SLOT_BITS)) as usize) & (SLOTS - 1);
            let lvl = &mut self.levels[level];
            lvl.slots[slot].push(entry);
            lvl.occupied |= 1 << slot;
        }
    }

    /// Moves overflow entries that now fit in the wheel (or are due) into it.
    fn pull_overflow(&mut self) {
        while let Some(min) = self.overflow.peek() {
            let tick = self.tick(&min.key);
            if tick > self.elapsed && self.level_for(tick) >= LEVELS {
                // The overflow heap is ordered by time, and later ticks need levels at least as high,
                // so nothing further back fits either.
                break;
            }
            let entry = self.overflow.pop().expect("peeked entry exists");
            self.place(entry);
        }
    }

    /// Advances the wheel until `ready` holds the smallest entry. Returns `false` if the queue is empty.
    fn refill(&mut self) -> bool {
        while self.ready.is_empty() {
            if let Some(level) = self.levels.iter().position(|l| l.occupied != 0) {
                // The lowest occupied level holds the earliest events: everything in it lies inside the current
                // slot of every higher level.
                let lvl = &mut self.levels[level];
                let slot = lvl.occupied.trailing_zeros() as usize;
                lvl.occupied &= !(1 << slot);
                let mut entries = mem::take(&mut lvl.slots[slot]);

                let shift = level as u32 * SLOT_BITS;
                let keep_above = !((1u64 << (shift + SLOT_BITS)) - 1);
                let slot_start = (self.elapsed & keep_above) | ((slot as u64) << shift);
                debug_assert!(slot_start > self.elapsed, "wheel slots must lie ahead of elapsed");
                self.elapsed = slot_start;

                for entry in entries.drain(..) {
                    self.place(entry);
                }
                // Cascaded entries land in `ready` or lower levels, never back in this slot; keep the allocation.
                self.levels[level].slots[slot] = entries;
                self.pull_overflow();
            } else if let Some(min) = self.overflow.peek() {
                self.elapsed = self.tick(&min.key);
                self.pull_overflow();
            } else {
                return false;
            }
        }
        true
    }
}

impl<T> Default for WheelQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> TimeQueue<T> for WheelQueue<T> {
    fn push(&mut self, key: OrderKey, item: T) {
        self.place(Entry { key, item });
        self.len += 1;
    }

    fn peek_key(&mut self) -> Option<OrderKey> {
        if self.refill() {
            self.ready.peek().map(|e| e.key)
        } else {
            None
        }
    }

    fn pop(&mut self) -> Option<(OrderKey, T)> {
        if !self.refill() {
            return None;
        }
        let entry = self.ready.pop()?;
        self.len -= 1;
        Some((entry.key, entry.item))
    }

    fn len(&self) -> usize {
        self.len
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempo_core::{ActorId, SimTime};

    fn key(at_ns: u64, seq: u64) -> OrderKey {
        OrderKey {
            at: SimTime::from_nanos(at_ns),
            src: ActorId::from_raw(0),
            seq,
            sub: 0,
        }
    }

    fn drain(q: &mut WheelQueue<u64>) -> Vec<u64> {
        std::iter::from_fn(|| q.pop().map(|(k, _)| k.at.as_nanos())).collect()
    }

    #[test]
    fn orders_within_one_bucket_by_nanosecond() {
        let mut q = WheelQueue::new();
        for (i, ns) in [900, 100, 500].into_iter().enumerate() {
            q.push(key(ns, i as u64), 0);
        }
        assert_eq!(drain(&mut q), vec![100, 500, 900]);
    }

    #[test]
    fn spans_every_level_and_the_overflow() {
        let mut q = WheelQueue::new();
        // 1 µs .. ~3 days, crossing every level boundary and into the overflow heap.
        let times: Vec<u64> = (0..40).map(|p| 1_000u64 << p).collect();
        for (i, &t) in times.iter().rev().enumerate() {
            q.push(key(t, i as u64), 0);
        }
        assert_eq!(q.len(), times.len());
        assert_eq!(drain(&mut q), times);
        assert!(q.is_empty());
    }

    #[test]
    fn accepts_pushes_earlier_than_the_wheel_position() {
        let mut q = WheelQueue::new();
        q.push(key(5_000_000, 0), 0);
        // Peeking advances the wheel to the 5 ms event's slot...
        assert_eq!(q.peek_key().unwrap().at.as_nanos(), 5_000_000);
        // ...but an earlier event pushed afterwards still comes out first.
        q.push(key(2_000, 1), 0);
        assert_eq!(drain(&mut q), vec![2_000, 5_000_000]);
    }

    #[test]
    fn pop_before_respects_the_window_end() {
        let mut q = WheelQueue::new();
        q.push(key(10_000, 0), 0);
        q.push(key(30_000, 1), 0);
        let end = SimTime::from_nanos(20_000);
        assert_eq!(q.pop_before(end).unwrap().0.at.as_nanos(), 10_000);
        assert!(q.pop_before(end).is_none());
        assert_eq!(q.len(), 1);
    }
}
