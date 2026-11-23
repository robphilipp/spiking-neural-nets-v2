//! Live frames.

use std::path::Path;
use std::time::Duration;

use spikes_api::frame::{Frame, Lod, decode};
use spikes_session::{BuildSettings, Engine, RunSettings, Selection, Session, Speed, ViewSettings, Viewer};
use tempo_runtime::SimDuration;

fn v1(name: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../test-data/v1")
            .join(name),
    )
    .unwrap()
}

fn ring(engine: &Engine) -> Session {
    let session = engine.create("ring", v1("ring.boo")).unwrap();
    session.build(BuildSettings::default()).unwrap();
    session
}

/// Starts the ring (group 1 stimulated) and runs 200 ms as fast as possible.
fn run_ring(session: &Session) {
    let network = session.network();
    let group1: Vec<u32> = network
        .neurons
        .iter()
        .filter(|n| n.name.starts_with("hidden-10"))
        .map(|n| n.index)
        .collect();
    session.stimulate(&group1, 10.0).unwrap();
    session
        .start(RunSettings {
            speed: Speed::AsFastAsPossible,
            duration: Some(SimDuration::from_millis(200)),
            record: None,
        })
        .unwrap();
    assert!(session.wait_for_run(Duration::from_secs(10)));
}

/// Frames until none arrive for 150 ms.
fn collect(viewer: &Viewer) -> Vec<Frame> {
    let mut frames = Vec::new();
    while let Some(bytes) = viewer.recv_timeout(Duration::from_millis(150)) {
        frames.push(decode(&bytes).unwrap());
    }
    frames
}

#[test]
fn frames_carry_every_spike_with_its_time() {
    let engine = Engine::new();
    let session = ring(&engine);
    let viewer = session.attach(ViewSettings::default());
    run_ring(&session);
    let frames = collect(&viewer);
    let mut spikes: Vec<(u32, u64)> = frames
        .iter()
        .flat_map(|f| f.spikes.iter().map(move |&(n, o)| (n, f.time_ns(o))))
        .collect();
    spikes.sort_by_key(|s| s.1);
    assert_eq!(spikes.len(), 25, "the ring's 25 spikes");
    assert_eq!(spikes[0].1, 2_000_000, "first spike at 2 ms");
    assert!(frames.iter().all(|f| f.header.lod == Lod::Exact));
    let seqs: Vec<u32> = frames.iter().map(|f| f.header.seq).collect();
    assert!(seqs.windows(2).all(|w| w[1] == w[0] + 1), "{seqs:?}");
    assert_eq!(frames.last().unwrap().header.t1_ns, 200_000_000);
}

#[test]
fn a_tight_budget_switches_to_counts_then_cells() {
    let engine = Engine::new();
    let session = ring(&engine);
    // The ring's 25 spikes (25 neurons) arrive in one frame: exactly they take 2 × 104 = 208 bytes, as counts
    // 104 + 56 = 160. 12 kB/s at 16 ms frames is 192 bytes: counts fit, exact spikes don't.
    let tight = session.attach(ViewSettings {
        budget_bytes_per_s: 12_000,
        weights: Selection::Nothing, // spikes only, so the whole budget is theirs
        ..ViewSettings::default()
    });
    let tiny = session.attach(ViewSettings {
        budget_bytes_per_s: 10,
        weights: Selection::Nothing,
        ..ViewSettings::default()
    });
    run_ring(&session);
    for (viewer, lod) in [(&tight, Lod::Counts), (&tiny, Lod::Binned)] {
        let frames = collect(viewer);
        let total: u32 = frames
            .iter()
            .map(|f| {
                f.spikes.len() as u32
                    + f.spike_counts.iter().map(|c| u32::from(c.1)).sum::<u32>()
                    + f.spike_bins.iter().map(|c| c.1).sum::<u32>()
            })
            .sum();
        assert_eq!(total, 25, "{lod:?}: every spike is counted");
        let summary: Vec<_> = frames
            .iter()
            .map(|f| (f.header.lod, f.spikes.len(), f.spike_counts.len(), f.spike_bins.len()))
            .collect();
        assert!(frames.iter().any(|f| f.header.lod == lod), "{lod:?} used: {summary:?}");
    }
}

#[test]
fn membrane_and_weight_selections() {
    let engine = Engine::new();
    let session = engine.create("stdp", v1("stdp-test.boo")).unwrap();
    // Attached before the build: the membrane neuron is observed when the network is built.
    let viewer = session.attach(ViewSettings {
        spikes: Selection::Nothing,
        membrane: vec![2],
        weights: Selection::Neurons(vec![2]),
        ..ViewSettings::default()
    });
    session.build(BuildSettings::default()).unwrap();
    for t in 0..20 {
        session.stimulate(&[0, 1], 10.0).unwrap();
        session
            .step(SimDuration::from_millis(if t % 2 == 0 { 30 } else { 70 }))
            .unwrap();
    }
    let frames = collect(&viewer);
    assert!(frames.iter().all(|f| f.spikes.is_empty()));
    let membrane: Vec<_> = frames.iter().flat_map(|f| f.membrane.clone()).collect();
    assert!(!membrane.is_empty());
    assert!(membrane.iter().all(|m| m.0 == 2), "only the selected neuron");
    let weights: Vec<_> = frames.iter().flat_map(|f| f.weights.clone()).collect();
    assert!(!weights.is_empty() && weights.iter().all(|w| w.0 == 2));
    // The latest weights in the frames match the neuron's synapses.
    let snapshot = session.query_neuron(2).unwrap();
    for (synapse, &(_, w, _)) in snapshot.synapses.iter().enumerate() {
        let last = weights.iter().rev().find(|x| x.1 == synapse as u32).map(|x| x.2);
        assert_eq!(last, Some(w), "synapse {synapse}");
    }
}

#[test]
fn a_slow_viewer_loses_frames_not_time() {
    let engine = Engine::new();
    let session = ring(&engine);
    let viewer = session.attach(ViewSettings {
        frame: Duration::from_millis(5),
        ..ViewSettings::default()
    });
    session
        .start(RunSettings {
            speed: Speed::Factor(1.0),
            duration: None,
            record: None,
        })
        .unwrap();
    std::thread::sleep(Duration::from_millis(300)); // not reading
    // Now read promptly for 150 ms (the run is still going, so frames keep coming).
    let mut frames = Vec::new();
    let until = std::time::Instant::now() + Duration::from_millis(150);
    while std::time::Instant::now() < until {
        if let Some(bytes) = viewer.recv_timeout(Duration::from_millis(20)) {
            frames.push(decode(&bytes).unwrap());
        }
    }
    session.stop().unwrap();
    let last = &frames.last().unwrap().header;
    assert!(last.dropped_frames > 10, "{last:?}");
    assert!(
        last.t1_ns >= 400_000_000,
        "the run kept to the clock: {} ms",
        last.t1_ns / 1_000_000
    );
    assert!((0.9..1.1).contains(&last.speed));
}

#[test]
fn viewers_survive_a_rebuild() {
    let engine = Engine::new();
    let session = ring(&engine);
    let viewer = session.attach(ViewSettings::default());
    run_ring(&session);
    let first = collect(&viewer).iter().map(|f| f.spikes.len()).sum::<usize>();
    session.build(BuildSettings::default()).unwrap();
    run_ring(&session);
    let second = collect(&viewer).iter().map(|f| f.spikes.len()).sum::<usize>();
    assert_eq!((first, second), (25, 25));
}

#[test]
fn frames_keep_coming_after_stop_and_rebuild() {
    // Rebuilding resets simulation time to zero. A frame made from the old run's clock just after the reset
    // used to make a frame end before it started: an overflow that killed the session's frames for good.
    let engine = Engine::new();
    let session = ring(&engine);
    let viewer = session.attach(ViewSettings {
        frame: Duration::from_millis(2),
        ..ViewSettings::default()
    });
    for round in 0..15 {
        session
            .build(BuildSettings {
                shards: 2,
                ..BuildSettings::default()
            })
            .unwrap();
        session
            .start(RunSettings {
                speed: Speed::Factor(1.0),
                duration: None,
                record: None,
            })
            .unwrap();
        std::thread::sleep(Duration::from_millis(40 + round * 7));
        session.stop().unwrap();
        // Frames arrive for this run: at least one ends past 20 ms of this run's time.
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let mut latest = 0;
        while std::time::Instant::now() < deadline && latest < 20_000_000 {
            if let Some(bytes) = viewer.recv_timeout(Duration::from_millis(20)) {
                latest = latest.max(decode(&bytes).unwrap().header.t1_ns);
            }
        }
        assert!(
            latest >= 20_000_000,
            "round {round}: frames stopped (latest t1 {latest} ns)"
        );
        while viewer.try_recv().is_some() {}
    }
}
