//! Signals that look up data files (app plan 05 §3, `data("name", t, c)`): checked when the inputs are given,
//! driven by the file's values in simulated time, repeated exactly, kept by a reset and by a saved run.

use std::time::Duration;

use spikes_session::{BuildSettings, Engine, InputSource, InputSpec, RunSettings, Session, SessionError, Speed};
use spikes_signal::{DataFile, SampleEncoding};
use tempo_runtime::SimDuration;

const NET: &str = "(
populations=[
    (name=n, count=4, line=(from=[0, 0, 0], to=[30, 0, 0]), threshold=10 mV, refractory_period=2 ms, membrane_noise=0 mV),
],
)";

fn ms(v: u64) -> SimDuration {
    SimDuration::from_millis(v)
}

/// Channel 0 is on from 100 to 300 ms, channel 1 from 400 to 600 ms (ramped edges); there is no channel 2.
const CSV: &str = "t,a,b\n0,0,0\n100,0,0\n101,1,0\n300,1,0\n301,0,0\n399,0,0\n400,0,1\n600,0,1\n601,0,0\n";

fn file(name: &str, text: &str) -> DataFile {
    DataFile {
        name: name.into(),
        bytes: text.as_bytes().to_vec(),
    }
}

/// A signal over `channels` channels (neurons 0…) with `formula`, giving its `data` files.
fn source(formula: &str, channels: u32, data: Vec<DataFile>) -> InputSource {
    InputSource {
        name: "odour".into(),
        start: SimDuration::ZERO,
        input: InputSpec::Signal {
            channels: (0..channels).map(|i| vec![i]).collect(),
            formula: formula.into(),
            sample: ms(1),
            encoding: SampleEncoding::Rate {
                max_hz: 400.0,
                mv: 15.0,
            },
            key: 0,
            duration: None,
            data,
        },
    }
}

fn built(seed: u64, shards: usize) -> Session {
    let s = Engine::new().create("data", NET).unwrap();
    s.build(BuildSettings {
        seed,
        shards,
        ..BuildSettings::default()
    })
    .unwrap();
    s
}

fn run_for(s: &Session, d: SimDuration) {
    s.start(RunSettings {
        speed: Speed::AsFastAsPossible,
        duration: Some(d),
        record: None,
    })
    .unwrap();
    assert!(s.wait_for_run(Duration::from_secs(30)));
}

fn spikes(s: &Session) -> Vec<u64> {
    (0..4).map(|i| s.query_neuron(i).unwrap().spikes).collect()
}

#[test]
fn a_signal_follows_the_data_file_in_simulated_time() {
    let s = built(1, 1);
    s.set_inputs(
        vec![source("data(\"odour\", t, c)", 2, vec![file("odour", CSV)])],
        Vec::new(),
    )
    .unwrap();
    run_for(&s, ms(350));
    let (a, b) = (spikes(&s)[0], spikes(&s)[1]);
    assert!(
        (50..=100).contains(&a),
        "channel 0 fired {a} times in its 200 ms at 400 Hz"
    );
    assert_eq!(b, 0, "channel 1 is off until 400 ms");
    assert_eq!(spikes(&s)[2..], [0, 0]);
    run_for(&s, ms(300));
    assert!(spikes(&s)[1] > 30, "channel 1 fires from 400 ms");
    assert_eq!(spikes(&s)[0], a, "channel 0 is done");
}

#[test]
fn data_is_checked_when_the_inputs_are_given_with_the_sources_name() {
    let s = built(1, 1);
    let err = |set: Vec<InputSource>| -> String {
        match s.set_inputs(set, Vec::new()).unwrap_err() {
            SessionError::Invalid(m) => m,
            other => panic!("{other:?}"),
        }
    };
    let m = err(vec![source("data(\"odour\", t, c)", 2, vec![])]);
    assert!(
        m.contains("input 'odour'") && m.contains("no data file named 'odour'"),
        "{m}"
    );
    let m = err(vec![source("data(\"odour\", t, c)", 3, vec![file("odour", CSV)])]);
    assert!(m.contains("has 2 channels, the source needs 3"), "{m}");
    let m = err(vec![source("data(\"odour\", t, 5)", 1, vec![file("odour", CSV)])]);
    assert!(m.contains("the source needs 6"), "{m}");
    let m = err(vec![source(
        "data(\"odour\", t)",
        1,
        vec![file("odour", "t,a\n1,2\n0,3\n")],
    )]);
    assert!(m.contains("data file 'odour': line 3: times must be ascending"), "{m}");
    // None of that started anything: a good set is still accepted.
    s.set_inputs(
        vec![source("data(\"odour\", t, c % 2)", 4, vec![file("odour", CSV)])],
        Vec::new(),
    )
    .unwrap();
}

#[test]
fn data_signals_repeat_exactly_and_a_reset_keeps_the_data() {
    let run = |seed, shards| {
        let s = built(seed, shards);
        s.set_inputs(
            vec![source(
                "data(\"odour\", t, c) * (0.5 + 0.5 * rand())",
                2,
                vec![file("odour", CSV)],
            )],
            Vec::new(),
        )
        .unwrap();
        run_for(&s, ms(700));
        (spikes(&s), s)
    };
    let (first, s) = run(3, 1);
    assert_eq!(first, run(3, 3).0, "the same seed, whatever the shards");
    assert_ne!(first, run(4, 1).0, "another seed");
    s.reset().unwrap();
    run_for(&s, ms(700));
    assert_eq!(spikes(&s), first, "a reset runs it again with its data");
}

#[test]
fn a_saved_run_with_data_continues_exactly() {
    let set = vec![source(
        "data(\"odour\", t, c % 2) * (0.5 + 0.5 * rand())",
        4,
        vec![file("odour", CSV)],
    )];
    let whole = built(2, 2);
    whole.set_inputs(set.clone(), Vec::new()).unwrap();
    run_for(&whole, ms(700));

    let part = built(2, 2);
    part.set_inputs(set, Vec::new()).unwrap();
    run_for(&part, ms(250));
    let bytes = part.save().unwrap();
    let resumed = Engine::new().create("resumed", NET).unwrap();
    resumed.load_saved(&bytes, 1).unwrap();
    run_for(&resumed, ms(450));
    assert_eq!(spikes(&resumed), spikes(&whole));
}
