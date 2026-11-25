//! The transport-independent protocol connection, driven with JSON and binary messages as a client would.

use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use spikes_api::frame::decode;
use spikes_api::input::InputBatch;
use spikes_api::protocol::{Notice, Outcome, Response, ServerMessage};
use spikes_session::{Connection, Engine, Outgoing};

fn ring() -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/v1/ring.boo")).unwrap()
}

struct Client {
    connection: Connection,
    out: Receiver<Outgoing>,
    next_id: u32,
    frames: Vec<Vec<u8>>,
    notices: Vec<(u32, Notice)>,
}

impl Client {
    fn new(recordings: &Path) -> Self {
        let (connection, out) = Connection::new(Arc::new(Engine::new()), recordings);
        Self {
            connection,
            out,
            next_id: 1,
            frames: Vec::new(),
            notices: Vec::new(),
        }
    }

    /// Sends a request (as JSON text) and waits for its response, collecting frames and notices meanwhile.
    fn call(&mut self, request: Value) -> Outcome {
        let id = self.next_id;
        self.next_id += 1;
        self.connection
            .handle_text(&json!({ "id": id, "request": request }).to_string());
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match self
                .out
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("a response")
            {
                Outgoing::Message(ServerMessage::Response { id: got, result }) if got == id => return result,
                other => self.keep(other),
            }
        }
    }

    fn ok(&mut self, request: Value) -> Response {
        match self.call(request.clone()) {
            Outcome::Ok(r) => r,
            Outcome::Error(e) => panic!("{request} failed: {e:?}"),
        }
    }

    fn keep(&mut self, o: Outgoing) {
        match o {
            Outgoing::Frame(f) => self.frames.push(f),
            Outgoing::Message(ServerMessage::Notice { session, notice }) => self.notices.push((session, notice)),
            Outgoing::Message(other) => panic!("unexpected {other:?}"),
        }
    }

    fn drain(&mut self, for_ms: u64) {
        let until = Instant::now() + Duration::from_millis(for_ms);
        while let Ok(o) = self.out.recv_timeout(until.saturating_duration_since(Instant::now())) {
            self.keep(o);
        }
    }
}

#[test]
fn a_session_through_the_protocol() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("connection-recordings");
    let _ = std::fs::remove_dir_all(&dir);
    let mut c = Client::new(&dir);

    let Response::Session { session } = c.ok(json!({"type": "createSession", "name": "ring", "description": ring()}))
    else {
        panic!()
    };
    let s = session.id;
    c.ok(json!({"type": "build", "session": s, "settings": {"shards": 2}}));
    let Response::Network { network } = c.ok(json!({"type": "getNetwork", "session": s})) else {
        panic!()
    };
    assert_eq!(network.neurons.len(), 25);
    let Response::Viewer { viewer } = c.ok(json!({
        "type": "attach", "session": s,
        "view": {"spikes": {"type": "all"}, "weights": {"type": "nothing"}}
    })) else {
        panic!()
    };

    let group1: Vec<u32> = network
        .neurons
        .iter()
        .filter(|n| n.name.starts_with("hidden-10"))
        .map(|n| n.index)
        .collect();
    c.ok(json!({"type": "stimulate", "session": s, "neurons": group1, "mv": 10.0}));
    c.ok(json!({"type": "start", "session": s, "speed": null, "durationMs": 200.0, "record": true}));
    c.drain(400);

    let spikes: usize = c
        .frames
        .iter()
        .map(|f| decode(f).unwrap())
        .inspect(|f| assert_eq!(u32::from(f.header.viewer), viewer))
        .map(|f| f.spikes.len())
        .sum();
    assert_eq!(spikes, 25);
    let recording = c.notices.iter().find_map(|(_, n)| match n {
        Notice::RunEnded { sim_ms, recording } => {
            assert_eq!(*sim_ms, 200.0);
            recording.clone()
        }
        _ => None,
    });
    let recording = recording.expect("a recording id");
    let Response::Recordings { recordings } = c.ok(json!({"type": "listRecordings"})) else {
        panic!()
    };
    assert!(
        recordings
            .iter()
            .any(|r| r.id == recording && r.files.iter().any(|f| f.name == "spikes.arrows"))
    );

    let Response::Neuron { neuron } = c.ok(json!({"type": "queryNeuron", "session": s, "neuron": 0})) else {
        panic!()
    };
    assert_eq!(neuron.spikes, 1);
    let Response::Sessions { sessions } = c.ok(json!({"type": "listSessions"})) else {
        panic!()
    };
    assert_eq!(sessions[0].sim_ms, 200.0);

    // Errors carry codes (and spans for parse errors).
    let Outcome::Error(e) =
        c.call(json!({"type": "createSession", "name": "bad", "description": "(NRN=[(nid=a, mst=1 parsec)])"}))
    else {
        panic!()
    };
    assert_eq!(serde_json::to_value(e.code).unwrap(), "parse");
    assert!(!e.spans.is_empty());
    let Outcome::Error(e) = c.call(json!({"type": "pause", "session": s})) else {
        panic!()
    };
    assert_eq!(serde_json::to_value(e.code).unwrap(), "invalidState");
    let Outcome::Error(e) = c.call(json!({"type": "queryNeuron", "session": 99, "neuron": 0})) else {
        panic!()
    };
    assert_eq!(serde_json::to_value(e.code).unwrap(), "notFound");
    c.connection.handle_text("{not json");
    match c.out.recv_timeout(Duration::from_secs(1)).unwrap() {
        Outgoing::Message(ServerMessage::Response {
            id: 0,
            result: Outcome::Error(_),
        }) => {}
        other => panic!("{other:?}"),
    }
}

#[test]
fn sensors_send_binary_batches() {
    let mut c = Client::new(Path::new(env!("CARGO_TARGET_TMPDIR")));
    let Response::Session { session } = c.ok(json!({"type": "createSession", "name": "ring", "description": ring()}))
    else {
        panic!()
    };
    let s = session.id;
    c.ok(json!({"type": "build", "session": s}));
    c.ok(json!({"type": "attach", "session": s, "view": {"spikes": {"type": "all"}, "weights": {"type": "nothing"}}}));
    let Response::Sensor { sensor } = c.ok(json!({
        "type": "connectSensor", "session": s,
        "sensor": {"name": "probe", "targets": [[0], [1]], "encoding": {"type": "current", "gainMv": 20.0}}
    })) else {
        panic!()
    };
    c.ok(json!({"type": "start", "session": s, "speed": 1.0}));
    // One sample on each channel, strong enough to fire neurons 0 and 1.
    let batch = InputBatch::Samples {
        sensor,
        seq: 0,
        t0_ns: 0,
        dt_ns: 1_000_000,
        channels: 2,
        values: vec![1.0, 1.0],
    };
    c.connection.handle_binary(&batch.encode()).unwrap();
    assert!(
        c.connection
            .handle_binary(
                &InputBatch::Samples {
                    sensor: 9,
                    seq: 1,
                    t0_ns: 0,
                    dt_ns: 1_000_000,
                    channels: 2,
                    values: vec![1.0, 1.0],
                }
                .encode()
            )
            .is_err()
    );
    c.drain(200);
    c.ok(json!({"type": "stop", "session": s}));
    let fired: std::collections::BTreeSet<u32> = c
        .frames
        .iter()
        .flat_map(|f| decode(f).unwrap().spikes.into_iter().map(|(n, _)| n))
        .collect();
    assert!(fired.contains(&0) && fired.contains(&1), "{fired:?}");
    assert!(c.notices.iter().any(|(_, n)| matches!(n, Notice::StateChanged { .. })));
}

#[test]
fn a_run_saved_through_the_protocol_opens_as_a_new_session() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("connection-saves");
    let _ = std::fs::remove_dir_all(&dir);
    let mut c = Client::new(&dir);
    let Response::Session { session } = c.ok(json!({"type": "createSession", "name": "ring", "description": ring()}))
    else {
        panic!()
    };
    let s = session.id;
    // Not built yet: nothing to save.
    let Outcome::Error(e) = c.call(json!({"type": "saveRun", "session": s, "name": "early"})) else {
        panic!()
    };
    assert_eq!(serde_json::to_value(e.code).unwrap(), "invalidState");

    c.ok(json!({"type": "build", "session": s, "settings": {"shards": 1}}));
    c.ok(json!({"type": "stimulate", "session": s, "neurons": [0, 1, 2], "mv": 10.0}));
    c.ok(json!({"type": "start", "session": s, "speed": null, "durationMs": 120.0}));
    c.drain(300);
    let Response::Saved { save } = c.ok(json!({"type": "saveRun", "session": s, "name": "ring at 120 ms"})) else {
        panic!()
    };
    assert_eq!(save.name, "ring_at_120_ms");
    assert_eq!(save.at_ms, 120.0);
    assert!(save.bytes > 0.0);
    let Response::Saves { saves } = c.ok(json!({"type": "listSaves"})) else {
        panic!()
    };
    assert_eq!(saves, std::slice::from_ref(&save));
    // Saved runs aren't recordings.
    let Response::Recordings { recordings } = c.ok(json!({"type": "listRecordings"})) else {
        panic!()
    };
    assert!(recordings.is_empty(), "{recordings:?}");

    let Response::Session { session: opened } = c.ok(json!({"type": "openSave", "id": save.id, "shards": 2})) else {
        panic!()
    };
    assert_ne!(opened.id, s);
    assert_eq!(opened.sim_ms, 120.0);
    let Response::Network { network } = c.ok(json!({"type": "getNetwork", "session": opened.id})) else {
        panic!()
    };
    assert_eq!(network.neurons.len(), 25);
    let Outcome::Error(e) = c.call(json!({"type": "openSave", "id": "nope.spksave"})) else {
        panic!()
    };
    assert_eq!(serde_json::to_value(e.code).unwrap(), "notFound");
}
