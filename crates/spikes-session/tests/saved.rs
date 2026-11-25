//! Saving a run and continuing it later, in another session and on another number of shards.

use std::time::Duration;

use spikes_session::{BuildSettings, Engine, InputSpec, NeuronSnapshot, RunSettings, Session, SessionError, Speed};
use tempo_runtime::{SimDuration, SimTime};

const NET: &str = "(
synapse_types=[
    (name=stdp_hard, decay=(time=1500 ms), release=(), learning=stdp_hard),
    (name=fixed, decay=(time=1500 ms), release=()),
],
populations=[
    (name=exc, conduction_speed=1 m/s, count=120, random=(box=(corner=[0, 0, 0], size=[300 µm, 300 µm, 300 µm]))),
    (name=inh, inhibitory=true, count=30, random=(box=(corner=[0, 0, 0], size=[300 µm, 300 µm, 300 µm]))),
],
connections=[
    (from=exc, to=exc, pattern=fan_in, count=15, type=stdp_hard, weight=uniform(0.5, 1.5)),
    (from=inh, to=exc, pattern=random, probability=0.2, type=fixed, weight=1),
    (from=exc, to=inh, pattern=fan_in, count=8, type=fixed, weight=1),
],
learning_rules=[
    (name=stdp_hard, rule=stdp_hard, depression=0.01, depression_time=10 ms, potentiation=0.02, potentiation_time=10 ms),
],
)";

fn ms(v: u64) -> SimDuration {
    SimDuration::from_millis(v)
}

fn run_for(session: &Session, duration: SimDuration) {
    session
        .start(RunSettings {
            speed: Speed::AsFastAsPossible,
            duration: Some(duration),
            record: None,
        })
        .unwrap();
    assert!(session.wait_for_run(Duration::from_secs(30)));
}

fn neurons(session: &Session) -> Vec<NeuronSnapshot> {
    (0..session.network().neurons.len() as u32)
        .map(|i| session.query_neuron(i).unwrap())
        .collect()
}

#[test]
fn a_saved_run_continues_exactly_in_another_session_on_other_shards() {
    let engine = Engine::new();
    let original = engine.create("net", NET).unwrap();
    original
        .build(BuildSettings {
            seed: 9,
            shards: 2,
            ..BuildSettings::default()
        })
        .unwrap();
    original
        .add_input(InputSpec::Poisson {
            targets: (0..120).collect(),
            rate_hz: 3_000.0,
            mv: 12.0,
            duration: None,
        })
        .unwrap();
    run_for(&original, ms(150));
    // A connection made while running, and an input added later: both are part of the saved run.
    original.connect(3, 140, 2.0, None, Some("stdp_hard")).unwrap();
    original
        .add_input(InputSpec::Periodic {
            targets: vec![5, 6, 7],
            period: ms(7),
            mv: 15.0,
            duration: Some(ms(400)),
        })
        .unwrap();
    run_for(&original, ms(50));
    let saved = original.save().unwrap();
    let synapses = original.network().synapse_count;
    run_for(&original, ms(300));
    let expected = neurons(&original);
    assert!(
        expected.iter().map(|n| n.spikes).sum::<u64>() > 1_000,
        "the network is active"
    );

    for shards in [1, 3] {
        let restored = engine
            .create("restored", "(neurons=[(name=placeholder, position=[0, 0, 0])])")
            .unwrap();
        let network = restored.load_saved(&saved, shards).unwrap();
        assert_eq!(
            network.synapse_count, synapses,
            "the connection made while running is there"
        );
        assert_eq!(restored.sim_time(), SimTime::from_start(ms(200)));
        assert_eq!(restored.description_text(), NET);
        run_for(&restored, ms(300));
        assert_eq!(neurons(&restored), expected, "{shards} shard(s)");
    }
}

#[test]
fn saving_needs_a_ready_session_and_loading_checks_the_file() {
    let engine = Engine::new();
    let session = engine.create("net", NET).unwrap();
    assert!(matches!(session.save(), Err(SessionError::State { .. })));
    session.build(BuildSettings::default()).unwrap();
    let saved = session.save().unwrap();
    let err = session.load_saved(b"not a saved run", 1).unwrap_err();
    assert!(err.to_string().contains("not a saved run"), "{err}");
    let err = session.load_saved(&saved[..saved.len() / 2], 1).unwrap_err();
    assert!(err.to_string().contains("damaged"), "{err}");
    session.load_saved(&saved, 1).unwrap();
}

#[test]
fn a_paused_run_is_saved_where_it_waits_and_keeps_running_after() {
    let engine = Engine::new();
    let reference = engine.create("reference", NET).unwrap();
    reference
        .build(BuildSettings {
            seed: 4,
            shards: 2,
            ..BuildSettings::default()
        })
        .unwrap();
    let input = InputSpec::Poisson {
        targets: (0..120).collect(),
        rate_hz: 3_000.0,
        mv: 12.0,
        duration: None,
    };
    reference.add_input(input.clone()).unwrap();
    run_for(&reference, ms(300));
    let expected = neurons(&reference);

    // The same run, paused at 100 ms (paced, so it waits for its clock), saved while paused.
    let session = engine.create("paused", NET).unwrap();
    session
        .build(BuildSettings {
            seed: 4,
            shards: 2,
            ..BuildSettings::default()
        })
        .unwrap();
    session.add_input(input).unwrap();
    run_for(&session, ms(100));
    session
        .start(RunSettings {
            speed: Speed::Factor(1.0),
            duration: Some(ms(200)),
            record: None,
        })
        .unwrap();
    session.pause().unwrap();
    // Running and not paused: refused.
    session.resume().unwrap();
    assert!(matches!(session.save(), Err(SessionError::State { .. })));
    session.pause().unwrap();
    let saved = session.save().unwrap();
    assert_eq!(session.state(), spikes_session::State::Paused, "still paused");
    session.stop().unwrap();

    let restored = engine
        .create("restored", "(neurons=[(name=placeholder, position=[0, 0, 0])])")
        .unwrap();
    restored.load_saved(&saved, 3).unwrap();
    let at = restored.sim_time();
    assert!(
        at >= SimTime::from_start(ms(100)) && at < SimTime::from_start(ms(300)),
        "{at}"
    );
    run_for(&restored, SimTime::from_start(ms(300)) - at);
    assert_eq!(neurons(&restored), expected);
}
