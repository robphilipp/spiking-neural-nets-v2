//! Sessions over several nodes (hosts or processes).
//!
//! A *node agent* ([`serve_node`]) runs on every host that lends its cores. A session given node addresses
//! ([`crate::Session::set_nodes`]) becomes the cluster's coordinator when it builds: it asks each agent to build the
//! same network as one node of the cluster (each keeps only its share of the neurons; see
//! `tempo_runtime::cluster`), and the nodes connect to each other directly for spikes (`tempo_cluster`). The
//! coordinator keeps everything else: the clock (the other nodes follow its windows), viewers, recordings, sensors
//! and the API. Every node's emitted events are forwarded to it, so live views and recordings see the whole network.
//! Requests about one neuron (query, observe, connections) go to the node that has it.
//!
//! The control protocol between coordinator and agents is a request/response exchange of length-prefixed
//! `postcard` frames over TCP, one connection per coordinator session.

use std::io::{self, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use spikes_network::{Event, SimMsg};
use tempo_cluster::{StrictLink, TcpTransport, field_codec};
use tempo_runtime::{EventPublisher, EventStream, Jitter, SimDuration, SimTime};

use crate::session::{BuildSettings, InputSpec, NeuronSnapshot};
use crate::{SessionError, SessionId};

/// How long nodes have to connect to each other (building a large network on a slow node counts too).
const MESH_TIMEOUT: Duration = Duration::from_secs(120);
/// How long to wait for a node agent to answer a request.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(150);
/// How long a run's end waits for the other nodes to report theirs.
const RUN_END_TIMEOUT: Duration = Duration::from_secs(30);

// ----- the control protocol -----

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
enum WireJitter {
    None,
    Uniform(u64),
    Normal(u64),
}

impl From<Jitter> for WireJitter {
    fn from(j: Jitter) -> Self {
        match j {
            Jitter::None => Self::None,
            Jitter::Uniform { half_width } => Self::Uniform(half_width.as_nanos()),
            Jitter::Normal { sigma } => Self::Normal(sigma.as_nanos()),
        }
    }
}

impl From<WireJitter> for Jitter {
    fn from(j: WireJitter) -> Self {
        match j {
            WireJitter::None => Self::None,
            WireJitter::Uniform(ns) => Self::Uniform {
                half_width: SimDuration::from_nanos(ns),
            },
            WireJitter::Normal(ns) => Self::Normal {
                sigma: SimDuration::from_nanos(ns),
            },
        }
    }
}

/// A route change, by the sending neuron's index (its route).
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub(crate) enum RouteChange {
    /// Add an edge to `post`, with this delay and receiver index.
    Connect { post: u32, delay_ns: u64, edge: u32 },
    /// Remove the edge to `post` with this receiver index.
    Prune { post: u32, edge: u32 },
}

#[derive(Serialize, Deserialize, Debug)]
enum Request {
    /// Open a listener for the node mesh; reply with its address.
    Prepare,
    /// Build the network as node `node` and connect to the other nodes.
    Build {
        cluster: u64,
        node: u32,
        mesh: Vec<SocketAddr>,
        shards: u32,
        text: String,
        seed: u64,
        min_delay_ns: u64,
        jitter: WireJitter,
        record_weights: bool,
    },
    /// Add a synthetic input (every node spawns it, so the directories stay identical).
    AddInput(InputSpec),
    /// Run until `end_ns`, following the coordinator's windows.
    Run {
        end_ns: u64,
    },
    Observe {
        neurons: Vec<u32>,
        on: bool,
    },
    Query {
        neuron: u32,
    },
    AddSynapse {
        pre: u32,
        post: u32,
        weight: f32,
        inhibitory: Option<bool>,
        learning: Option<String>,
        last_ns: u64,
    },
    PruneSynapse {
        pre: u32,
        post: u32,
    },
    ChangeRoute {
        pre: u32,
        change: RouteChange,
    },
    /// The neuron now sends along its route (it gained its first connection).
    SetOut {
        pre: u32,
    },
    /// The node's neurons' synapses, as they are now.
    Synapses,
    /// Done: drop the network.
    Close,
}

#[derive(Serialize, Deserialize, Debug)]
enum Response {
    Ok,
    Prepared(SocketAddr),
    Snapshot(NeuronSnapshot),
    Index(Option<u32>),
    Synapses(Vec<spikes_record::SynapseRow>),
    Err(String),
}

fn write_frame(stream: &mut TcpStream, bytes: &[u8]) -> io::Result<()> {
    stream.write_all(&(bytes.len() as u32).to_le_bytes())?;
    stream.write_all(bytes)
}

fn read_frame(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut len = [0u8; 4];
    stream.read_exact(&mut len)?;
    let mut buf = vec![0u8; u32::from_le_bytes(len) as usize];
    stream.read_exact(&mut buf)?;
    Ok(buf)
}

/// What nodes send the coordinator over the mesh, after (and in order with) their spikes.
#[derive(Serialize, Deserialize, Debug)]
enum AppMsg {
    /// Emitted events (`tempo_cluster::encode_events`).
    Events(Vec<u8>),
    /// The node's run ended at this time; every event it emitted has been sent before this.
    RunEnded { now_ns: u64 },
    /// A system event (a warning or notice about the runtime) at this simulation time.
    System {
        at_ns: u64,
        event: tempo_runtime::SystemEvent,
    },
}

// ----- the coordinator's side -----

/// A connection to one node agent.
struct NodeClient {
    address: String,
    stream: Mutex<TcpStream>,
}

impl NodeClient {
    fn connect(address: &str) -> Result<Self, SessionError> {
        let unreachable = |e: &dyn std::fmt::Display| SessionError::Invalid(format!("node {address}: {e}"));
        let addr = address
            .to_socket_addrs()
            .map_err(|e| unreachable(&e))?
            .next()
            .ok_or_else(|| unreachable(&"no address"))?;
        let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(5)).map_err(|e| unreachable(&e))?;
        stream.set_nodelay(true).map_err(|e| unreachable(&e))?;
        stream
            .set_read_timeout(Some(REQUEST_TIMEOUT))
            .map_err(|e| unreachable(&e))?;
        Ok(Self {
            address: address.to_string(),
            stream: Mutex::new(stream),
        })
    }

    fn local_ip(&self) -> Option<IpAddr> {
        lock(&self.stream).local_addr().ok().map(|a| a.ip())
    }

    fn call(&self, request: &Request) -> Result<Response, SessionError> {
        let failed = |e: &dyn std::fmt::Display| SessionError::Invalid(format!("node {}: {e}", self.address));
        let mut stream = lock(&self.stream);
        let bytes = postcard::to_stdvec(request).map_err(|e| failed(&e))?;
        write_frame(&mut stream, &bytes).map_err(|e| failed(&e))?;
        let reply = read_frame(&mut stream).map_err(|e| failed(&e))?;
        match postcard::from_bytes(&reply).map_err(|e| failed(&e))? {
            Response::Err(message) => Err(failed(&message)),
            response => Ok(response),
        }
    }

    fn ok(&self, request: &Request) -> Result<(), SessionError> {
        self.call(request).map(|_| ())
    }
}

/// A coordinator's view of its cluster.
pub(crate) struct Coordinator {
    clients: Vec<NodeClient>,
    pub(crate) link: StrictLink<SimMsg>,
    /// The node of each neuron (by index).
    owners: Mutex<Vec<u32>>,
    /// Each node's last reported run end (index = node; the coordinator's own entry is unused).
    ended: Mutex<Vec<Option<SimTime>>>,
    ended_changed: Condvar,
    closed: AtomicBool,
}

impl std::fmt::Debug for Coordinator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Coordinator")
            .field("nodes", &self.clients.iter().map(|c| &c.address).collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// Connects a node's link to the rest of the cluster (blocks until every node has connected).
pub(crate) type MeshConnect = Box<dyn FnOnce(&StrictLink<SimMsg>) -> Result<(), SessionError> + Send>;

/// A synapse to add on another node (see [`Coordinator::add_synapse`]).
pub(crate) struct NewSynapse<'a> {
    pub(crate) pre: u32,
    pub(crate) post: u32,
    pub(crate) weight: f32,
    pub(crate) inhibitory: Option<bool>,
    pub(crate) learning: Option<&'a str>,
    pub(crate) last: SimTime,
}

/// What a session needs to build as a node of a cluster.
pub(crate) struct NodeSetup {
    pub(crate) placement: tempo_runtime::NodePlacement,
    pub(crate) link: StrictLink<SimMsg>,
    /// Connects the mesh once the runtime is built (blocks until every node has connected).
    pub(crate) connect: MeshConnect,
}

impl Coordinator {
    /// Connects to the node agents and asks them to build (in the background; [`NodeSetup::connect`] waits for
    /// them). The coordinator is node 0.
    pub(crate) fn start(
        nodes: &[String],
        text: &str,
        settings: &BuildSettings,
    ) -> Result<(Arc<Self>, NodeSetup), SessionError> {
        let clients = nodes
            .iter()
            .map(|a| NodeClient::connect(a))
            .collect::<Result<Vec<_>, _>>()?;
        // The mesh listens on the address the node agents reach us from.
        let ip = clients
            .first()
            .and_then(NodeClient::local_ip)
            .unwrap_or(IpAddr::from([127, 0, 0, 1]));
        let listener = TcpListener::bind((ip, 0)).map_err(|e| SessionError::Invalid(format!("mesh listener: {e}")))?;
        let mut mesh = vec![
            listener
                .local_addr()
                .map_err(|e| SessionError::Invalid(e.to_string()))?,
        ];
        for client in &clients {
            match client.call(&Request::Prepare)? {
                Response::Prepared(addr) => mesh.push(addr),
                other => return Err(SessionError::Invalid(format!("node {}: {other:?}", client.address))),
            }
        }
        let n = mesh.len() as u32;
        let shards = settings.shards;
        let cluster: u64 = rand::random();
        let link = StrictLink::new(0, n, shards, vec![field_codec::<spikes_network::ModulatorField>()]);
        let coordinator = Arc::new(Self {
            clients,
            link: link.clone(),
            owners: Mutex::new(Vec::new()),
            ended: Mutex::new(vec![None; n as usize]),
            ended_changed: Condvar::new(),
            closed: AtomicBool::new(false),
        });

        // Ask every node to build; their replies come once they've connected to the mesh.
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        for (i, _) in coordinator.clients.iter().enumerate() {
            let coordinator = Arc::clone(&coordinator);
            let request = Request::Build {
                cluster,
                node: i as u32 + 1,
                mesh: mesh.clone(),
                shards: shards as u32,
                text: text.to_string(),
                seed: settings.seed,
                min_delay_ns: settings.min_delay.as_nanos(),
                jitter: settings.jitter.into(),
                record_weights: settings.record_weights,
            };
            let done_tx = done_tx.clone();
            std::thread::spawn(move || {
                let _ = done_tx.send(coordinator.clients[i].ok(&request));
            });
        }
        drop(done_tx);
        let mesh_for_connect = mesh.clone();
        let setup = NodeSetup {
            placement: tempo_runtime::NodePlacement {
                node: 0,
                shards_per_node: vec![shards; n as usize],
            },
            link,
            connect: Box::new(move |link| {
                let transport =
                    TcpTransport::connect(0, &listener, &mesh_for_connect, cluster, link.sink(), MESH_TIMEOUT);
                // A node that failed to build never connects: report its error rather than the timeout.
                let mut first_error = None;
                if transport.is_err() {
                    for result in done_rx.try_iter() {
                        if let Err(e) = result {
                            first_error.get_or_insert(e);
                        }
                    }
                }
                let transport = match (transport, first_error) {
                    (_, Some(e)) => return Err(e),
                    (Err(e), None) => return Err(SessionError::Invalid(format!("connecting the nodes: {e}"))),
                    (Ok(t), None) => t,
                };
                link.attach(Arc::new(transport));
                for result in done_rx.iter() {
                    result?;
                }
                Ok(())
            }),
        };
        Ok((coordinator, setup))
    }

    /// Takes in the other nodes' events and run ends. Call once the runtime exists.
    pub(crate) fn listen(self: &Arc<Self>, publisher: EventPublisher<Event>, system: tempo_runtime::SystemPublisher) {
        let weak = Arc::downgrade(self);
        self.link.on_app(Arc::new(move |from, bytes| {
            let Some(me) = weak.upgrade() else { return };
            match postcard::from_bytes::<AppMsg>(bytes) {
                Ok(AppMsg::Events(events)) => {
                    if let Some(batch) = tempo_cluster::decode_events::<Event>(&events) {
                        publisher.publish(batch);
                    }
                }
                Ok(AppMsg::System { at_ns, event }) => system.publish(SimTime::from_nanos(at_ns), event),
                Ok(AppMsg::RunEnded { now_ns }) => {
                    if let Some(slot) = lock(&me.ended).get_mut(from as usize) {
                        *slot = Some(SimTime::from_nanos(now_ns));
                    }
                    me.ended_changed.notify_all();
                }
                Err(_) => {}
            }
        }));
    }

    /// Records which node has each neuron.
    pub(crate) fn set_owners(&self, owners: Vec<u32>) {
        *lock(&self.owners) = owners;
    }

    /// The node agent for a neuron, if it lives on another node.
    pub(crate) fn owner(&self, neuron: u32) -> Option<usize> {
        match lock(&self.owners).get(neuron as usize) {
            Some(&node) if node > 0 => Some(node as usize - 1),
            _ => None,
        }
    }

    fn each(&self, request: &Request) -> Result<(), SessionError> {
        self.clients.iter().try_for_each(|c| c.ok(request))
    }

    /// Every node runs until `end` (they wait for the coordinator's windows).
    pub(crate) fn run(&self, end: SimTime) -> Result<(), SessionError> {
        lock(&self.ended).iter_mut().for_each(|e| *e = None);
        self.each(&Request::Run { end_ns: end.as_nanos() })
    }

    /// Waits until every other node has reported its run's end (and so sent all its events). Returns the first
    /// node whose time differs from `at`, or a failure.
    pub(crate) fn wait_for_run_end(&self, at: SimTime) -> Result<(), SessionError> {
        let deadline = Instant::now() + RUN_END_TIMEOUT;
        let mut ended = lock(&self.ended);
        loop {
            if let Some(failure) = self.link.failure() {
                return Err(SessionError::Invalid(format!("the cluster failed: {failure}")));
            }
            if ended.iter().skip(1).all(Option::is_some) {
                return match ended.iter().enumerate().skip(1).find(|(_, t)| **t != Some(at)) {
                    Some((node, t)) => Err(SessionError::Invalid(format!(
                        "node {node} ended its run at {t:?}, the coordinator at {at}"
                    ))),
                    None => Ok(()),
                };
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(SessionError::Invalid("the other nodes didn't finish the run".into()));
            }
            ended = self
                .ended_changed
                .wait_timeout(ended, (deadline - now).min(Duration::from_millis(50)))
                .unwrap_or_else(|p| p.into_inner())
                .0;
        }
    }

    pub(crate) fn add_input(&self, input: &InputSpec) -> Result<(), SessionError> {
        self.each(&Request::AddInput(input.clone()))
    }

    pub(crate) fn observe(&self, node: usize, neurons: Vec<u32>, on: bool) -> Result<(), SessionError> {
        self.clients[node].ok(&Request::Observe { neurons, on })
    }

    pub(crate) fn query(&self, node: usize, neuron: u32) -> Result<NeuronSnapshot, SessionError> {
        match self.clients[node].call(&Request::Query { neuron })? {
            Response::Snapshot(s) => Ok(s),
            other => Err(SessionError::Invalid(format!("unexpected reply {other:?}"))),
        }
    }

    /// Adds a synapse on node agent `node` (which has its receiving neuron); returns its index there.
    pub(crate) fn add_synapse(&self, node: usize, s: NewSynapse<'_>) -> Result<u32, SessionError> {
        match self.clients[node].call(&Request::AddSynapse {
            pre: s.pre,
            post: s.post,
            weight: s.weight,
            inhibitory: s.inhibitory,
            learning: s.learning.map(str::to_string),
            last_ns: s.last.as_nanos(),
        })? {
            Response::Index(Some(i)) => Ok(i),
            other => Err(SessionError::Invalid(format!("unexpected reply {other:?}"))),
        }
    }

    pub(crate) fn prune_synapse(&self, node: usize, pre: u32, post: u32) -> Result<Option<u32>, SessionError> {
        match self.clients[node].call(&Request::PruneSynapse { pre, post })? {
            Response::Index(i) => Ok(i),
            other => Err(SessionError::Invalid(format!("unexpected reply {other:?}"))),
        }
    }

    pub(crate) fn change_route(&self, node: usize, pre: u32, change: RouteChange) -> Result<(), SessionError> {
        self.clients[node].ok(&Request::ChangeRoute { pre, change })
    }

    pub(crate) fn set_out(&self, node: usize, pre: u32) -> Result<(), SessionError> {
        self.clients[node].ok(&Request::SetOut { pre })
    }

    /// Every other node's synapses, as they are now.
    pub(crate) fn synapses(&self) -> Result<Vec<spikes_record::SynapseRow>, SessionError> {
        let mut all = Vec::new();
        for client in &self.clients {
            match client.call(&Request::Synapses)? {
                Response::Synapses(rows) => all.extend(rows),
                other => return Err(SessionError::Invalid(format!("unexpected reply {other:?}"))),
            }
        }
        Ok(all)
    }

    /// The node agents' addresses.
    pub(crate) fn addresses(&self) -> Vec<String> {
        self.clients.iter().map(|c| c.address.clone()).collect()
    }

    /// Ends the cluster: the nodes drop their networks.
    pub(crate) fn close(&self) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        self.link.close();
        for client in &self.clients {
            let _ = client.ok(&Request::Close);
        }
    }
}

impl Drop for Coordinator {
    fn drop(&mut self) {
        self.close();
    }
}

// ----- a node's side -----

/// Forwards a node's emitted events to the coordinator, in order with its run-end reports. Its thread owns the
/// event stream and is the only sender of either, so a run's end always follows the run's events.
pub(crate) struct Forwarder {
    ends: Mutex<std::sync::mpsc::Sender<SimTime>>,
    link: StrictLink<SimMsg>,
    stop: Arc<AtomicBool>,
}

impl Forwarder {
    pub(crate) fn start(
        stream: EventStream<Event>,
        system: tempo_runtime::SystemEvents,
        link: StrictLink<SimMsg>,
    ) -> Arc<Self> {
        let (ends_tx, ends) = std::sync::mpsc::channel::<SimTime>();
        let stop = Arc::new(AtomicBool::new(false));
        let (l, s) = (link.clone(), Arc::clone(&stop));
        std::thread::Builder::new()
            .name("spikes-forward".into())
            .spawn(move || {
                while !s.load(Ordering::Acquire) {
                    match stream.recv_timeout(Duration::from_millis(2)) {
                        Ok(Some(batch)) => send_events(&l, &batch),
                        Ok(None) => {}
                        Err(_) => break,
                    }
                    send_system(&l, &system);
                    while let Ok(now) = ends.try_recv() {
                        // Everything the run emitted is in the stream by now: send it, then the end.
                        send_events(&l, &stream.drain());
                        send_system(&l, &system);
                        let msg = AppMsg::RunEnded { now_ns: now.as_nanos() };
                        l.send_app(0, postcard::to_stdvec(&msg).expect("encodes"));
                    }
                }
            })
            .expect("forwarder thread starts");
        Arc::new(Self {
            ends: Mutex::new(ends_tx),
            link,
            stop,
        })
    }

    /// Reports the run's end (after every event it emitted).
    pub(crate) fn run_ended(&self, now: SimTime) {
        let _ = lock(&self.ends).send(now);
    }

    pub(crate) fn stop(&self) {
        self.stop.store(true, Ordering::Release);
    }

    pub(crate) fn link(&self) -> &StrictLink<SimMsg> {
        &self.link
    }
}

fn send_system(link: &StrictLink<SimMsg>, system: &tempo_runtime::SystemEvents) {
    while let Some(notice) = system.try_next() {
        let msg = AppMsg::System {
            at_ns: notice.sim_time.as_nanos(),
            event: notice.event,
        };
        link.send_app(0, postcard::to_stdvec(&msg).expect("encodes"));
    }
}

fn send_events(link: &StrictLink<SimMsg>, batch: &[tempo_runtime::Emitted<Event>]) {
    if batch.is_empty() {
        return;
    }
    let msg = AppMsg::Events(tempo_cluster::encode_events(batch));
    link.send_app(0, postcard::to_stdvec(&msg).expect("encodes"));
}

/// Serves as a node agent: each connection is one coordinator session, for which this process builds and runs its
/// share of the network. Runs until the listener fails.
pub fn serve_node(listener: TcpListener) -> io::Result<()> {
    let mut next = 0u32;
    loop {
        let (stream, peer) = listener.accept()?;
        next += 1;
        let id = SessionId(u32::MAX - next);
        std::thread::Builder::new()
            .name(format!("node-{next}"))
            .spawn(move || {
                if let Err(e) = serve_coordinator(stream, id) {
                    eprintln!("spikes node: coordinator {peer}: {e}");
                }
            })?;
    }
}

/// The node's session for one coordinator.
fn serve_coordinator(mut stream: TcpStream, id: SessionId) -> io::Result<()> {
    stream.set_nodelay(true)?;
    let mut mesh: Option<TcpListener> = None;
    let mut session: Option<crate::Session> = None;
    let result = loop {
        let bytes = match read_frame(&mut stream) {
            Ok(b) => b,
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break Ok(()),
            Err(e) => break Err(e),
        };
        let request: Request = match postcard::from_bytes(&bytes) {
            Ok(r) => r,
            Err(e) => break Err(io::Error::new(io::ErrorKind::InvalidData, e)),
        };
        let close = matches!(request, Request::Close);
        let response =
            handle(request, &stream, &mut mesh, &mut session, id).unwrap_or_else(|e| Response::Err(e.to_string()));
        write_frame(&mut stream, &postcard::to_stdvec(&response).expect("encodes"))?;
        if close {
            break Ok(());
        }
    };
    if let Some(session) = session {
        session.leave_cluster();
    }
    result
}

fn handle(
    request: Request,
    control: &TcpStream,
    mesh: &mut Option<TcpListener>,
    session: &mut Option<crate::Session>,
    id: SessionId,
) -> Result<Response, SessionError> {
    let built = |s: &Option<crate::Session>| {
        s.clone()
            .ok_or_else(|| SessionError::Invalid("this node has no network yet".into()))
    };
    Ok(match request {
        Request::Prepare => {
            let ip = control
                .local_addr()
                .map_err(|e| SessionError::Invalid(e.to_string()))?
                .ip();
            let listener = TcpListener::bind((ip, 0)).map_err(|e| SessionError::Invalid(e.to_string()))?;
            let addr = listener
                .local_addr()
                .map_err(|e| SessionError::Invalid(e.to_string()))?;
            *mesh = Some(listener);
            Response::Prepared(addr)
        }
        Request::Build {
            cluster,
            node,
            mesh: addresses,
            shards,
            text,
            seed,
            min_delay_ns,
            jitter,
            record_weights,
        } => {
            if let Some(old) = session.take() {
                old.leave_cluster();
            }
            let listener = mesh
                .take()
                .ok_or_else(|| SessionError::Invalid("build before prepare".into()))?;
            let s = crate::Session::new(id, format!("node {node}"), text)?;
            let shards = shards as usize;
            let link = StrictLink::new(
                node,
                addresses.len() as u32,
                shards,
                vec![field_codec::<spikes_network::ModulatorField>()],
            );
            let setup = NodeSetup {
                placement: tempo_runtime::NodePlacement {
                    node,
                    shards_per_node: vec![shards; addresses.len()],
                },
                link,
                connect: Box::new(move |link| {
                    let transport =
                        TcpTransport::connect(node, &listener, &addresses, cluster, link.sink(), MESH_TIMEOUT)
                            .map_err(|e| SessionError::Invalid(format!("connecting the nodes: {e}")))?;
                    link.attach(Arc::new(transport));
                    Ok(())
                }),
            };
            let settings = BuildSettings {
                seed,
                shards,
                min_delay: SimDuration::from_nanos(min_delay_ns),
                jitter: jitter.into(),
                record_weights,
            };
            s.build_member(settings, setup)?;
            *session = Some(s);
            Response::Ok
        }
        Request::AddInput(input) => {
            built(session)?.add_input(input)?;
            Response::Ok
        }
        Request::Run { end_ns } => {
            built(session)?.run_member(SimTime::from_nanos(end_ns))?;
            Response::Ok
        }
        Request::Observe { neurons, on } => {
            built(session)?.observe(&neurons, on)?;
            Response::Ok
        }
        Request::Query { neuron } => Response::Snapshot(built(session)?.query_neuron(neuron)?),
        Request::AddSynapse {
            pre,
            post,
            weight,
            inhibitory,
            learning,
            last_ns,
        } => Response::Index(Some(built(session)?.add_synapse_here(
            pre,
            post,
            weight,
            inhibitory,
            learning.as_deref(),
            SimTime::from_nanos(last_ns),
        )?)),
        Request::PruneSynapse { pre, post } => Response::Index(built(session)?.prune_synapse_here(pre, post)?),
        Request::ChangeRoute { pre, change } => {
            built(session)?.change_route_here(pre, change)?;
            Response::Ok
        }
        Request::SetOut { pre } => {
            built(session)?.set_out_here(pre)?;
            Response::Ok
        }
        Request::Synapses => Response::Synapses(built(session)?.synapses_here()?),
        Request::Close => {
            if let Some(s) = session.take() {
                s.leave_cluster();
            }
            Response::Ok
        }
    })
}
