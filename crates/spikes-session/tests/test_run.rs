//! Test runs (app plan 05, I5): a session's network run with some inputs in a throwaway engine, counted per bin.

use std::time::Duration;

use spikes_session::{
    BuildSettings, Engine, InputSource, InputSpec, RunSettings, Session, SessionError, Speed, State, TestOutcome,
    TestPlan, Trial, test_run,
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

fn noisy(name: &str) -> InputSource {
    InputSource {
        name: name.into(),
        start: SimDuration::ZERO,
        input: InputSpec::Signal {
            channels: (0..8).map(|i| vec![i]).collect(),
            formula: "clamp(0.5 + 0.3 * gauss(), 0, 1)".into(),
            sample: ms(1),
            encoding: SampleEncoding::Rate {
                max_hz: 100.0,
                mv: 15.0,
            },
            key: 0,
            duration: None,
            data: Vec::new(),
        },
    }
}

fn taps(name: &str, targets: Vec<u32>, at: &[u64]) -> InputSource {
    InputSource {
        name: name.into(),
        start: SimDuration::ZERO,
        input: InputSpec::Pulses {
            targets,
            times: at.iter().map(|&t| ms(t)).collect(),
            mv: 15.0,
        },
    }
}

fn plan(sources: Vec<InputSource>) -> TestPlan {
    TestPlan {
        sources,
        trials: Vec::new(),
        duration: ms(1000),
        bin: ms(100),
        groups: vec![("first half".into(), vec![0, 1, 2, 3]), ("last".into(), vec![7])],
        budget: None,
        seed: None,
    }
}

fn session(engine: &Engine, seed: Option<u64>) -> Session {
    let s = engine.create("real", NET).unwrap();
    if let Some(seed) = seed {
        s.build(BuildSettings {
            seed,
            ..BuildSettings::default()
        })
        .unwrap();
    }
    s
}

fn run(s: &Session, plan: TestPlan) -> TestOutcome {
    test_run(s, plan).unwrap()
}

#[test]
fn a_test_run_repeats_for_a_seed_and_differs_for_another() {
    let engine = Engine::new();
    let s = session(&engine, Some(3));
    let a = run(&s, plan(vec![noisy("noise")]));
    assert!(a.complete);
    assert!(a.spikes > 100, "{}", a.spikes);
    let again = |o: &TestOutcome| (o.counts.clone(), o.groups.clone(), o.spikes);
    let b = run(&s, plan(vec![noisy("noise")]));
    assert_eq!(again(&a), again(&b));

    // The session's own seed is the default; the plan can give another.
    let same = TestPlan {
        seed: Some(3),
        ..plan(vec![noisy("noise")])
    };
    assert_eq!(again(&a), again(&run(&s, same)));
    let other = TestPlan {
        seed: Some(4),
        ..plan(vec![noisy("noise")])
    };
    assert_ne!(again(&a), again(&run(&s, other)));

    // A session that was never built uses the default build settings.
    let fresh = session(&engine, None);
    let default_seed = BuildSettings::default().seed;
    let c = run(&fresh, plan(vec![noisy("noise")]));
    let d = run(
        &fresh,
        TestPlan {
            seed: Some(default_seed),
            ..plan(vec![noisy("noise")])
        },
    );
    assert_eq!(again(&c), again(&d));
}

#[test]
fn the_real_session_is_untouched() {
    let engine = Engine::new();
    let s = session(&engine, Some(1));
    s.set_inputs(vec![noisy("real")], Vec::new()).unwrap();
    let before = (s.state(), s.sim_time(), engine.sessions().len(), s.sensors().len());
    let loaded = session(&engine, None);

    let out = run(&s, plan(vec![taps("taps", vec![0, 1], &[100, 300])]));
    run(&loaded, plan(vec![taps("taps", vec![0], &[100])]));
    assert_eq!(out.spikes, 4);
    assert_eq!(out.groups[0].spikes, 4);
    assert_eq!(out.groups[1].spikes, 0);

    assert_eq!(
        (s.state(), s.sim_time(), s.sensors().len()),
        (before.0, before.1, before.3)
    );
    assert_eq!(
        engine.sessions().len(),
        before.2 + 1,
        "no session of the throwaway is listed"
    );
    assert_eq!(loaded.state(), State::Loaded);
    assert_eq!(s.trials(), Vec::new());

    // It still runs, with the inputs it had (not the test's), and exactly as one that was never tested.
    let control = session(&Engine::new(), Some(1));
    control.set_inputs(vec![noisy("real")], Vec::new()).unwrap();
    let go = |s: &Session| {
        s.start(RunSettings {
            speed: Speed::AsFastAsPossible,
            duration: Some(ms(500)),
            record: None,
        })
        .unwrap();
        assert!(s.wait_for_run(Duration::from_secs(30)));
        (0..8).map(|i| s.query_neuron(i).unwrap().spikes).collect::<Vec<_>>()
    };
    let spikes = go(&s);
    assert!(spikes.iter().sum::<u64>() > 50);
    assert_eq!(spikes, go(&control));
    assert_eq!(s.sim_time().since_start(), ms(500));
}

#[test]
fn groups_and_bins_add_up() {
    let engine = Engine::new();
    let s = session(&engine, Some(1));
    let mut p = plan(vec![noisy("noise")]);
    p.groups = vec![
        ("all".into(), (0..8).collect()),
        ("a".into(), vec![0, 1, 2, 3]),
        ("b".into(), vec![3, 4, 5, 6, 7, 3, 99]),
        ("none".into(), vec![]),
    ];
    let o = run(&s, p);
    assert_eq!(o.counts.len(), 10);
    assert_eq!(o.counts.iter().map(|&c| u64::from(c)).sum::<u64>(), o.spikes);
    let [all, a, b, none] = &o.groups[..] else { panic!() };
    assert_eq!((all.neurons, a.neurons, b.neurons, none.neurons), (8, 4, 5, 0));
    assert_eq!(all.counts, o.counts, "a group of every neuron is the whole network");
    assert_eq!(all.spikes, o.spikes);
    assert_eq!(none.spikes, 0);
    assert_eq!(none.counts, vec![0; 10]);
    // 3 is in both groups: together they count it twice.
    let ab: Vec<u32> = a.counts.iter().zip(&b.counts).map(|(x, y)| x + y).collect();
    assert!(ab.iter().zip(&o.counts).all(|(x, y)| x >= y));
    for g in &o.groups {
        assert_eq!(g.counts.iter().map(|&c| u64::from(c)).sum::<u64>(), g.spikes);
    }

    // Pulses at known times land in known bins: bin 1 is [100, 200) ms, and 250 ms is in bin 2.
    let mut p = plan(vec![taps("taps", vec![2], &[110, 150, 250])]);
    p.duration = ms(450);
    let o = run(&s, p);
    assert_eq!(o.counts, vec![0, 2, 1, 0, 0]);
    assert_eq!(o.groups[0].counts, o.counts);
    assert_eq!(o.sim, ms(450));
}

#[test]
fn a_budget_stops_a_long_run_and_says_so() {
    // A network with connections (a run is stopped between synchronization windows, which are long without any).
    let ring = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../test-data/networks/ring.boo"
    ))
    .unwrap();
    let s = Engine::new().create("ring", ring).unwrap();
    let background = InputSource {
        name: "background".into(),
        start: SimDuration::ZERO,
        input: InputSpec::Poisson {
            targets: (0..5).collect(),
            rate_hz: 200.0,
            mv: 10.0,
            duration: None,
        },
    };
    let mut p = plan(vec![background]);
    p.groups = vec![("first".into(), (0..5).collect())];
    p.duration = SimDuration::from_secs(1_000_000);
    p.bin = SimDuration::from_secs(1000);
    p.budget = Some(Duration::from_millis(300));
    let o = run(&s, p);
    assert!(!o.complete);
    assert!(
        o.sim > SimDuration::ZERO && o.sim < SimDuration::from_secs(1_000_000),
        "{:?}",
        o.sim
    );
    assert!(o.elapsed < Duration::from_secs(20), "{:?}", o.elapsed);
    assert_eq!(o.counts.len() as u64, o.sim.as_nanos().div_ceil(1_000_000_000_000));
    assert_eq!(o.counts.iter().map(|&c| u64::from(c)).sum::<u64>(), o.spikes);
    assert_eq!(s.state(), State::Loaded, "the session itself was not built or run");
}

#[test]
fn a_replay_is_refused_and_bad_plans_are_errors() {
    let engine = Engine::new();
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("test-run-replay");
    let _ = std::fs::remove_dir_all(&dir);
    let live = session(&engine, Some(1));
    live.set_inputs(vec![taps("taps", vec![0], &[10])], Vec::new()).unwrap();
    live.start(RunSettings {
        speed: Speed::AsFastAsPossible,
        duration: Some(ms(100)),
        record: Some(dir.clone()),
    })
    .unwrap();
    assert!(live.wait_for_run(Duration::from_secs(30)));
    let replay = engine.open_recording(&dir).unwrap();
    assert!(matches!(
        test_run(&replay, plan(vec![])),
        Err(SessionError::Invalid(why)) if why.contains("replay")
    ));

    for bad in [
        TestPlan {
            duration: SimDuration::ZERO,
            ..plan(vec![])
        },
        TestPlan {
            bin: SimDuration::ZERO,
            ..plan(vec![])
        },
        TestPlan {
            bin: SimDuration::from_micros(1),
            duration: SimDuration::from_secs(100),
            ..plan(vec![])
        },
        plan(vec![taps("far", vec![99], &[1])]),
        plan(vec![noisy("twice"), noisy("twice")]),
    ] {
        assert!(matches!(
            test_run(&live, bad),
            Err(SessionError::Invalid(_) | SessionError::NotFound(_))
        ));
    }
}

#[test]
fn trials_go_through_and_are_checked_like_set_inputs() {
    let engine = Engine::new();
    let s = session(&engine, Some(1));
    let trial = |number, start, end| Trial {
        number,
        condition: "c".into(),
        start: ms(start),
        end: ms(end),
    };
    let mut p = plan(vec![taps("taps", vec![0], &[10])]);
    p.trials = vec![trial(1, 0, 400), trial(2, 500, 900)];
    assert_eq!(run(&s, p).spikes, 1);
    let mut p = plan(vec![]);
    p.trials = vec![trial(1, 500, 900), trial(2, 0, 400)];
    assert!(matches!(test_run(&s, p), Err(SessionError::Invalid(_))));
    assert_eq!(s.trials(), Vec::new(), "the session's own trials are not changed");
}

#[test]
fn a_test_run_counts_what_a_plain_run_does() {
    // The slices a test run goes in make no difference to the run: each neuron spikes as often as in one long run, but for the end.
    let engine = Engine::new();
    let s = session(&engine, Some(2));
    let mut p = plan(vec![noisy("noise")]);
    p.groups = (0..8).map(|i| (format!("n{i}"), vec![i])).collect();
    p.duration = ms(2000);
    let o = run(&s, p);

    let control = session(&Engine::new(), Some(2));
    control.set_inputs(vec![noisy("noise")], Vec::new()).unwrap();
    control
        .start(RunSettings {
            speed: Speed::AsFastAsPossible,
            duration: Some(ms(2000)),
            record: None,
        })
        .unwrap();
    assert!(control.wait_for_run(Duration::from_secs(30)));
    let plain: Vec<u64> = (0..8).map(|i| control.query_neuron(i).unwrap().spikes).collect();
    // (A spike decided just before the end can rise just after it: the plain run counts it, the test run doesn't.)
    for (g, plain) in o.groups.iter().zip(plain) {
        assert!(
            g.spikes <= plain && plain - g.spikes <= 1,
            "{}: {} against {plain}",
            g.name,
            g.spikes
        );
    }
}
