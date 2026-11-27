//! Moving frames between nodes: in memory (tests, several nodes in one process) or over TCP (one connection per
//! pair of nodes, Nagle off, length-prefixed frames, a writer thread that batches whatever is queued into one write).

use std::io::{self, BufReader, BufWriter, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Sends frames to other nodes. Never blocks for long: frames are queued.
pub trait Transport: Send + Sync {
    /// Queues `frame` for node `to`.
    fn send(&self, to: u32, frame: Vec<u8>);
}

/// Receives what a transport delivers.
pub trait FrameSink: Send + Sync {
    /// A frame from node `from`.
    fn on_frame(&self, from: u32, frame: &[u8]);
    /// The connection to `peer` failed or closed.
    fn on_failure(&self, peer: u32, error: String);
}

/// Several nodes in one process, connected in memory: a frame is handed to its receiver straight away, on the
/// sender's thread. A frame for a node that hasn't joined yet (its thread is still starting) is kept, and handed
/// over, in order, when it joins.
#[derive(Default)]
pub struct MemoryNetwork {
    nodes: Vec<Mutex<MemoryNode>>,
}

/// A node of a [`MemoryNetwork`]: its sink once joined, and the frames that came before.
#[derive(Default)]
struct MemoryNode {
    sink: Option<Arc<dyn FrameSink>>,
    /// Frames sent before it joined: (from, frame), in order.
    early: Vec<(u32, Vec<u8>)>,
}

impl std::fmt::Debug for MemoryNetwork {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryNetwork")
            .field("nodes", &self.nodes.len())
            .finish()
    }
}

impl MemoryNetwork {
    /// A network of `nodes` nodes.
    pub fn new(nodes: u32) -> Arc<Self> {
        Arc::new(Self {
            nodes: (0..nodes).map(|_| Mutex::default()).collect(),
        })
    }

    /// Node `node`'s transport; what reaches the node goes to `sink` (first, anything sent to it before it joined).
    pub fn transport(self: &Arc<Self>, node: u32, sink: Arc<dyn FrameSink>) -> Arc<dyn Transport> {
        let mut slot = self.nodes[node as usize].lock().unwrap_or_else(|p| p.into_inner());
        assert!(slot.sink.is_none(), "node {node} joined twice");
        // Under the lock, so a frame sent meanwhile waits and comes after these.
        for (from, frame) in std::mem::take(&mut slot.early) {
            sink.on_frame(from, &frame);
        }
        slot.sink = Some(sink);
        drop(slot);
        Arc::new(MemoryTransport {
            network: Arc::clone(self),
            node,
        })
    }
}

struct MemoryTransport {
    network: Arc<MemoryNetwork>,
    node: u32,
}

impl Transport for MemoryTransport {
    fn send(&self, to: u32, frame: Vec<u8>) {
        let mut slot = self.network.nodes[to as usize]
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        match slot.sink.clone() {
            Some(sink) => {
                drop(slot);
                sink.on_frame(self.node, &frame);
            }
            // Not joined yet: kept for when it does.
            None => slot.early.push((self.node, frame)),
        }
    }
}

/// A full mesh of TCP connections between the nodes of a cluster.
pub struct TcpTransport {
    peers: Vec<Option<Sender<Vec<u8>>>>,
    streams: Mutex<Vec<TcpStream>>,
}

impl std::fmt::Debug for TcpTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TcpTransport")
            .field("peers", &self.peers.len())
            .finish()
    }
}

/// The first bytes on every connection: who's calling, and for which cluster run.
const MAGIC: &[u8; 4] = b"TMPO";

impl TcpTransport {
    /// Connects node `node` to every other node: it calls the nodes before it (at `addresses`) and accepts calls
    /// from the nodes after it on `listener`. `cluster` identifies this cluster (every node passes the same value),
    /// so a stray connection from another one is refused. Gives up after `timeout`.
    pub fn connect(
        node: u32,
        listener: &TcpListener,
        addresses: &[SocketAddr],
        cluster: u64,
        sink: Arc<dyn FrameSink>,
        timeout: Duration,
    ) -> io::Result<Self> {
        let nodes = addresses.len() as u32;
        let deadline = Instant::now() + timeout;
        let mut streams: Vec<Option<TcpStream>> = (0..nodes).map(|_| None).collect();

        for peer in 0..node {
            let mut stream = loop {
                match TcpStream::connect_timeout(&addresses[peer as usize], Duration::from_secs(1)) {
                    Ok(s) => break s,
                    Err(e) if Instant::now() >= deadline => {
                        return Err(io::Error::new(
                            e.kind(),
                            format!("node {peer} at {}: {e}", addresses[peer as usize]),
                        ));
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(50)),
                }
            };
            stream.write_all(MAGIC)?;
            stream.write_all(&cluster.to_le_bytes())?;
            stream.write_all(&node.to_le_bytes())?;
            streams[peer as usize] = Some(stream);
        }

        listener.set_nonblocking(true)?;
        let mut waiting = nodes - node - 1;
        while waiting > 0 {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    stream.set_nonblocking(false)?;
                    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
                    let mut hello = [0u8; 16];
                    if stream.read_exact(&mut hello).is_err() || &hello[..4] != MAGIC {
                        continue;
                    }
                    let their_cluster = u64::from_le_bytes(hello[4..12].try_into().expect("8 bytes"));
                    let peer = u32::from_le_bytes(hello[12..16].try_into().expect("4 bytes"));
                    if their_cluster != cluster || peer <= node || peer >= nodes || streams[peer as usize].is_some() {
                        continue;
                    }
                    stream.set_read_timeout(None)?;
                    streams[peer as usize] = Some(stream);
                    waiting -= 1;
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            format!("{waiting} node(s) after node {node} didn't connect in time"),
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(e) => return Err(e),
            }
        }
        listener.set_nonblocking(false)?;

        let mut peers = Vec::with_capacity(nodes as usize);
        let mut kept = Vec::new();
        for (peer, stream) in streams.into_iter().enumerate() {
            let Some(stream) = stream else {
                peers.push(None);
                continue;
            };
            let peer = peer as u32;
            stream.set_nodelay(true)?;
            kept.push(stream.try_clone()?);
            let (tx, rx) = channel();
            let writer = stream.try_clone()?;
            let sink_w = Arc::clone(&sink);
            std::thread::Builder::new()
                .name(format!("tempo-tx-{peer}"))
                .spawn(move || {
                    if let Err(e) = write_loop(writer, &rx) {
                        sink_w.on_failure(peer, format!("sending to node {peer}: {e}"));
                    }
                })?;
            let sink_r = Arc::clone(&sink);
            std::thread::Builder::new()
                .name(format!("tempo-rx-{peer}"))
                .spawn(move || {
                    let error = read_loop(stream, peer, sink_r.as_ref());
                    sink_r.on_failure(peer, error);
                })?;
            peers.push(Some(tx));
        }
        Ok(Self {
            peers,
            streams: Mutex::new(kept),
        })
    }
}

fn write_loop(stream: TcpStream, rx: &Receiver<Vec<u8>>) -> io::Result<()> {
    let mut out = BufWriter::with_capacity(1 << 16, stream);
    while let Ok(frame) = rx.recv() {
        write_frame(&mut out, &frame)?;
        // Whatever else is queued goes in the same write.
        while let Ok(frame) = rx.try_recv() {
            write_frame(&mut out, &frame)?;
        }
        out.flush()?;
    }
    Ok(())
}

fn write_frame(out: &mut impl Write, frame: &[u8]) -> io::Result<()> {
    let len = u32::try_from(frame.len()).map_err(|_| io::Error::other("frame over 4 GB"))?;
    out.write_all(&len.to_le_bytes())?;
    out.write_all(frame)
}

fn read_loop(stream: TcpStream, peer: u32, sink: &dyn FrameSink) -> String {
    let mut input = BufReader::with_capacity(1 << 16, stream);
    let mut buf = Vec::new();
    loop {
        let mut len = [0u8; 4];
        if let Err(e) = input.read_exact(&mut len) {
            return format!("connection to node {peer} closed: {e}");
        }
        buf.resize(u32::from_le_bytes(len) as usize, 0);
        if let Err(e) = input.read_exact(&mut buf) {
            return format!("connection to node {peer} broke: {e}");
        }
        sink.on_frame(peer, &buf);
    }
}

impl Transport for TcpTransport {
    fn send(&self, to: u32, frame: Vec<u8>) {
        if let Some(Some(tx)) = self.peers.get(to as usize) {
            // A closed peer is reported by its reader; nothing more to do here.
            let _ = tx.send(frame);
        }
    }
}

impl Drop for TcpTransport {
    fn drop(&mut self) {
        // Closing the sockets ends the reader threads; dropping the senders ends the writers.
        for stream in self.streams.lock().unwrap_or_else(|p| p.into_inner()).drain(..) {
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
    }
}
