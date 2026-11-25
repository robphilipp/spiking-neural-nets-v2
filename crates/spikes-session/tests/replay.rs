//! Replaying recorded runs in the live views.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use spikes_api::frame::decode;
use spikes_session::{
    BuildSettings, Engine, InputSpec, RunSettings, Selection, Session, SessionError, Speed, ViewSettings, Viewer,
};
use tempo_runtime::{SimDuration, SimTime};

fn v1(name: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../test-data/v1")
            .join(name),
    )
    .unwrap()
}

fn view() -> ViewSettings {
    ViewSettings {
        membrane: vec![2],
        weights: Selection::All,
        ..ViewSettings::default()
    }
}

/// What a viewer saw: spikes (time ns, neuron), membrane updates (time ns, mV), latest weight per synapse.
#[derive(Debug, Default, PartialEq)]
struct Seen {
    spikes: Vec<(u64, u32)>,
    membrane: Vec<(u64, u32)>, // mV as bits, to compare exactly
    weights: BTreeMap<(u32, u32), u32>,
}

fn collect(viewer: &Viewer) -> Seen {
    let mut seen = Seen::default();
    while let Some(bytes) = viewer.recv_timeout(Duration::from_millis(200)) {
        let f = decode(&bytes).unwrap();
        seen.spikes.extend(f.spikes.iter().map(|&(n, o)| (f.time_ns(o), n)));
        seen.membrane
            .extend(f.membrane.iter().map(|&(_, o, mv)| (f.time_ns(o), mv.to_bits())));
        for &(post, synapse, w) in &f.weights {
            seen.weights.insert((post, synapse), w.to_bits());
        }
    }
    seen.spikes.sort_unstable();
    seen.membrane.sort_unstable();
    seen
}

fn run(session: &Session, record: Option<PathBuf>) {
    session
        .start(RunSettings {
            speed: Speed::AsFastAsPossible,
            duration: Some(SimDuration::from_secs(2)),
            record,
        })
        .unwrap();
    assert!(session.wait_for_run(Duration::from_secs(10)));
}

/// Records 2 s of the STDP test network (Poisson input, the output neuron observed) and returns the directory
/// and what a viewer saw live.
fn record(engine: &Engine, name: &str) -> (PathBuf, Seen) {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    let live = engine.create("stdp", v1("stdp-test.boo")).unwrap();
    live.build(BuildSettings::default()).unwrap();
    live.add_input(InputSpec::Poisson {
        targets: vec![0, 1],
        rate_hz: 80.0,
        mv: 3.0,
        duration: None,
    })
    .unwrap();
    let viewer = live.attach(view());
    live.observe(&[2], true).unwrap();
    run(&live, Some(dir.clone()));
    (dir, collect(&viewer))
}

#[test]
fn a_replay_shows_exactly_what_the_live_run_showed() {
    let engine = Engine::new();
    let (dir, live) = record(&engine, "replay-exact");
    assert!(
        live.spikes.len() > 50 && !live.membrane.is_empty() && !live.weights.is_empty(),
        "{live:?}"
    );

    let replay = engine.open_recording(&dir).unwrap();
    assert!(replay.is_replay());
    let network = replay.build(BuildSettings::default()).unwrap();
    assert_eq!(
        network.neurons.iter().map(|n| n.name).collect::<Vec<_>>(),
        ["input-1", "input-2", "output-1"]
    );
    assert_eq!(network.synapses.len(), 2);
    let viewer = replay.attach(view());
    run(&replay, None);
    let replayed = collect(&viewer);
    assert_eq!(replayed.spikes, live.spikes, "the same spikes at the same times");
    assert_eq!(replayed.membrane, live.membrane, "the same membrane trace");
    assert_eq!(replayed.weights, live.weights, "the same final weights");
}

#[test]
fn a_replay_can_seek_and_refuses_what_only_a_live_run_can_do() {
    let engine = Engine::new();
    let (dir, live) = record(&engine, "replay-seek");
    let replay = engine.open_recording(&dir).unwrap();
    replay.build(BuildSettings::default()).unwrap();
    replay.seek(SimTime::from_start(SimDuration::from_secs(1))).unwrap();
    assert_eq!(replay.sim_time(), SimTime::from_start(SimDuration::from_secs(1)));
    let viewer = replay.attach(view());
    replay
        .start(RunSettings {
            speed: Speed::AsFastAsPossible,
            duration: Some(SimDuration::from_secs(1)),
            record: None,
        })
        .unwrap();
    assert!(replay.wait_for_run(Duration::from_secs(10)));
    let after: Vec<_> = live.spikes.iter().copied().filter(|s| s.0 >= 1_000_000_000).collect();
    assert_eq!(
        collect(&viewer).spikes,
        after,
        "exactly the recorded spikes from 1 s on"
    );

    for refused in [
        replay.stimulate(&[0], 1.0),
        replay.connect(0, 2, 1.0, None, None),
        replay.query_neuron(0).map(|_| ()),
        replay.start(RunSettings {
            speed: Speed::AsFastAsPossible,
            duration: None,
            record: Some(dir.join("again")),
        }),
    ] {
        assert!(matches!(refused, Err(SessionError::Invalid(_))), "{refused:?}");
    }
    assert!(matches!(
        engine.open_recording(Path::new("/nonexistent")),
        Err(SessionError::NotFound(_))
    ));
}

#[test]
fn a_paced_replay_pauses_and_steps() {
    let engine = Engine::new();
    let (dir, _) = record(&engine, "replay-paced");
    let replay = engine.open_recording(&dir).unwrap();
    replay.build(BuildSettings::default()).unwrap();
    replay
        .start(RunSettings {
            speed: Speed::Factor(1.0),
            duration: None,
            record: None,
        })
        .unwrap();
    std::thread::sleep(Duration::from_millis(60));
    replay.pause().unwrap();
    std::thread::sleep(Duration::from_millis(20));
    let at = replay.sim_time();
    replay.step(SimDuration::from_millis(10)).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(replay.sim_time(), at + SimDuration::from_millis(10));
    replay.stop().unwrap();
}
