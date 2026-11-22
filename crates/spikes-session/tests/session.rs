//! The session lifecycle and commands.

use std::path::Path;
use std::time::Duration;

use spikes_session::{BuildSettings, Engine, RunSettings, SessionError, SessionNotice, Speed, State};
use tempo_runtime::{SimDuration, SimTime};

fn v1(name: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../test-data/v1")
            .join(name),
    )
    .unwrap()
}

fn ms(v: u64) -> SimDuration {
    SimDuration::from_millis(v)
}

#[test]
fn parse_errors_come_back_with_positions() {
    let engine = Engine::new();
    let err = engine.create("bad", "(NRN=[(nid=a, mst=1 parsec)])").unwrap_err();
    let SessionError::Parse(errors) = err else {
        panic!("{err}")
    };
    assert!(!errors.is_empty());
    assert!(errors[0].span.line >= 1);
}

#[test]
fn lifecycle_build_run_and_continue() {
    let engine = Engine::new();
    let session = engine.create("stdp", v1("stdp-test.boo")).unwrap();
    assert_eq!(session.state(), State::Loaded);
    assert!(matches!(
        session.start(RunSettings::default()),
        Err(SessionError::State {
            state: State::Loaded,
            ..
        })
    ));

    let network = session.build(BuildSettings::default()).unwrap();
    assert_eq!(network.neurons.len(), 3);
    assert_eq!(network.synapses.len(), 2);
    let output = network.by_name("output-1").unwrap();
    assert_eq!(session.state(), State::Ready);

    let notices = session.notices();
    session.stimulate(&[0, 1], 10.0).unwrap();
    session
        .start(RunSettings {
            speed: Speed::AsFastAsPossible,
            duration: Some(ms(100)),
            record: None,
        })
        .unwrap();
    assert!(session.wait_for_run(Duration::from_secs(10)));
    assert_eq!(session.state(), State::Ready);
    assert_eq!(session.sim_time(), SimTime::from_start(ms(100)));
    let notices: Vec<SessionNotice> = notices.try_iter().collect();
    assert!(
        notices.iter().any(|n| matches!(n, SessionNotice::RunEnded { .. })),
        "{notices:?}"
    );

    let snapshot = session.query_neuron(output).unwrap();
    assert!(snapshot.spikes >= 1, "{snapshot:?}");
    assert_eq!(snapshot.synapses.len(), 2);

    // A second run continues from 100 ms.
    session
        .start(RunSettings {
            speed: Speed::AsFastAsPossible,
            duration: Some(ms(50)),
            record: None,
        })
        .unwrap();
    assert!(session.wait_for_run(Duration::from_secs(10)));
    assert_eq!(session.sim_time(), SimTime::from_start(ms(150)));
}

#[test]
fn paced_runs_pause_step_resume_and_stop() {
    let engine = Engine::new();
    let session = engine.create("ring", v1("ring.boo")).unwrap();
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
    std::thread::sleep(Duration::from_millis(50));
    session.pause().unwrap();
    assert_eq!(session.state(), State::Paused);
    std::thread::sleep(Duration::from_millis(20)); // let the shards reach the pause
    let paused_at = session.sim_time();
    assert!(
        paused_at > SimTime::ZERO && paused_at < SimTime::from_start(ms(500)),
        "{paused_at}"
    );
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(session.sim_time(), paused_at, "time stands still while paused");

    session.step(ms(5)).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(session.sim_time(), paused_at + ms(5));

    session.set_speed(Speed::Factor(4.0)).unwrap();
    session.resume().unwrap();
    std::thread::sleep(Duration::from_millis(50));
    let advanced = session.sim_time().saturating_duration_since(paused_at + ms(5));
    assert!(advanced > ms(100), "×4 for 50 ms of wall time ≈ 200 ms: {advanced}");

    // Queries are answered while running.
    let n = session.network().by_name("hidden-101").unwrap();
    session.query_neuron(n).unwrap();

    session.stop().unwrap();
    assert_eq!(session.state(), State::Ready);
    assert!(matches!(
        session.pause(),
        Err(SessionError::State {
            state: State::Ready,
            ..
        })
    ));
}

#[test]
fn recorded_runs_write_a_recording() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("session-recording");
    let _ = std::fs::remove_dir_all(&dir);
    let engine = Engine::new();
    let session = engine.create("stdp", v1("stdp-test.boo")).unwrap();
    session.build(BuildSettings::default()).unwrap();
    session.stimulate(&[0, 1], 10.0).unwrap();
    let notices = session.notices();
    session
        .start(RunSettings {
            speed: Speed::AsFastAsPossible,
            duration: Some(ms(100)),
            record: Some(dir.clone()),
        })
        .unwrap();
    assert!(session.wait_for_run(Duration::from_secs(10)));
    let recording = notices
        .try_iter()
        .find_map(|n| match n {
            SessionNotice::RunEnded { recording, .. } => recording,
            _ => None,
        })
        .expect("a recording summary");
    assert!(recording.rows.spikes >= 3, "{recording:?}");
    assert!(dir.join("spikes.arrows").exists());
}

#[test]
fn modulators_can_be_released_and_bad_input_is_rejected() {
    let text = "(
MOD=[(nam=ach, clr=50 ms, lat=1 ms)],
NRN=[(nid=a, mst=5 mV, mpn=0 mV, LOC=(cst=ct, px1=0, px2=0, px3=0), RCP=[(mod=ach, dir=6 mV)])],
)";
    let engine = Engine::new();
    let session = engine.create("mod", text).unwrap();
    session.build(BuildSettings::default()).unwrap();
    assert!(matches!(
        session.release_modulator(3, 1.0),
        Err(SessionError::NotFound(_))
    ));
    assert!(matches!(session.stimulate(&[7], 1.0), Err(SessionError::NotFound(_))));
    assert!(matches!(
        session.set_speed(Speed::Factor(0.0)),
        Err(SessionError::Invalid(_))
    ));
    session.release_modulator(0, 1.0).unwrap();
    session.step(ms(20)).unwrap();
    assert_eq!(
        session.query_neuron(0).unwrap().spikes,
        1,
        "the direct modulator fired it"
    );

    let other = engine.create("plain", "(NRN=[(nid=b, LOC=(cst=ct, px1=0, px2=0, px3=0))])").unwrap();
    other.build(BuildSettings::default()).unwrap();
    assert!(matches!(other.release_modulator(0, 1.0), Err(SessionError::Invalid(_))));
    assert_eq!(engine.sessions().len(), 2);
    engine.close(other.id()).unwrap();
    assert!(engine.session(other.id()).is_err());
}
