//! Test support for the `tempo` actor framework.
//!
//! - [`trace_hash`]: a fingerprint of a run, for determinism tests.
//! - [`models`]: small actor models (ping-pong, a ring, a random LIF network) used by tests and benchmarks here
//!   and in later phases.

pub mod models;

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use tempo_runtime::TraceEntry;

/// A fingerprint of a run's deliveries.
///
/// It hashes each actor's own sequence of deliveries (in the order that actor received them) and combines the
/// actors in ID order. So it depends on what every actor saw, but not on how deliveries to *different* actors were
/// interleaved, which will differ between thread counts in phase 2 even when every actor's view is identical.
pub fn trace_hash(trace: &[TraceEntry]) -> u64 {
    let mut entries: Vec<&TraceEntry> = trace.iter().collect();
    // Stable sort: each actor's entries keep their delivery order.
    entries.sort_by_key(|e| e.dst);
    let mut hasher = DefaultHasher::new();
    for entry in entries {
        entry.hash(&mut hasher);
    }
    hasher.finish()
}
