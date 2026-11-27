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

/// Runs a test body on its own thread and fails the test if it is still running after `limit`.
///
/// Tests that run several nodes in one process wait on each other (a lockstep barrier, a socket). If a message is
/// lost or one side panics, the other waits forever, and `cargo test` has no per-test timeout, so the whole run
/// hangs silently. With this, the test panics with its name instead and the rest of the suite runs.
///
/// If the body panics, that panic is re-raised unchanged. After a timeout the body's thread is left running
/// (a blocked thread cannot be stopped), but the test harness exits the process when the run ends without waiting
/// for it.
pub fn with_timeout<T: Send + 'static>(
    limit: std::time::Duration,
    name: &str,
    body: impl FnOnce() -> T + Send + 'static,
) -> T {
    let (done, result) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name(format!("{name} (watched)"))
        .spawn(move || {
            // A closed channel means the watchdog already gave up, so a late result has nowhere to go.
            let _ = done.send(std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)));
        })
        .expect("spawn the watched test thread");
    match result.recv_timeout(limit) {
        Ok(Ok(value)) => value,
        Ok(Err(payload)) => std::panic::resume_unwind(payload),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => panic!(
            "test {name} still running after {} s: it probably hung (deadlock or lost message)",
            limit.as_secs()
        ),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            unreachable!("the watched thread always sends a result")
        }
    }
}
