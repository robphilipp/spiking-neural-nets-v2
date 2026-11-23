//! A session over several nodes behaves exactly like one in a single process.

use std::collections::BTreeMap;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::time::Duration;

use spikes_api::frame::decode;
use spikes_session::{
    BuildSettings, Engine, InputSpec, RunSettings, Selection, Session, SessionNotice, Speed, ViewSettings, Viewer,
};
use tempo_runtime::SimDuration;

fn data(dir: &str, name: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../test-data")
            .join(dir)
            .join(name),
    )
    .unwrap()
}

/// Starts `n` node agents in this process; returns their addresses.
fn agents(n: usize) -> Vec<String> {
    (0..n)
        .map(|_| {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap().to_string();
            std::thread::spawn(move || spikes_session::cluster::serve_node(listener));
            addr
        })
        .collect()
}

#[derive(Debug, Default, PartialEq)]
struct Seen {
    spikes: Vec<(u64, u32)>,
    membrane: Vec<(u64, u32, u32)>,
    weights: BTreeMap<(u32, u32), u32>,
}

/// Reads a viewer's frames as they come (a viewer keeps only a few waiting), until `done` and quiet.
fn collect(viewer: &Viewer, done: &std::sync::atomic::AtomicBool) -> Seen {
    let mut seen = Seen::default();
    loop {
        let Some(bytes) = viewer.recv_timeout(Duration::from_millis(200)) else {
            if done.load(std::sync::atomic::Ordering::Acquire) {
                break;
            }
            continue;
        };
        let f = decode(&bytes).unwrap();
        seen.spikes.extend(f.spikes.iter().map(|&(n, o)| (f.time_ns(o), n)));
        seen.membrane
            .extend(f.membrane.iter().map(|&(n, o, mv)| (f.time_ns(o), n, mv.to_bits())));
        for &(post, synapse, w) in &f.weights {
            seen.weights.insert((post, synapse), w.to_bits());
        }
    }
    seen.spikes.sort_unstable();
    seen.membrane.sort_unstable();
    seen
}

fn run(session: &Session, ms: u64, record: Option<PathBuf>) -> Option<spikes_record::Summary> {
    let notices = session.notices();
    session
        .start(RunSettings {
            speed: Speed::AsFastAsPossible,
            duration: Some(SimDuration::from_millis(ms)),
            record,
        })
        .unwrap();
    let ended = session.wait_for_run(Duration::from_secs(30));
    assert!(
        ended,
        "the run should end: {:?} at {} {:?}",
        session.state(),
        session.sim_time(),
        session.cluster_stats()
    );
    assert_eq!(session.failure(), None);
    notices.try_iter().find_map(|n| match n {
        SessionNotice::RunEnded { recording, .. } => recording,
        _ => None,
    })
}

/// Builds the line-sensor network (on `nodes`, if any), drives its inputs, observes some neurons, and runs it twice
/// (the second time recorded); returns what a viewer saw, a neuron's final state, and the recording's row counts.
fn line_sensor(engine: &Engine, nodes: Vec<String>, record: PathBuf) -> (Seen, String, String, String) {
    let session = engine.create("line", data("v1", "line-sensor-layers-wta.boo")).unwrap();
    session.set_nodes(nodes).unwrap();
    let network = session
        .build(BuildSettings {
            shards: 2,
            ..BuildSettings::default()
        })
        .unwrap();
    assert_eq!(network.neurons.len(), 69);
    assert_eq!(network.synapses.len(), 871, "every node's synapses are listed");
    let inputs: Vec<u32> = (0..network.neurons.len() as u32)
        .filter(|&i| network.neurons[i as usize].name.starts_with("input-"))
        .collect();
    let last = network.neurons.len() as u32 - 1;
    let viewer = session.attach(ViewSettings {
        membrane: vec![0, last],
        weights: Selection::All,
        ..ViewSettings::default()
    });
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let collector = {
        let done = std::sync::Arc::clone(&done);
        std::thread::spawn(move || collect(&viewer, &done))
    };
    session
        .add_input(InputSpec::Poisson {
            targets: inputs.clone(),
            rate_hz: 2_000.0,
            mv: 6.0,
            duration: None,
        })
        .unwrap();
    session.stimulate(&inputs[..3], 20.0).unwrap();
    run(&session, 300, None);
    // A connection added (and one pruned) between runs, across whatever nodes the neurons are on.
    session.connect(inputs[0], last, 2.0, None, None).unwrap();
    session.connect(inputs[1], last, 3.0, None, None).unwrap();
    session.connect(last, inputs[2], 1.0, Some(true), None).unwrap();
    session.prune(inputs[0], last).unwrap();
    let summary = run(&session, 300, Some(record)).expect("recorded");
    let state = format!("{:?}", session.query_neuron(last).unwrap());
    let synapses = format!("{:?}", session.network().synapses);
    done.store(true, std::sync::atomic::Ordering::Release);
    let seen = collector.join().unwrap();
    engine.close(session.id()).unwrap();
    (
        seen,
        state,
        format!("{:?} {}", summary.rows, summary.synapses),
        synapses,
    )
}

#[test]
fn a_cluster_session_sees_exactly_what_one_process_sees() {
    let engine = Engine::new();
    let tmp = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let (alone, alone_state, alone_rows, alone_synapses) = line_sensor(&engine, Vec::new(), tmp.join("cluster-alone"));
    assert!(
        alone.spikes.len() > 200 && !alone.membrane.is_empty(),
        "{} spikes",
        alone.spikes.len()
    );
    let (cluster, cluster_state, cluster_rows, cluster_synapses) =
        line_sensor(&engine, agents(2), tmp.join("cluster-3"));
    assert_eq!(cluster.spikes.len(), alone.spikes.len());
    assert_eq!(cluster.spikes, alone.spikes, "the same spikes at the same times");
    assert_eq!(cluster.membrane, alone.membrane, "the same membrane traces");
    assert_eq!(cluster.weights, alone.weights, "the same weights");
    assert_eq!(cluster_state, alone_state, "a neuron on another node answers queries");
    assert_eq!(cluster_rows, alone_rows, "the recording has everything");
    assert_eq!(
        cluster_synapses, alone_synapses,
        "the same synapses after connecting and pruning"
    );
}

/// The reward network: dopamine released from the coordinator reaches synapses on every node.
fn reward(engine: &Engine, nodes: Vec<String>) -> Vec<String> {
    let session = engine.create("reward", data("examples", "reward.boo")).unwrap();
    session.set_nodes(nodes).unwrap();
    let network = session.build(BuildSettings::default()).unwrap();
    session
        .add_input(InputSpec::Poisson {
            targets: vec![0, 1],
            rate_hz: 60.0,
            mv: 6.0,
            duration: None,
        })
        .unwrap();
    run(&session, 500, None);
    session.release_modulator(0, 1.0).unwrap();
    run(&session, 500, None);
    let states = (0..network.neurons.len() as u32)
        .map(|i| format!("{:?}", session.query_neuron(i).unwrap()))
        .collect();
    engine.close(session.id()).unwrap();
    states
}

#[test]
fn dopamine_reaches_every_node() {
    let engine = Engine::new();
    let alone = reward(&engine, Vec::new());
    let cluster = reward(&engine, agents(2));
    assert_eq!(cluster, alone);
}
