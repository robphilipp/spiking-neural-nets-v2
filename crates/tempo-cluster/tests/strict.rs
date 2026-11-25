//! A strict cluster run gives exactly the results of a single process.

use std::net::TcpListener;
use std::sync::Arc;
use std::time::Duration;

use tempo_cluster::{MemoryNetwork, StrictLink, TcpTransport};
use tempo_runtime::{ActorId, NodePlacement, Runtime, RuntimeConfig, SimDuration, SimTime, TraceEntry};
use tempo_testkit::models::lif::{self, LifEvent, LifMsg};
use tempo_testkit::trace_hash;

fn ms(v: u64) -> SimTime {
    SimTime::from_start(SimDuration::from_millis(v))
}

fn network() -> lif::LifConfig {
    lif::LifConfig {
        neurons: 400,
        synapses_per_neuron: 30,
        input_until: ms(40),
        ..lif::LifConfig::default()
    }
}

fn config(shards: usize, cluster: Option<NodePlacement>) -> RuntimeConfig {
    RuntimeConfig {
        seed: 11,
        trace: true,
        shards,
        min_remote_delay: Some(network().input_delay),
        cluster,
        ..RuntimeConfig::default()
    }
}

type Outcome = (Vec<TraceEntry>, Vec<(SimTime, ActorId)>, u64);

fn finish(mut rt: Runtime<LifMsg, LifEvent>) -> Outcome {
    let spikes = rt.take_events().into_iter().map(|e| (e.at, e.src)).collect();
    let stats = rt.stats();
    if rt.placement().nodes() > 1 {
        // (A spike crosses to a node once, however many of its synapses are there.)
        assert!(
            stats.node_sends > 300 && stats.node_receives > 300,
            "events should cross nodes: {stats:?}"
        );
    }
    (rt.take_trace(), spikes, stats.lookahead_violations + stats.late_remote)
}

fn single() -> (u64, Vec<(SimTime, ActorId)>) {
    let mut rt = Runtime::new(config(2, None));
    rt.buffer_events();
    lif::build(&mut rt, &network());
    rt.run_until(ms(50));
    let (trace, spikes, violations) = finish(rt);
    assert_eq!(violations, 0);
    (trace_hash(&trace), spikes)
}

/// Every node's trace and spikes, merged.
fn merge(outcomes: Vec<Outcome>) -> (u64, Vec<(SimTime, ActorId)>) {
    let mut trace = Vec::new();
    let mut spikes = Vec::new();
    for (t, s, violations) in outcomes {
        assert_eq!(violations, 0, "late or early events");
        trace.extend(t);
        spikes.extend(s);
    }
    spikes.sort();
    (trace_hash(&trace), spikes)
}

const SHARDS: [usize; 3] = [2, 1, 3];

fn placement(node: u32) -> NodePlacement {
    NodePlacement {
        node,
        shards_per_node: SHARDS.to_vec(),
    }
}

/// One node: builds the whole network, keeps its share, runs in step with the others.
fn node(node: u32, link: StrictLink<LifMsg>, steps: &[u64]) -> Outcome {
    node_with(node, link, |rt| {
        for &end in steps {
            rt.run_until(ms(end));
        }
    })
}

fn node_with(node: u32, link: StrictLink<LifMsg>, run: impl FnOnce(&mut Runtime<LifMsg, LifEvent>)) -> Outcome {
    let mut rt = Runtime::new(config(SHARDS[node as usize], Some(placement(node))));
    rt.buffer_events();
    lif::build(&mut rt, &network());
    rt.set_link(Arc::new(link));
    run(&mut rt);
    finish(rt)
}

fn memory_links() -> Vec<StrictLink<LifMsg>> {
    let net = MemoryNetwork::new(SHARDS.len() as u32);
    (0..SHARDS.len() as u32)
        .map(|n| {
            let link = StrictLink::new(n, SHARDS.len() as u32, SHARDS[n as usize], Vec::new());
            link.attach(net.transport(n, link.sink()));
            link
        })
        .collect()
}

fn run_cluster(links: Vec<StrictLink<LifMsg>>, steps: &[u64]) -> (u64, Vec<(SimTime, ActorId)>) {
    let outcomes = std::thread::scope(|s| {
        let handles: Vec<_> = links
            .into_iter()
            .enumerate()
            .map(|(n, link)| s.spawn(move || node(n as u32, link, steps)))
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    merge(outcomes)
}

#[test]
fn three_nodes_in_memory_match_one_process() {
    let (hash, spikes) = single();
    assert!(spikes.len() > 100, "the network should be active");
    assert_eq!(run_cluster(memory_links(), &[50]), (hash, spikes.clone()));
    // Several runs in a row (start, stop, start) agree too.
    assert_eq!(run_cluster(memory_links(), &[10, 11, 30, 50]), (hash, spikes));
}

#[test]
fn three_nodes_over_tcp_match_one_process() {
    let (hash, spikes) = single();
    let listeners: Vec<TcpListener> = (0..SHARDS.len())
        .map(|_| TcpListener::bind("127.0.0.1:0").unwrap())
        .collect();
    let addresses: Vec<_> = listeners.iter().map(|l| l.local_addr().unwrap()).collect();
    let links: Vec<StrictLink<LifMsg>> = std::thread::scope(|s| {
        let handles: Vec<_> = listeners
            .iter()
            .enumerate()
            .map(|(n, listener)| {
                let addresses = &addresses;
                s.spawn(move || {
                    let link = StrictLink::new(n as u32, SHARDS.len() as u32, SHARDS[n], Vec::new());
                    let transport =
                        TcpTransport::connect(n as u32, listener, addresses, 42, link.sink(), Duration::from_secs(10))
                            .unwrap();
                    link.attach(Arc::new(transport));
                    link
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    assert_eq!(run_cluster(links, &[50]), (hash, spikes));
}

#[test]
fn the_coordinators_clock_paces_and_stops_the_whole_cluster() {
    let (hash, spikes) = single();
    // The coordinator runs at a quarter of real time; the other nodes are unpaced and follow its decisions.
    let links = memory_links();
    let started = std::time::Instant::now();
    let outcomes: Vec<Outcome> = std::thread::scope(|s| {
        let handles: Vec<_> = links
            .into_iter()
            .enumerate()
            .map(|(n, link)| {
                s.spawn(move || {
                    node_with(n as u32, link, |rt| {
                        if n == 0 {
                            rt.clock().set_time_factor(0.25);
                        }
                        rt.run_until(ms(50));
                    })
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let wall = started.elapsed();
    assert!(
        wall >= Duration::from_micros(12_000),
        "paced by the coordinator: {wall:?}"
    );
    assert_eq!(merge(outcomes), (hash, spikes));

    // Stopping the coordinator's clock ends every node's run, at the same simulation time.
    let links = memory_links();
    let nows: Vec<SimTime> = std::thread::scope(|s| {
        let handles: Vec<_> = links
            .into_iter()
            .enumerate()
            .map(|(n, link)| {
                s.spawn(move || {
                    let mut rt = Runtime::new(config(SHARDS[n], Some(placement(n as u32))));
                    lif::build(&mut rt, &network());
                    rt.set_link(Arc::new(link));
                    if n == 0 {
                        rt.clock().set_time_factor(1.0);
                        let clock = rt.clock();
                        std::thread::spawn(move || {
                            std::thread::sleep(Duration::from_millis(30));
                            clock.stop();
                        });
                    }
                    rt.run(); // until stopped
                    rt.now()
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    // (Paced at real time for 30 ms: on a loaded machine the cluster may get through much less than that.)
    assert!(
        nows[0] > SimTime::ZERO && nows[0] < ms(200),
        "stopped after about 30 ms: {:?}",
        nows[0]
    );
    assert!(
        nows.iter().all(|&t| t == nows[0]),
        "every node stopped at the same time: {nows:?}"
    );
}
