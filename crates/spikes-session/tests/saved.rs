//! Saving a run and continuing it later, in another session and on another number of shards.

use std::time::Duration;

use spikes_session::{BuildSettings, Engine, InputSpec, NeuronSnapshot, RunSettings, Session, SessionError, Speed};
use tempo_runtime::{SimDuration, SimTime};

const NET: &str = "(
POP=[
    (nam=exc, cnt=120, csp=1 m/s, REG=(cst=bx, px1=0, px2=0, px3=0, sx1=300 µm, sx2=300 µm, sx3=300 µm)),
    (nam=inh, cnt=30, inh=t, REG=(cst=bx, px1=0, px2=0, px3=0, sx1=300 µm, sx2=300 µm, sx3=300 µm)),
],
RUL=[
    (prp=exc, psp=exc, cnt=15, cnw=r0.5:1.5, lrn=stdp_hard),
    (prp=inh, psp=exc, prb=0.2, cnw=1),
    (prp=exc, psp=inh, cnt=8, cnw=1),
],
LRN=[(fnc=stdp_hard, ina=0.01, inp=10 ms, exa=0.02, exp=10 ms)],
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
        let restored = engine.create("restored", "(NRN=[])").unwrap();
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
