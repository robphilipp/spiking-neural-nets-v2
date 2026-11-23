//! Adding and pruning connections, before and during runs.

use std::time::Duration;

use spikes_session::{BuildSettings, Engine, RunSettings, SessionError, SessionNotice, Speed};
use tempo_runtime::SimDuration;

/// Two unconnected neurons 1 mm apart, no noise; `b` fires on 10 mV through a weight-10 synapse (spp = 1 mV).
const PAIR: &str = "(
NRN=[
    (nid=a, mst=5 mV, mpn=0 mV, csp=10 m/s, LOC=(cst=ct, px1=0, px2=0, px3=0)),
    (nid=b, mst=5 mV, mpn=0 mV, csp=10 m/s, rfp=1 ms, LOC=(cst=ct, px1=1000, px2=0, px3=0)),
],
LRN=[(fnc=stdp_hard, nam=fast, ina=0.01, inp=10 ms, exa=0.02, exp=10 ms)],
)";

fn ms(v: f64) -> SimDuration {
    SimDuration::from_millis_f64(v)
}

#[test]
fn connections_can_be_added_and_pruned_while_ready() {
    let engine = Engine::new();
    let s = engine.create("pair", PAIR).unwrap();
    s.build(BuildSettings::default()).unwrap();
    let notices = s.notices();

    s.connect(0, 1, 10.0, None, Some("fast")).unwrap();
    assert!(notices.try_iter().any(|n| n == SessionNotice::NetworkChanged));
    let synapses = &s.network().synapses;
    assert_eq!((synapses.len(), synapses[0].pre, synapses[0].post), (1, 0, 1));

    // a spikes at 2 ms (its rise time); the spike reaches b 0.5 ms (synaptic delay) + 0.1 ms (1 mm at 10 m/s)
    // later, at 2.6 ms, when b's spike is decided (its own rise follows).
    s.stimulate(&[0], 10.0).unwrap();
    s.step(ms(30.0)).unwrap(); // past a's 20 ms refractory period, for the next stimulus
    let b = s.query_neuron(1).unwrap();
    assert_eq!(b.spikes, 1);
    assert!(
        (b.last_spike.unwrap().since_start().as_millis_f64() - 2.6).abs() < 1e-6,
        "{:?}",
        b.last_spike
    );

    s.prune(0, 1).unwrap();
    assert!(s.network().synapses.is_empty());
    s.stimulate(&[0], 10.0).unwrap();
    s.step(ms(30.0)).unwrap();
    assert_eq!(s.query_neuron(0).unwrap().spikes, 2);
    assert_eq!(
        s.query_neuron(1).unwrap().spikes,
        1,
        "the pruned connection carries nothing"
    );

    assert!(matches!(s.prune(0, 1), Err(SessionError::NotFound(_))));
    assert!(matches!(
        s.connect(0, 1, 1.0, None, Some("nope")),
        Err(SessionError::NotFound(_))
    ));
    assert!(matches!(
        s.connect(0, 9, 1.0, None, None),
        Err(SessionError::NotFound(_))
    ));
}

#[test]
fn connections_can_be_added_and_pruned_during_a_run() {
    let engine = Engine::new();
    let s = engine.create("pair", PAIR).unwrap();
    s.build(BuildSettings {
        shards: 2,
        ..BuildSettings::default()
    })
    .unwrap();
    s.start(RunSettings {
        speed: Speed::Factor(1.0),
        duration: None,
        record: None,
    })
    .unwrap();
    std::thread::sleep(Duration::from_millis(30));

    s.connect(0, 1, 10.0, None, None).unwrap();
    s.stimulate(&[0], 10.0).unwrap();
    std::thread::sleep(Duration::from_millis(80));
    assert_eq!(
        s.query_neuron(1).unwrap().spikes,
        1,
        "the new connection carried a's spike"
    );

    s.prune(0, 1).unwrap();
    s.stimulate(&[0], 10.0).unwrap();
    std::thread::sleep(Duration::from_millis(80));
    assert_eq!(s.query_neuron(0).unwrap().spikes, 2);
    assert_eq!(s.query_neuron(1).unwrap().spikes, 1);

    // And again, after pruning: a new synapse gets a new slot.
    s.connect(0, 1, 10.0, Some(false), None).unwrap();
    s.stimulate(&[0], 10.0).unwrap();
    std::thread::sleep(Duration::from_millis(80));
    assert_eq!(s.query_neuron(1).unwrap().spikes, 2);
    assert_eq!(
        s.query_neuron(1).unwrap().synapses.len(),
        2,
        "the pruned slot is kept, so indices stay stable"
    );
    s.stop().unwrap();
}
