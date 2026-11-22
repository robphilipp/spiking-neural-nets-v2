//! Time-ordered event queues for the `tempo` actor framework.
//!
//! Each shard keeps its pending events in a [`TimeQueue`], which always hands back the event with the smallest
//! [`OrderKey`] `(time, sender, sequence, sub)`. Two implementations share the trait so they can be benchmarked
//! against each other:
//!
//! - [`HeapQueue`]: a binary heap. Simple, O(log n) per operation.
//! - [`WheelQueue`]: a hierarchical timing wheel keyed by simulation time. Average O(1) per operation, and
//!   well suited to the mix of microsecond synaptic delays, millisecond timers and rare long delays.
//!
//! Both are complete priority queues: any key can be pushed at any time, including keys earlier than ones already
//! popped, and the minimum is always returned first. The wheel runs in *simulation* time, so unlike v1's
//! hashed-wheel scheduler it has no ticking thread and no dependence on sleep precision.

mod heap;
mod wheel;

pub use heap::HeapQueue;
pub use wheel::WheelQueue;

use core::cmp::Ordering;
use tempo_core::{OrderKey, SimTime};

/// A priority queue of items ordered by [`OrderKey`], smallest first.
pub trait TimeQueue<T> {
    /// Adds an item.
    fn push(&mut self, key: OrderKey, item: T);

    /// The smallest key, without removing it. Takes `&mut self` because a wheel may need to advance to find it.
    fn peek_key(&mut self) -> Option<OrderKey>;

    /// Removes and returns the item with the smallest key.
    fn pop(&mut self) -> Option<(OrderKey, T)>;

    /// Removes and returns the smallest item if its time is before `end`. Used to process one time window.
    fn pop_before(&mut self, end: SimTime) -> Option<(OrderKey, T)> {
        match self.peek_key() {
            Some(key) if key.at < end => self.pop(),
            _ => None,
        }
    }

    /// Number of queued items.
    fn len(&self) -> usize;

    /// `true` when nothing is queued.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A queued item. Ordered by key in *reverse*, so `std`'s max-heap `BinaryHeap` pops the smallest key first.
#[derive(Debug)]
struct Entry<T> {
    key: OrderKey,
    item: T,
}

impl<T> PartialEq for Entry<T> {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl<T> Eq for Entry<T> {}

impl<T> PartialOrd for Entry<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for Entry<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        other.key.cmp(&self.key)
    }
}
