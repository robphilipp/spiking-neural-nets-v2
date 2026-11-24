//! Best effort: every node follows its own copy of the coordinator's clock; events that arrive too late are
//! delivered late and counted. Slowing the clock gives them time.

use std::net::TcpListener;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use tempo_cluster::{StrictLink, TcpTransport};
use tempo_runtime::{ClockSchedule, NodePlacement, Runtime, RuntimeConfig, SimDuration, SimTime};
use tempo_testkit::models::lif::{self, LifMsg};

const SHARDS: [usize; 3] = [1, 1, 1];

fn ms(v: u64) -> SimTime {
    SimTime::from_start(SimDuration::from_millis(v))
}

fn links() -> Vec<StrictLink<LifMsg>> {
    let listeners: Vec<TcpListener> = (0..SHARDS.len())
        .map(|_| TcpListener::bind("127.0.0.1:0").unwrap())
        .collect();
    let addresses: Vec<_> = listeners.iter().map(|l| l.local_addr().unwrap()).collect();
    std::thread::scope(|s| {
        let handles: Vec<_> = listeners
            .iter()
            .enumerate()
            .map(|(n, listener)| {
                let addresses = &addresses;
                s.spawn(move || {
                    let link = StrictLink::new(n as u32, SHARDS.len() as u32, SHARDS[n], Vec::new());
                    let t =
                        TcpTransport::connect(n as u32, listener, addresses, 7, link.sink(), Duration::from_secs(10))
                            .unwrap();
                    link.attach(Arc::new(t));
                    link.set_lockstep(false);
                    link
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    })
}

/// Runs 60 ms of the LIF network at `time_factor` wall seconds per simulated second; returns (events received
/// from other nodes, of which late) over all nodes.
fn run(time_factor: f64) -> (u64, u64) {
    let network = lif::LifConfig {
        neurons: 300,
        synapses_per_neuron: 30,
        input_until: ms(60),
        ..lif::LifConfig::default()
    };
    // Every node paces from the same instant, a little in the future.
    let schedule = ClockSchedule::Running {
        anchor_wall: SystemTime::now() + Duration::from_millis(300),
        anchor_sim: SimTime::ZERO,
        time_factor,
    };
    let totals: Vec<(u64, u64)> = std::thread::scope(|s| {
        let handles: Vec<_> = links()
            .into_iter()
            .enumerate()
            .map(|(n, link)| {
                let network = network.clone();
                s.spawn(move || {
                    let mut rt = Runtime::new(RuntimeConfig {
                        seed: 3,
                        shards: SHARDS[n],
                        min_remote_delay: Some(network.input_delay),
                        cluster: Some(NodePlacement {
                            node: n as u32,
                            shards_per_node: SHARDS.to_vec(),
                        }),
                        ..RuntimeConfig::default()
                    });
                    lif::build(&mut rt, &network);
                    rt.set_link(Arc::new(link.clone()));
                    rt.clock().follow(schedule);
                    rt.run_until(ms(60));
                    let stats = link.stats();
                    let events: u64 = stats.arrivals.iter().map(|a| a.events).sum();
                    let late: u64 = stats.arrivals.iter().map(|a| a.late).sum();
                    assert_eq!(
                        rt.stats().late_remote,
                        late,
                        "the link counts what the runtime delivered late"
                    );
                    (events, late)
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    totals.into_iter().fold((0, 0), |(e, l), (a, b)| (e + a, l + b))
}

#[test]
fn slowing_the_clock_makes_events_on_time() {
    let (events_fast, late_fast) = run(1.0);
    let (events_slow, late_slow) = run(20.0);
    eprintln!("real time: {late_fast} of {events_fast} late; 20× slower: {late_slow} of {events_slow} late");
    assert!(events_fast > 1000 && events_slow > 1000, "events should cross nodes");
    let (fast, slow) = (
        late_fast as f64 / events_fast as f64,
        late_slow as f64 / events_slow as f64,
    );
    // Timing-based, so with margins for a busy machine (alone: ~8% late at real time, ~0.3% at 20× slower).
    assert!(
        slow <= fast / 2.0,
        "slower runs have far fewer late events: {slow} vs {fast}"
    );
    assert!(slow < 0.05, "at 20× slower nearly every event is on time: {slow}");
}
