//! Input sets (app plan 05): sources given once per build, generated in simulated time, repeated by a reset, saved
//! with a run, and pulses fired by hand.

use std::time::Duration;

use spikes_session::{
    BuildSettings, Engine, InputSource, InputSpec, NeuronSnapshot, RunSettings, Session, SessionError, Speed, State,
};
use spikes_signal::SampleEncoding;
use tempo_runtime::SimDuration;

/// Eight neurons in a line, no connections: what arrives is only what the inputs send.
const NET: &str = "(
populations=[
    (name=n, count=8, line=(from=[0, 0, 0], to=[70, 0, 0]), threshold=10 mV, refractory_period=2 ms, membrane_noise=0 mV),
],
)";

fn ms(v: u64) -> SimDuration {
    SimDuration::from_millis(v)
}

fn built(seed: u64, shards: usize) -> Session {
    let session = Engine::new().create("inputs", NET).unwrap();
    session
        .build(BuildSettings {
            seed,
            shards,
            ..BuildSettings::default()
        })
        .unwrap();
    session
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

fn spikes(session: &Session) -> Vec<u64> {
    (0..8).map(|i| session.query_neuron(i).unwrap().spikes).collect()
}

fn signal(name: &str, formula: &str, encoding: SampleEncoding) -> InputSource {
    InputSource {
        name: name.into(),
        start: SimDuration::ZERO,
        input: InputSpec::Signal {
            channels: (0..8).map(|i| vec![i]).collect(),
            formula: formula.into(),
            sample: ms(1),
            encoding,
            key: 0,
            duration: None,
        },
    }
}

const RATE: SampleEncoding = SampleEncoding::Rate { max_hz: 100.0, mv: 15.0 };

#[test]
fn a_signal_drives_its_channels_in_simulated_time() {
    // Channel c fires at c/7 of 100 Hz: rates rise across the channels.
    let s = built(1, 1);
    s.set_inputs(vec![signal("ramp", "c / (n - 1)", RATE)]).unwrap();
    run_for(&s, ms(5000));
    let counts = spikes(&s);
    assert_eq!(counts[0], 0, "rate 0");
    let expected = |c: u64| 500.0 * c as f64 / 7.0; // c/7 × 100 Hz × 5 s
    for c in 1..8u64 {
        let got = counts[c as usize] as f64;
        assert!((got - expected(c)).abs() < 5.0 * expected(c).sqrt() + 5.0, "channel {c}: {got} spikes");
    }
}

#[test]
fn signals_repeat_exactly_with_a_seed_whatever_the_shards_and_differ_with_another() {
    let run = |seed: u64, shards: usize| {
        let s = built(seed, shards);
        s.set_inputs(vec![signal("noise", "clamp(0.5 + 0.3 * gauss(), 0, 1) * (rand() < 0.8)", RATE)])
            .unwrap();
        run_for(&s, ms(2000));
        spikes(&s)
    };
    let a = run(3, 1);
    assert_eq!(a, run(3, 1));
    assert_eq!(a, run(3, 4), "four shards: the same run");
    assert_ne!(a, run(4, 1), "another seed: other noise");
}

#[test]
fn pulses_arrive_at_their_times_and_sources_start_when_scheduled() {
    let s = built(1, 1);
    s.set_inputs(vec![
        InputSource {
            name: "taps".into(),
            start: ms(100),
            input: InputSpec::Pulses {
                targets: vec![0, 1],
                times: vec![ms(0), ms(50), ms(400)],
                mv: 15.0,
            },
        },
        InputSource {
            name: "late".into(),
            start: ms(1000),
            input: InputSpec::Periodic {
                targets: vec![2],
                period: ms(100),
                mv: 15.0,
                duration: Some(ms(450)),
            },
        },
    ])
    .unwrap();
    run_for(&s, ms(450));
    assert_eq!(&spikes(&s)[..3], &[2, 2, 0], "pulses at 100 and 150 ms so far; the periodic input not yet");
    run_for(&s, ms(1550));
    assert_eq!(&spikes(&s)[..3], &[3, 3, 5], "and at 500 ms; then 1000, 1100, …, 1400 ms");
}

#[test]
fn inputs_are_given_once_per_build_checked_first_and_repeated_by_a_reset() {
    let s = built(5, 2);
    let bad = vec![
        signal("ok", "1", RATE),
        signal("broken", "sin(t", RATE),
    ];
    let err = s.set_inputs(bad).unwrap_err().to_string();
    assert!(err.contains("input 'broken'") && err.contains("expected ')'"), "{err}");
    let twice = vec![signal("a", "1", RATE), signal("a", "0", RATE)];
    assert!(s.set_inputs(twice).unwrap_err().to_string().contains("two sources are named 'a'"));

    // Nothing started from the failed attempts: the run is as built.
    let set = vec![signal("wave", "0.5 + 0.5 * sin(t / 50 + c)", RATE)];
    s.set_inputs(set.clone()).unwrap();
    assert!(
        matches!(s.set_inputs(set), Err(SessionError::Invalid(m)) if m.contains("already has its inputs")),
        "once per build"
    );
    run_for(&s, ms(1000));
    let first = spikes(&s);
    assert!(first.iter().sum::<u64>() > 100);

    s.reset().unwrap();
    assert_eq!(s.state(), State::Ready);
    run_for(&s, ms(1000));
    assert_eq!(spikes(&s), first, "a reset gives the same inputs again: the same run");

    // Building again forgets them.
    s.build(BuildSettings {
        seed: 5,
        shards: 2,
        ..BuildSettings::default()
    })
    .unwrap();
    run_for(&s, ms(200));
    assert_eq!(spikes(&s).iter().sum::<u64>(), 0);
}

#[test]
fn a_pulse_source_fires_by_hand() {
    let s = built(1, 1);
    s.set_inputs(vec![
        InputSource {
            name: "tap".into(),
            start: SimDuration::ZERO,
            input: InputSpec::Pulses {
                targets: vec![4],
                times: Vec::new(),
                mv: 15.0,
            },
        },
        signal("quiet", "0", RATE),
    ])
    .unwrap();
    s.fire("tap").unwrap();
    run_for(&s, ms(100));
    assert_eq!(spikes(&s)[4], 1);
    assert!(matches!(s.fire("nothing"), Err(SessionError::NotFound(_))));
    assert!(matches!(s.fire("quiet"), Err(SessionError::Invalid(_))), "not a pulse");
}

#[test]
fn a_saved_run_with_a_signal_continues_exactly() {
    let neurons = |s: &Session| -> Vec<NeuronSnapshot> { (0..8).map(|i| s.query_neuron(i).unwrap()).collect() };
    let set = vec![signal("wave", "0.5 + 0.5 * sin(t / 40 + c) * (rand() < 0.9)", RATE)];
    let whole = built(2, 2);
    whole.set_inputs(set.clone()).unwrap();
    run_for(&whole, ms(1500));

    let part = built(2, 2);
    part.set_inputs(set).unwrap();
    run_for(&part, ms(700));
    let bytes = part.save().unwrap();
    let resumed = Engine::new().create("resumed", NET).unwrap();
    resumed.load_saved(&bytes, 3).unwrap();
    run_for(&resumed, ms(800));
    assert_eq!(spikes(&resumed), spikes(&whole));
    assert_eq!(neurons(&resumed).iter().map(|n| n.spikes).sum::<u64>(), spikes(&whole).iter().sum::<u64>());
}
