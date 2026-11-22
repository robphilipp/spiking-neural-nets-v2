//! Event subscriptions: streams of emitted events for a UI, a recorder or another live consumer.
//!
//! Each shard collects the events emitted during a window and, at the end of the window, hands them to every
//! subscriber as one batch. A bounded subscriber (a UI) loses a batch when its buffer is full, and the loss is
//! counted. A lossless subscriber (a recorder) has an unbounded buffer, so nothing is lost, but memory grows while
//! it falls behind. Either way the simulation never waits for a subscriber.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, SyncSender, TrySendError, channel, sync_channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::Emitted;

/// Decides which events a subscriber receives.
pub type EventFilter<E> = Arc<dyn Fn(&Emitted<E>) -> bool + Send + Sync>;

enum Tx<E> {
    Bounded(SyncSender<Vec<Emitted<E>>>),
    Unbounded(Sender<Vec<Emitted<E>>>),
}

struct Subscriber<E> {
    tx: Tx<E>,
    filter: Option<EventFilter<E>>,
    dropped: Arc<AtomicU64>,
}

impl<E> Subscriber<E> {
    /// Sends without blocking. Returns `false` if the subscriber has gone away.
    fn send(&self, events: Vec<Emitted<E>>) -> bool {
        if events.is_empty() {
            return true;
        }
        let len = events.len() as u64;
        match &self.tx {
            Tx::Unbounded(tx) => tx.send(events).is_ok(),
            Tx::Bounded(tx) => match tx.try_send(events) {
                Ok(()) => true,
                Err(TrySendError::Full(_)) => {
                    self.dropped.fetch_add(len, Ordering::Relaxed);
                    true
                }
                Err(TrySendError::Disconnected(_)) => false,
            },
        }
    }
}

/// The subscribers of one runtime, shared by all its shards.
pub(crate) struct Subscribers<E> {
    list: Mutex<Vec<Subscriber<E>>>,
    count: AtomicUsize,
}

impl<E> Default for Subscribers<E> {
    fn default() -> Self {
        Self {
            list: Mutex::new(Vec::new()),
            count: AtomicUsize::new(0),
        }
    }
}

impl<E: Clone> Subscribers<E> {
    /// `true` if anyone is subscribed (so shards only collect events when needed).
    pub(crate) fn active(&self) -> bool {
        self.count.load(Ordering::Relaxed) > 0
    }

    /// Subscribes with a buffer of `capacity` batches, or an unbounded (lossless) buffer for `None`.
    pub(crate) fn subscribe(&self, capacity: Option<usize>, filter: Option<EventFilter<E>>) -> EventStream<E> {
        let (tx, rx) = match capacity {
            Some(capacity) => {
                let (tx, rx) = sync_channel(capacity.max(1));
                (Tx::Bounded(tx), rx)
            }
            None => {
                let (tx, rx) = channel();
                (Tx::Unbounded(tx), rx)
            }
        };
        let dropped = Arc::new(AtomicU64::new(0));
        let mut list = self.list.lock().unwrap_or_else(|p| p.into_inner());
        list.push(Subscriber {
            tx,
            filter,
            dropped: Arc::clone(&dropped),
        });
        self.count.store(list.len(), Ordering::Relaxed);
        EventStream { rx, dropped }
    }

    /// Hands a batch to every subscriber without blocking.
    pub(crate) fn publish(&self, batch: Vec<Emitted<E>>) {
        if batch.is_empty() {
            return;
        }
        let mut list = self.list.lock().unwrap_or_else(|p| p.into_inner());
        if list.len() == 1 && list[0].filter.is_none() {
            // Common case: one unfiltered subscriber gets the batch itself, without copying.
            if !list[0].send(batch) {
                list.clear();
            }
        } else {
            list.retain(|sub| {
                let events = match &sub.filter {
                    Some(filter) => batch.iter().filter(|e| filter(e)).cloned().collect(),
                    None => batch.clone(),
                };
                sub.send(events)
            });
        }
        self.count.store(list.len(), Ordering::Relaxed);
    }
}

/// The runtime behind an [`EventStream`] has gone away.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Closed;

/// A live stream of emitted events, delivered in batches (one per shard per window).
///
/// Events within a batch are in time order. Batches from different shards interleave, so sort by time if you
/// need a global order. Dropping the stream unsubscribes.
pub struct EventStream<E> {
    rx: Receiver<Vec<Emitted<E>>>,
    dropped: Arc<AtomicU64>,
}

impl<E> EventStream<E> {
    /// Everything that has arrived so far, without waiting.
    pub fn drain(&self) -> Vec<Emitted<E>> {
        self.rx.try_iter().flatten().collect()
    }

    /// Waits for the next batch; `None` once the runtime is gone and everything has been received.
    pub fn next_batch(&self) -> Option<Vec<Emitted<E>>> {
        self.rx.recv().ok()
    }

    /// Waits up to `timeout` for the next batch.
    pub fn next_batch_timeout(&self, timeout: Duration) -> Option<Vec<Emitted<E>>> {
        self.rx.recv_timeout(timeout).ok()
    }

    /// Waits up to `timeout` for the next batch, telling a timeout (`Ok(None)`) apart from the runtime having
    /// gone away with everything received (`Err(Closed)`).
    pub fn recv_timeout(&self, timeout: Duration) -> Result<Option<Vec<Emitted<E>>>, Closed> {
        match self.rx.recv_timeout(timeout) {
            Ok(batch) => Ok(Some(batch)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(Closed),
        }
    }

    /// How many events were dropped for this subscriber because it fell behind.
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

impl<E> std::fmt::Debug for EventStream<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventStream")
            .field("dropped", &self.dropped())
            .finish_non_exhaustive()
    }
}
