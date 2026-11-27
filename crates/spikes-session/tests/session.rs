//! The session lifecycle and commands.

use std::path::Path;
use std::time::Duration;

use spikes_session::{BuildSettings, Engine, RunSettings, SessionError, SessionNotice, Speed, State};
use tempo_runtime::{SimDuration, SimTime};

fn v1(name: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../test-data/networks")
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
    let err = engine
        .create("bad", "(neurons=[(name=a, threshold=1 parsec, position=[0, 0, 0])])")
        .unwrap_err();
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
neurons=[
    (name=a, threshold=5 mV, membrane_noise=0 mV, receptors=[(modulator=ach, direct_potential=6 mV)], position=[0, 0, 0]),
],
modulators=[
    (name=ach, clearance_time=50 ms, latency=1 ms),
],
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

    let other = engine
        .create(
            "plain",
            "(
neurons=[
    (name=b, position=[0, 0, 0]),
],
)",
        )
        .unwrap();
    other.build(BuildSettings::default()).unwrap();
    assert!(matches!(other.release_modulator(0, 1.0), Err(SessionError::Invalid(_))));
    assert_eq!(engine.sessions().len(), 2);
    engine.close(other.id()).unwrap();
    assert!(engine.session(other.id()).is_err());
}

#[test]
fn stopping_right_after_starting_never_hangs() {
    // A stop() issued before the run thread gets going used to be lost (the run took its stop baseline too
    // late), leaving a paused run that never ended. Run the race many times under a watchdog.
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let engine = Engine::new();
        let session = engine.create("ring", v1("ring.boo")).unwrap();
        session.build(BuildSettings::default()).unwrap();
        for i in 0..200 {
            session.start(RunSettings::default()).unwrap();
            if i % 2 == 0 {
                session.pause().unwrap();
            }
            session.stop().unwrap();
            assert_eq!(session.state(), State::Ready);
        }
        done_tx.send(()).unwrap();
    });
    done_rx
        .recv_timeout(Duration::from_secs(30))
        .expect("start/stop deadlocked");
}

#[test]
fn a_run_stopped_while_paused_leaves_the_session_usable() {
    // Stopping a paused run used to leave the clock paused: a later step() waited for it for ever (holding the
    // session), and a later paced start() began paused.
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let engine = Engine::new();
        let session = engine.create("ring", v1("ring.boo")).unwrap();
        session.build(BuildSettings::default()).unwrap();
        session.start(RunSettings::default()).unwrap();
        session.pause().unwrap();
        session.stop().unwrap();
        let before = session.sim_time();
        session.step(ms(5)).unwrap();
        assert_eq!(session.sim_time(), before + ms(5));
        session
            .start(RunSettings {
                speed: Speed::Factor(1.0),
                duration: Some(ms(30)),
                record: None,
            })
            .unwrap();
        assert_eq!(session.state(), State::Running);
        assert!(
            session.wait_for_run(Duration::from_secs(5)),
            "the paced run reaches its end"
        );
        done_tx.send(()).unwrap();
    });
    done_rx
        .recv_timeout(Duration::from_secs(30))
        .expect("hung after stopping a paused run");
}

#[test]
fn a_query_racing_the_end_of_a_run_is_still_answered_promptly() {
    // A query that reached the control plane as the run ended used to wait for the next run, timing out after 5 s.
    let engine = Engine::new();
    let session = engine.create("ring", v1("ring.boo")).unwrap();
    session
        .build(BuildSettings {
            shards: 2,
            ..BuildSettings::default()
        })
        .unwrap();
    for _ in 0..40 {
        session
            .start(RunSettings {
                speed: Speed::AsFastAsPossible,
                duration: Some(ms(89)),
                record: None,
            })
            .unwrap();
        let t = std::time::Instant::now();
        session.query_neuron(12).unwrap();
        session.query_neuron(17).unwrap();
        assert!(t.elapsed() < Duration::from_secs(1), "{:?}", t.elapsed());
        session.stop().unwrap();
    }
}

#[test]
fn reset_puts_the_network_back_as_built_and_a_run_from_there_repeats_exactly() {
    let text = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/examples/wta-learning.boo"),
    )
    .unwrap();
    let engine = Engine::new();
    let session = engine.create("wta", text).unwrap();
    assert!(session.reset().is_err(), "nothing built yet");
    let network = session
        .build(BuildSettings {
            seed: 7,
            shards: 2,
            ..BuildSettings::default()
        })
        .unwrap();
    let n = network.neurons.len() as u32;
    let inputs: Vec<u32> = (0..n)
        .filter(|&i| network.neurons.get(i).unwrap().name.starts_with("in-"))
        .collect();
    session
        .add_input(spikes_session::InputSpec::Poisson {
            targets: inputs.clone(),
            rate_hz: 100.0,
            mv: 10.0,
            duration: None,
        })
        .unwrap();
    let state = |s: &spikes_session::Session| -> Vec<(u64, Vec<f32>)> {
        (0..n)
            .map(|i| {
                let q = s.query_neuron(i).unwrap();
                (q.spikes, q.synapses.iter().map(|x| x.1).collect())
            })
            .collect()
    };
    let built = state(&session);
    let run = |s: &spikes_session::Session| {
        s.start(RunSettings {
            speed: Speed::AsFastAsPossible,
            duration: Some(ms(2000)),
            record: None,
        })
        .unwrap();
        assert!(s.wait_for_run(Duration::from_secs(60)));
    };
    run(&session);
    let first = state(&session);
    assert_ne!(first, built, "it ran, and learned");
    // Things done after the first run don't survive a reset.
    session.connect(inputs[0], inputs[1], 1.0, None, None).unwrap();
    session.stimulate(&inputs, 20.0).unwrap();

    session.reset().unwrap();
    assert_eq!(session.state(), State::Ready);
    assert_eq!(session.sim_time(), SimTime::ZERO);
    assert_eq!(state(&session), built, "the built weights and state");
    run(&session);
    assert_eq!(state(&session), first, "the same run again, background input included");
}

#[test]
fn neurons_report_their_spike_potential() {
    let engine = Engine::new();
    let session = engine
        .create(
            "amplitudes",
            "(neuron_types=[(name=strong, spike_potential=2.5 mV)],
              neurons=[(name=a, type=strong, position=[0, 0, 0]), (name=b, position=[10, 0, 0])])"
                .to_string(),
        )
        .unwrap();
    session.build(BuildSettings::default()).unwrap();
    let network = session.network();
    let mv: Vec<f32> = network.neurons.iter().map(|n| n.spike_mv).collect();
    assert_eq!(mv, vec![2.5, 1.0], "a's type sets 2.5 mV; b has the default 1 mV");
}
