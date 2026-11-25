//! Random command sequences never panic, hang or leave a session in a wrong state.

use std::path::Path;
use std::time::Duration;

use proptest::prelude::*;
use spikes_session::{
    BuildSettings, Engine, InputSpec, RunSettings, SampleBatch, SensorSpec, SessionError, Speed, State, ViewSettings,
};
use tempo_runtime::SimDuration;

#[derive(Clone, Debug)]
enum Op {
    Build { shards: usize },
    StartMax { ms: u64 },
    StartPaced { speed: f64 },
    Pause,
    Resume,
    Step { ms: u64 },
    SetSpeed(Option<f64>),
    Stop,
    Stimulate { neuron: u32 },
    Query { neuron: u32 },
    Observe { neuron: u32 },
    AddInput,
    Sensor,
    Attach,
    Edit { valid: bool },
    Connect { pre: u32, post: u32 },
    Prune { pre: u32, post: u32 },
    Wait,
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (1usize..4).prop_map(|shards| Op::Build { shards }),
        (1u64..200).prop_map(|ms| Op::StartMax { ms }),
        prop_oneof![Just(0.5), Just(1.0), Just(8.0)].prop_map(|speed| Op::StartPaced { speed }),
        Just(Op::Pause),
        Just(Op::Resume),
        (1u64..20).prop_map(|ms| Op::Step { ms }),
        proptest::option::of(prop_oneof![Just(0.25), Just(2.0)]).prop_map(Op::SetSpeed),
        Just(Op::Stop),
        (0u32..30).prop_map(|neuron| Op::Stimulate { neuron }),
        (0u32..30).prop_map(|neuron| Op::Query { neuron }),
        (0u32..30).prop_map(|neuron| Op::Observe { neuron }),
        Just(Op::AddInput),
        Just(Op::Sensor),
        Just(Op::Attach),
        any::<bool>().prop_map(|valid| Op::Edit { valid }),
        (0u32..30, 0u32..30).prop_map(|(pre, post)| Op::Connect { pre, post }),
        (0u32..30, 0u32..30).prop_map(|(pre, post)| Op::Prune { pre, post }),
        Just(Op::Wait),
    ]
}

fn ring() -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/networks/ring.boo")).unwrap()
}

/// Errors that are fine: the operation doesn't apply in this state, or its argument is out of range.
fn acceptable(result: Result<(), SessionError>) {
    match result {
        Ok(()) | Err(SessionError::State { .. } | SessionError::NotFound(_) | SessionError::Invalid(_)) => {}
        Err(e) => panic!("unexpected error: {e}"),
    }
}

fn apply(ops: Vec<Op>) {
    let engine = Engine::new();
    let session = engine.create("ring", ring()).unwrap();
    let mut viewers = Vec::new();
    let mut sensors = Vec::new();
    for op in ops {
        let before = session.state();
        match op {
            Op::Build { shards } => {
                let r = session
                    .build(BuildSettings {
                        shards,
                        ..BuildSettings::default()
                    })
                    .map(|_| ());
                acceptable(r);
                if !matches!(before, State::Running | State::Paused) {
                    assert_eq!(session.state(), State::Ready);
                }
            }
            Op::StartMax { ms } => acceptable(session.start(RunSettings {
                speed: Speed::AsFastAsPossible,
                duration: Some(SimDuration::from_millis(ms)),
                record: None,
            })),
            Op::StartPaced { speed } => acceptable(session.start(RunSettings {
                speed: Speed::Factor(speed),
                duration: Some(SimDuration::from_millis(50)),
                record: None,
            })),
            Op::Pause => acceptable(session.pause()),
            Op::Resume => acceptable(session.resume()),
            Op::Step { ms } => acceptable(session.step(SimDuration::from_millis(ms))),
            Op::SetSpeed(s) => acceptable(session.set_speed(s.map_or(Speed::AsFastAsPossible, Speed::Factor))),
            Op::Stop => {
                acceptable(session.stop());
                assert!(!matches!(session.state(), State::Running | State::Paused));
            }
            Op::Stimulate { neuron } => acceptable(session.stimulate(&[neuron], 5.0)),
            Op::Query { neuron } => acceptable(session.query_neuron(neuron).map(|_| ())),
            Op::Observe { neuron } => acceptable(session.observe(&[neuron], true)),
            Op::AddInput => acceptable(session.add_input(InputSpec::Poisson {
                targets: vec![0, 1, 2],
                rate_hz: 50.0,
                mv: 3.0,
                duration: Some(SimDuration::from_millis(100)),
            })),
            Op::Sensor => match session.connect_sensor(SensorSpec::new("s", vec![vec![0]])) {
                Ok(sensor) => {
                    acceptable(sensor.send_samples(&SampleBatch {
                        t0_ns: 0,
                        dt_ns: 1_000_000,
                        values: vec![1.0; 5],
                    }));
                    sensors.push(sensor);
                }
                Err(e) => acceptable(Err(e)),
            },
            Op::Attach => viewers.push(session.attach(ViewSettings::default())),
            Op::Edit { valid } => {
                let text = if valid {
                    ring()
                } else {
                    "(neurons=[(name=x, threshold=1 parsec, position=[0, 0, 0])])".to_string()
                };
                match session.update_description(text) {
                    Ok(()) => assert_eq!(session.state(), State::Loaded),
                    Err(SessionError::Parse(_)) => assert!(!valid),
                    Err(e) => acceptable(Err(e)),
                }
            }
            Op::Connect { pre, post } => acceptable(session.connect(pre, post, 2.0, None, None)),
            Op::Prune { pre, post } => acceptable(session.prune(pre, post)),
            Op::Wait => {
                session.wait_for_run(Duration::from_millis(20));
            }
        }
        assert_ne!(session.state(), State::Failed, "{:?}", session.failure());
        for v in &viewers {
            while v.try_recv().is_some() {}
        }
    }
    session.stop().unwrap();
    engine.close(session.id()).unwrap();
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 48, ..ProptestConfig::default() })]

    #[test]
    fn random_command_sequences(ops in proptest::collection::vec(op(), 1..14)) {
        // Under a watchdog: a deadlock fails the test instead of hanging it.
        let (tx, rx) = std::sync::mpsc::channel();
        let shown = format!("{ops:?}");
        std::thread::spawn(move || {
            let outcome = std::panic::catch_unwind(|| apply(ops));
            let _ = tx.send(outcome.is_ok());
        });
        match rx.recv_timeout(Duration::from_secs(30)) {
            Ok(true) => {}
            Ok(false) => prop_assert!(false, "panicked: {}", shown),
            Err(_) => prop_assert!(false, "hung: {}", shown),
        }
    }
}
