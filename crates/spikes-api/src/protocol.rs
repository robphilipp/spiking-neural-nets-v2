//! JSON messages between clients and the engine (app plan 02 §1–3). TypeScript types are generated from these
//! with `ts-rs` (`cargo test -p spikes-api` writes them to `packages/spikes-client/src/generated/`).
//!
//! Conventions: field and variant names are camelCase; times are milliseconds (`f64`); neurons are dense `u32`
//! indices in description order; ids are `u32`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A request from a client, with an id echoed in its response.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ClientMessage {
    /// Chosen by the client; echoed in the response.
    pub id: u32,
    /// What to do.
    pub request: Request,
}

/// A message from the engine.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
#[ts(export)]
pub enum ServerMessage {
    /// The answer to a request.
    Response {
        /// The request's id.
        id: u32,
        /// What happened.
        result: Outcome,
    },
    /// Something happened to a session.
    Notice {
        /// The session.
        session: u32,
        /// What happened.
        notice: Notice,
    },
}

/// A response's result: a value, or an error.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Outcome {
    /// Success.
    Ok(Response),
    /// Failure.
    Error(ApiError),
}

/// Why a request failed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ApiError {
    /// What kind of failure.
    pub code: ErrorCode,
    /// A message for people.
    pub message: String,
    /// For parse errors: where in the description.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spans: Vec<SourceError>,
}

/// Kinds of failure.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ErrorCode {
    /// The description doesn't parse (see `spans`).
    Parse,
    /// Not allowed in the session's current state.
    InvalidState,
    /// No such session, neuron, viewer, sensor or modulator.
    NotFound,
    /// An argument is out of range or malformed.
    Invalid,
    /// Recording failed.
    Io,
    /// The simulation didn't answer in time.
    Timeout,
    /// A bug.
    Internal,
}

/// A parse error's position.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SourceError {
    /// Line, from 1.
    pub line: u32,
    /// Column in characters, from 1.
    pub column: u32,
    /// What's wrong.
    pub message: String,
}

/// Everything a client can ask.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
#[ts(export)]
pub enum Request {
    /// All sessions.
    ListSessions,
    /// A new session from a `.boo` description.
    CreateSession {
        /// A name for people.
        name: String,
        /// The description's text.
        description: String,
    },
    /// Replaces a session's description (not while running); the session goes back to `loaded`.
    UpdateDescription {
        /// The session.
        session: u32,
        /// The new text.
        description: String,
    },
    /// Stops and removes a session.
    CloseSession {
        /// The session.
        session: u32,
    },
    /// Builds (or rebuilds) the network.
    Build {
        /// The session.
        session: u32,
        /// Settings; omitted ones take their defaults.
        #[serde(default)]
        settings: BuildSettings,
    },
    /// The session's description text.
    GetDescription {
        /// The session.
        session: u32,
    },
    /// How the session's cluster is doing (only for a session built on nodes).
    GetCluster {
        /// The session.
        session: u32,
    },
    /// Where the built network runs: its nodes and shards, each neuron's node and shard, and its populations and
    /// sets with their pins.
    GetDeployment {
        /// The session.
        session: u32,
    },
    /// The built network: neurons, synapses, modulators.
    GetNetwork {
        /// The session.
        session: u32,
    },
    /// Starts or continues a run.
    Start {
        /// The session.
        session: u32,
        /// Simulated seconds per wall second; `null` for as fast as possible.
        speed: Option<f64>,
        /// Run this long, then stop; `null` until stopped.
        #[serde(default)]
        duration_ms: Option<f64>,
        /// Record the run (the engine chooses where; see `listRecordings`).
        #[serde(default)]
        record: bool,
        /// For a session built on nodes, at a set speed: keep every node in lockstep (exact; the default), or let
        /// each follow its copy of the clock (best effort: spikes may arrive late; see `getCluster`).
        #[serde(default)]
        #[ts(optional)]
        lockstep: Option<bool>,
    },
    /// Pauses a run.
    Pause {
        /// The session.
        session: u32,
    },
    /// Resumes a paused run.
    Resume {
        /// The session.
        session: u32,
    },
    /// Advances a paused run, or a ready session, by this much.
    Step {
        /// The session.
        session: u32,
        /// How far.
        ms: f64,
    },
    /// Changes the speed.
    SetSpeed {
        /// The session.
        session: u32,
        /// Simulated seconds per wall second; `null` for as fast as possible.
        speed: Option<f64>,
    },
    /// Stops a run.
    Stop {
        /// The session.
        session: u32,
    },
    /// Sends input to neurons now.
    Stimulate {
        /// The session.
        session: u32,
        /// Neuron indices.
        neurons: Vec<u32>,
        /// Input in mV.
        mv: f32,
    },
    /// Releases a modulator at the network's release site.
    ReleaseModulator {
        /// The session.
        session: u32,
        /// Modulator index.
        modulator: u16,
        /// How much.
        amount: f32,
    },
    /// Adds a synthetic input (only while ready).
    AddInput {
        /// The session.
        session: u32,
        /// The input.
        input: Input,
    },
    /// Adds a connection, now (also while running).
    Connect {
        /// The session.
        session: u32,
        /// Sending neuron.
        pre: u32,
        /// Receiving neuron.
        post: u32,
        /// Initial (and equilibrium) weight.
        weight: f32,
        /// Inhibitory; `null` for the sending neuron's default.
        #[serde(default)]
        inhibitory: Option<bool>,
        /// A learning rule from the description; `null` for none.
        #[serde(default)]
        learning: Option<String>,
    },
    /// Removes a connection (also while running).
    Prune {
        /// The session.
        session: u32,
        /// Sending neuron.
        pre: u32,
        /// Receiving neuron.
        post: u32,
    },
    /// A neuron's current state.
    QueryNeuron {
        /// The session.
        session: u32,
        /// Neuron index.
        neuron: u32,
    },
    /// Turns membrane recording on or off for neurons.
    Observe {
        /// The session.
        session: u32,
        /// Neuron indices.
        neurons: Vec<u32>,
        /// On or off.
        on: bool,
    },
    /// Starts a live view; frames arrive as binary messages.
    Attach {
        /// The session.
        session: u32,
        /// What to show.
        view: View,
    },
    /// Changes a view.
    UpdateView {
        /// The session.
        session: u32,
        /// The viewer.
        viewer: u32,
        /// What to show.
        view: View,
    },
    /// Ends a view.
    Detach {
        /// The session.
        session: u32,
        /// The viewer.
        viewer: u32,
    },
    /// Connects an external sensor; samples are sent as binary input batches.
    ConnectSensor {
        /// The session.
        session: u32,
        /// The sensor.
        sensor: SensorSpec,
    },
    /// Disconnects a sensor.
    DisconnectSensor {
        /// The session.
        session: u32,
        /// The sensor.
        sensor: u32,
    },
    /// The sensors connected to a session (through any connection).
    ListSensors {
        /// The session.
        session: u32,
    },
    /// Recorded runs.
    ListRecordings,
    /// Opens a recorded run (by its id from `listRecordings`) as a replay session; build and start it like a run.
    OpenRecording {
        /// The recording.
        id: String,
    },
    /// Converts a recorded run's tables to Parquet (sorted by time), beside its Arrow files.
    ExportParquet {
        /// The recording.
        id: String,
    },
    /// A replay: jumps to this simulated time (not while running).
    Seek {
        /// The session.
        session: u32,
        /// Where to.
        ms: f64,
    },
    /// Saves a session's run so far (when ready: built, or stopped), to continue later with `openSave`.
    SaveRun {
        /// The session.
        session: u32,
        /// A name for it.
        name: String,
    },
    /// Saved runs.
    ListSaves,
    /// Opens a saved run as a new session, ready at the saved time: start it to continue.
    OpenSave {
        /// The saved run (its id from `listSaves`).
        id: String,
        /// Worker threads (default 1, as for `build`); on each node, with `nodes`.
        #[serde(default)]
        shards: Option<u32>,
        /// Node agents to build it on (as for `build`); none: the engine alone. It may have been saved on others.
        #[serde(default)]
        nodes: Option<Vec<String>>,
    },
}

/// How to build a network.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct BuildSettings {
    /// Random seed (default 1).
    #[ts(optional)]
    pub seed: Option<u32>,
    /// Worker threads (default 1).
    #[ts(optional)]
    pub shards: Option<u32>,
    /// The shortest connection delay in ms (default 0.1).
    #[ts(optional)]
    pub min_delay_ms: Option<f64>,
    /// Report learning weight changes (default true).
    #[ts(optional)]
    pub record_weights: Option<bool>,
    /// Node agents (`host:port`, each a `spikes-server --node`) to spread the network over, as one cluster this
    /// server coordinates; omitted or empty for this server alone. Each node runs `shards` threads.
    #[ts(optional)]
    pub nodes: Option<Vec<String>>,
}

/// A synthetic input running inside the simulation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
#[ts(export)]
pub enum Input {
    /// Events at Poisson times, each to one random target.
    Poisson {
        /// Neuron indices.
        targets: Vec<u32>,
        /// Mean rate over all targets.
        rate_hz: f64,
        /// Input per event.
        mv: f32,
        /// How long; `null` for ever.
        #[serde(default)]
        duration_ms: Option<f64>,
    },
    /// Every target, every period.
    Periodic {
        /// Neuron indices.
        targets: Vec<u32>,
        /// Interval.
        period_ms: f64,
        /// Input per pulse.
        mv: f32,
        /// How long; `null` for ever.
        #[serde(default)]
        duration_ms: Option<f64>,
    },
}

/// Which neurons a view wants.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
#[ts(export)]
pub enum Selection {
    /// All of them.
    All,
    /// These.
    Neurons {
        /// Neuron indices.
        neurons: Vec<u32>,
    },
    /// None.
    Nothing,
}

/// What a live view shows.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct View {
    /// Wall-clock ms between frames (default 16).
    #[serde(default = "default_frame_ms")]
    pub frame_ms: f64,
    /// Bytes per second the client can take (default 4 MB/s).
    #[serde(default = "default_budget")]
    pub budget_bytes_per_s: u32,
    /// Spikes from these neurons.
    pub spikes: Selection,
    /// Membrane potential of these neurons, on every update.
    #[serde(default)]
    pub membrane: Vec<u32>,
    /// Weight changes of synapses onto these neurons.
    pub weights: Selection,
}

fn default_frame_ms() -> f64 {
    16.0
}

fn default_budget() -> u32 {
    4 << 20
}

/// An external sensor.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SensorSpec {
    /// A name for people.
    pub name: String,
    /// Neurons (by index) each channel drives; its length is the channel count.
    pub targets: Vec<Vec<u32>>,
    /// How samples become input.
    pub encoding: Encoding,
    /// Time from a batch's arrival to its first sample, ms (default 20): covers the batch interval plus jitter.
    #[serde(default)]
    #[ts(optional)]
    pub ingress_delay_ms: Option<f64>,
    /// Keep samples that arrive while paused, instead of dropping them (default false: live sensors).
    #[serde(default)]
    pub buffer_while_paused: bool,
}

/// How a sensor's samples become neuron input.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
#[ts(export)]
pub enum Encoding {
    /// `gainMv × value` per sample.
    Current {
        /// mV per unit.
        gain_mv: f32,
    },
    /// Poisson events at `clamp(value, 0, 1) × maxHz`.
    Rate {
        /// Rate at value 1.
        max_hz: f64,
        /// Input per event.
        mv: f32,
    },
    /// An event per `level` of change, `±mv`.
    Threshold {
        /// Change per event.
        level: f32,
        /// Input per event.
        mv: f32,
    },
    /// The sensor sends events itself.
    Spikes {
        /// Input per event.
        mv: f32,
    },
}

/// Successful results.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
#[ts(export)]
pub enum Response {
    /// Done; nothing to return.
    Done,
    /// The sessions.
    Sessions {
        /// One summary each.
        sessions: Vec<SessionSummary>,
    },
    /// One session.
    Session {
        /// Its summary.
        session: SessionSummary,
    },
    /// A built network.
    Network {
        /// The network.
        network: Network,
    },
    /// A description's text.
    Description {
        /// The `.boo` text.
        text: String,
    },
    /// A cluster's state.
    Cluster {
        /// The state.
        cluster: Cluster,
    },
    /// Where a network runs.
    Deployment {
        /// The deployment.
        deployment: Deployment,
    },
    /// A neuron's state.
    Neuron {
        /// The state.
        neuron: NeuronState,
    },
    /// A new viewer.
    Viewer {
        /// Its id; its frames carry it in their header (`viewer`, modulo 2¹⁶).
        viewer: u32,
    },
    /// A new sensor.
    Sensor {
        /// Its id, used in binary input batches.
        sensor: u32,
    },
    /// A session's connected sensors.
    Sensors {
        /// One each.
        sensors: Vec<SensorInfo>,
    },
    /// Recorded runs.
    Recordings {
        /// One each.
        recordings: Vec<Recording>,
    },
    /// A saved run.
    Saved {
        /// It.
        save: SavedRun,
    },
    /// Saved runs.
    Saves {
        /// One each, oldest first.
        saves: Vec<SavedRun>,
    },
}

/// A saved run (see `saveRun`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SavedRun {
    /// Id (the file name).
    pub id: String,
    /// The name it was given.
    pub name: String,
    /// The simulated time it was saved at.
    pub at_ms: f64,
    /// Size in bytes.
    pub bytes: f64,
}

/// Where a session is in its life.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum State {
    /// Parsed, not built.
    Loaded,
    /// Built, not running.
    Ready,
    /// Running.
    Running,
    /// Running, clock paused.
    Paused,
    /// Failed; rebuild.
    Failed,
}

/// A session at a glance.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SessionSummary {
    /// Id.
    pub id: u32,
    /// Name.
    pub name: String,
    /// State.
    pub state: State,
    /// Simulation time.
    pub sim_ms: f64,
    /// Simulated seconds per wall second; `null` for as fast as possible.
    pub speed: Option<f64>,
    /// Neurons (0 until built).
    pub neurons: u32,
    /// A replay of a recording: the recording's last event time; `null` for a live session.
    pub replay_end_ms: Option<f64>,
    /// Nodes the session runs on: 1 for this server alone.
    pub nodes: u32,
}

/// A sensor connected to a session.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SensorInfo {
    /// Unique among the session's sensors.
    #[ts(type = "number")]
    pub id: u64,
    /// Its name.
    pub name: String,
    /// The neurons (indices) each channel drives.
    pub channels: Vec<Vec<u32>>,
    /// How samples become input: `current`, `rate`, `threshold` or `spikes`.
    pub encoding: String,
    /// Samples (or events) received so far.
    #[ts(type = "number")]
    pub samples: u64,
    /// Neuron inputs produced so far.
    #[ts(type = "number")]
    pub inputs: u64,
}

/// Where a built network runs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Deployment {
    /// Every node: this engine (`main`) first, then the node agents in the order given.
    pub nodes: Vec<DeployedNode>,
    /// Each neuron's node (an index into `nodes`).
    pub neuron_nodes: Vec<u32>,
    /// Each neuron's shard on its node.
    pub neuron_shards: Vec<u16>,
    /// Populations, sets, and pinned neurons listed alone, in description order.
    pub groups: Vec<DeployedGroup>,
    /// The nodes the description names (`nodes=[(name=…)]`).
    pub declared_nodes: Vec<String>,
    /// What doesn't fit (pins that weren't honoured, named nodes no agent is).
    pub warnings: Vec<String>,
}

/// One node of a deployment.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DeployedNode {
    /// `main` for this engine, a node agent's name (`name=host:port`), or empty.
    pub name: String,
    /// A node agent's `host:port`; empty for this engine.
    pub address: String,
    /// Neurons on each of its shards.
    #[ts(type = "number[]")]
    pub shard_neurons: Vec<u64>,
    /// Synapses it made (onto its own neurons), when built.
    #[ts(type = "number")]
    pub synapses: u64,
}

/// A population, set or pinned neuron, and where its neurons run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DeployedGroup {
    /// Its name.
    pub name: String,
    /// `population`, `set` or `neuron`.
    pub kind: String,
    /// The node it's pinned to (`node=`), if any.
    pub pinned: Option<String>,
    /// Its neurons on each node.
    #[ts(type = "number[]")]
    pub per_node: Vec<u64>,
}

/// A session's cluster: this server (the coordinator, node 0) and its node agents.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Cluster {
    /// Every node, the coordinator first.
    pub nodes: Vec<ClusterNode>,
    /// Windows agreed with every node so far.
    #[ts(type = "number")]
    pub rounds: u64,
    /// Wall time the coordinator spent waiting for the other nodes (their reports and spikes in flight), in ms.
    pub waited_ms: f64,
    /// Spikes and other events received from the other nodes.
    #[ts(type = "number")]
    pub received: u64,
    /// Events that arrived too late for their time and were delivered late (always 0 in lockstep).
    #[ts(type = "number")]
    pub late: u64,
    /// The current (or last) run kept every node in lockstep.
    pub lockstep: bool,
    /// How timely events are on each link between nodes.
    pub links: Vec<ClusterLink>,
    /// Why the cluster failed, if it did.
    pub failure: Option<String>,
}

/// Events from one node to another.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ClusterLink {
    /// Sending node (0 is the coordinator).
    pub from: u32,
    /// Receiving node.
    pub to: u32,
    /// Events received.
    #[ts(type = "number")]
    pub events: u64,
    /// Of those, events that arrived after their time and were delivered late.
    #[ts(type = "number")]
    pub late: u64,
    /// The largest lateness, in ms of simulation time.
    pub max_late_ms: f64,
    /// The 99th percentile of lateness over late events, in ms.
    pub p99_late_ms: f64,
}

/// One node of a cluster.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ClusterNode {
    /// `host:port` of its agent; "coordinator" for this server.
    pub address: String,
    /// Events the coordinator sent it.
    #[ts(type = "number")]
    pub events_sent: u64,
    /// Bytes the coordinator sent it.
    #[ts(type = "number")]
    pub bytes_sent: u64,
}

/// A built network.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Network {
    /// Neurons by index.
    pub neurons: Vec<Neuron>,
    /// Synapses, grouped by receiving neuron: all of them for networks of up to 500,000 synapses, none for bigger
    /// ones (`getNeuron` gives a neuron's own).
    pub synapses: Vec<Synapse>,
    /// How many synapses there are.
    #[ts(type = "number")]
    pub synapse_count: u64,
    /// Modulator names by index.
    pub modulators: Vec<String>,
    /// Learning rules new connections can use.
    pub learning_rules: Vec<String>,
}

/// One neuron.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Neuron {
    /// Index.
    pub index: u32,
    /// Name.
    pub name: String,
    /// Its population (empty for a neuron listed on its own).
    pub population: String,
    /// Dynamics class.
    pub kind: String,
    /// Position in µm.
    pub position: [f64; 3],
    /// Outgoing connections default to inhibitory.
    pub inhibitory: bool,
    /// Spatial cell (for binned frames).
    pub cell: u32,
}

/// One synapse.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Synapse {
    /// Receiving neuron.
    pub post: u32,
    /// Index on the receiving neuron.
    pub synapse: u32,
    /// Sending neuron.
    pub pre: u32,
    /// Initial weight.
    pub weight: f32,
    /// Inhibitory.
    pub inhibitory: bool,
}

/// A neuron's state as of its last update.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NeuronState {
    /// Membrane potential, mV.
    pub membrane_mv: f64,
    /// Intrinsic-plasticity offset, mV.
    pub intrinsic_mv: f64,
    /// Firing-rate estimate, Hz.
    pub rate_hz: f64,
    /// Time of the last update.
    pub last_event_ms: f64,
    /// Time of the last spike.
    pub last_spike_ms: Option<f64>,
    /// The session's simulated time when this was queried (e.g. for how long ago the last spike was).
    pub now_ms: f64,
    /// Firing tonically.
    pub tonic: bool,
    /// Spikes so far.
    pub spikes: u32,
    /// Incoming synapses.
    pub synapses: Vec<SynapseState>,
}

/// An incoming synapse's state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SynapseState {
    /// Sending neuron.
    pub pre: u32,
    /// Weight.
    pub weight: f32,
    /// Three-factor eligibility.
    pub eligibility: f32,
}

/// A recorded run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Recording {
    /// Id (the directory name).
    pub id: String,
    /// Where it is on the engine's machine.
    pub path: String,
    /// Its tables' files.
    pub files: Vec<RecordingFile>,
}

/// One file of a recording.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RecordingFile {
    /// File name, e.g. `spikes.parquet`.
    pub name: String,
    /// Size in bytes.
    pub bytes: f64,
}

/// Things that happen to a session.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
#[ts(export)]
pub enum Notice {
    /// The state changed.
    StateChanged {
        /// The new state.
        state: State,
    },
    /// A run ended.
    RunEnded {
        /// Simulation time at the end.
        sim_ms: f64,
        /// The recording's id, if recorded.
        recording: Option<String>,
    },
    /// Recording failed (the run went on).
    RecordingFailed {
        /// Why.
        message: String,
    },
    /// Connections were added or pruned: fetch the network again.
    NetworkChanged,
    /// A sensor connected or disconnected: list them again (`listSensors`).
    SensorsChanged,
    /// The run thread failed.
    Failed {
        /// Why.
        message: String,
    },
}
