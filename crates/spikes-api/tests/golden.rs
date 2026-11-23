//! Writes reference encodings for the TypeScript client's tests (`packages/spikes-client/test/golden/`), so the
//! Rust and TypeScript sides are checked against the same bytes. `cargo test -p spikes-api` refreshes them; a
//! change shows up in `git diff`.

use std::path::Path;

use spikes_api::frame::{FrameBuilder, FrameHeader, Lod};
use spikes_api::input::InputBatch;

fn golden(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/spikes-client/test/golden")
        .join(name)
}

#[test]
fn write_golden_files() {
    let header = FrameHeader {
        session: 7,
        seq: 42,
        viewer: 3,
        t0_ns: 1_500_000_000,
        t1_ns: 1_516_000_000,
        wall_unix_ms: 1_795_000_000_000,
        speed: 0.25,
        lag_ms: 1.5,
        lod: Lod::Exact,
        dropped_events: 2,
        dropped_frames: 1,
        time_unit_ns: 1,
        ..FrameHeader::default()
    };
    let mut b = FrameBuilder::new(&header);
    b.spikes(&[1, 2, 3], &[10, 20, 30])
        .spike_counts(&[4, 5, 6], &[1, 2, 3])
        .spike_bins(&[7], &[70])
        .membrane(&[1, 1], &[5, 6], &[-1.5, 2.25])
        .weights(&[1, 1], &[0, 1], &[0.5, 2.0])
        .transitions(&[9, 8, 7], &[1, 2, 3], &[true, false, true]);
    std::fs::write(golden("frame.bin"), b.finish()).unwrap();

    let samples = InputBatch::Samples {
        sensor: 3,
        seq: 7,
        t0_ns: 1 << 40,
        dt_ns: 1_000_000,
        channels: 2,
        values: vec![0.5, -1.0, 2.0, 3.5],
    };
    std::fs::write(golden("samples.bin"), samples.encode()).unwrap();
    let events = InputBatch::Events {
        sensor: 1,
        seq: 0,
        t0_ns: 5,
        events: vec![(0, 10), (3, 20), (1, 30)],
    };
    std::fs::write(golden("events.bin"), events.encode()).unwrap();
}
