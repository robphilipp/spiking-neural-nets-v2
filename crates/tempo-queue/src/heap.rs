use std::collections::BinaryHeap;

use tempo_core::OrderKey;

use crate::{Entry, TimeQueue};

/// A [`TimeQueue`] backed by a binary heap: O(log n) push and pop. The baseline for benchmarks.
#[derive(Debug)]
pub struct HeapQueue<T> {
    heap: BinaryHeap<Entry<T>>,
}

impl<T> HeapQueue<T> {
    /// An empty queue.
    pub fn new() -> Self {
        Self {
            heap: BinaryHeap::new(),
        }
    }

    /// An empty queue with room for `capacity` items.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            heap: BinaryHeap::with_capacity(capacity),
        }
    }
}

impl<T> Default for HeapQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> TimeQueue<T> for HeapQueue<T> {
    fn push(&mut self, key: OrderKey, item: T) {
        self.heap.push(Entry { key, item });
    }

    fn peek_key(&mut self) -> Option<OrderKey> {
        self.heap.peek().map(|e| e.key)
    }

    fn pop(&mut self) -> Option<(OrderKey, T)> {
        self.heap.pop().map(|e| (e.key, e.item))
    }

    fn len(&self) -> usize {
        self.heap.len()
    }
}
