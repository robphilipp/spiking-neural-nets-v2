//! Live frames for viewers (app plan 01 §3).
//!
//! Each session has a [`Hub`] and one aggregator thread. The thread reads the runtime's bounded (lossy)
//! subscription, sorts each event into the pending frame of every viewer that wants it, and at each viewer's
//! frame deadline encodes the frame within the viewer's byte budget: exact spikes if they fit, else spike counts
//! per neuron, else per spatial cell. A viewer that reads too slowly loses frames (counted in the next one it
//! gets); the simulation never waits. Viewers survive rebuilds: the hub just switches to the new runtime's events.

use std::collections::{BTreeMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use spikes_api::frame::{FrameBuilder, FrameHeader, Lod};
use spikes_model::Transition;
use spikes_network::{Event, Runtime};
use tempo_runtime::{ClockHandle, Emitted, EventStream, SimTime};

use crate::network::NetworkInfo;

/// Frames a viewer may have waiting before newer ones are dropped.
const VIEWER_QUEUE: usize = 3;
/// Batches the engine's subscription may hold before it drops events.
const SUBSCRIPTION_BATCHES: usize = 4096;
/// The longest the aggregator waits for events before checking deadlines.
const TICK: Duration = Duration::from_millis(2);

/// Which neurons a viewer wants data for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Selection {
    /// Every neuron.
    All,
    /// These neurons (by index).
    Neurons(Vec<u32>),
    /// None.
    Nothing,
}

impl Selection {
    fn mask(&self, n: usize) -> Option<Vec<bool>> {
        match self {
            Self::All => Some(vec![true; n]),
            Self::Nothing => None,
            Self::Neurons(list) => {
                let mut mask = vec![false; n];
                for &i in list {
                    if let Some(m) = mask.get_mut(i as usize) {
                        *m = true;
                    }
                }
                Some(mask)
            }
        }
    }
}

/// What a viewer wants, and how much.
#[derive(Clone, Debug, PartialEq)]
pub struct ViewSettings {
    /// Wall-clock time between frames.
    pub frame: Duration,
    /// Bytes per second the viewer can take; spikes switch to counts or cells to stay under it.
    pub budget_bytes_per_s: usize,
    /// Spikes from these neurons.
    pub spikes: Selection,
    /// Membrane potential, on every update, of these neurons (they are observed for as long as the session lives).
    pub membrane: Vec<u32>,
    /// Weight changes of synapses onto these neurons (the latest value per synapse per frame).
    pub weights: Selection,
}

impl Default for ViewSettings {
    fn default() -> Self {
        Self {
            frame: Duration::from_millis(16),
            budget_bytes_per_s: 4 << 20,
            spikes: Selection::All,
            membrane: Vec::new(),
            weights: Selection::All,
        }
    }
}

/// Events waiting for a viewer's next frame. Times are ns.
#[derive(Default)]
struct Pending {
    spikes: Vec<(u32, u64)>,
    membrane: Vec<(u32, u64, f32)>,
    weights: BTreeMap<(u32, u32), f32>,
    transitions: Vec<(u32, u64, bool)>,
    t_min: Option<u64>,
    t_max: Option<u64>,
}

impl Pending {
    fn time(&mut self, t: u64) {
        self.t_min = Some(self.t_min.map_or(t, |m| m.min(t)));
        self.t_max = Some(self.t_max.map_or(t, |m| m.max(t)));
    }

    fn is_empty(&self) -> bool {
        self.spikes.is_empty() && self.membrane.is_empty() && self.weights.is_empty() && self.transitions.is_empty()
    }
}

struct Slot {
    id: u64,
    settings: ViewSettings,
    spike_mask: Option<Vec<bool>>,
    membrane_mask: Vec<bool>,
    weight_mask: Option<Vec<bool>>,
    tx: SyncSender<Vec<u8>>,
    seq: u32,
    dropped_frames: u32,
    deadline: Instant,
    pending: Pending,
    last_t1: u64,
}

impl Slot {
    fn configure(&mut self, settings: ViewSettings, n: usize) {
        self.spike_mask = settings.spikes.mask(n);
        self.weight_mask = settings.weights.mask(n);
        self.membrane_mask = Selection::Neurons(settings.membrane.clone())
            .mask(n)
            .unwrap_or_default();
        self.settings = settings;
    }
}

struct Source {
    stream: EventStream<Event>,
    clock: ClockHandle,
    network: Arc<NetworkInfo>,
}

/// A session's viewers and their aggregator.
pub(crate) struct Hub {
    session: u32,
    slots: Mutex<Vec<Slot>>,
    source: Mutex<Option<Source>>,
    next_id: AtomicU64,
    /// The latest simulation time the session reported (ns): the clock's progress doesn't cover a run that ends
    /// as fast as possible after its last event, or steps taken while not running.
    known_ns: AtomicU64,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl Hub {
    /// A hub with its aggregator thread, which ends when the hub is dropped.
    pub(crate) fn start(session: u32) -> Arc<Self> {
        let hub = Arc::new(Self {
            session,
            slots: Mutex::new(Vec::new()),
            source: Mutex::new(None),
            next_id: AtomicU64::new(0),
            known_ns: AtomicU64::new(0),
        });
        let weak = Arc::downgrade(&hub);
        // If the thread can't start, viewers just get no frames.
        let _ = std::thread::Builder::new()
            .name(format!("session-{session}-frames"))
            .spawn(move || aggregate(&weak));
        hub
    }

    /// Switches to a newly built runtime's events.
    pub(crate) fn connect(&self, rt: &Runtime, network: Arc<NetworkInfo>) {
        let n = network.neurons.len();
        for slot in lock(&self.slots).iter_mut() {
            let settings = slot.settings.clone();
            slot.configure(settings, n);
            slot.pending = Pending::default();
            slot.last_t1 = 0;
        }
        self.known_ns.store(0, Ordering::Relaxed);
        *lock(&self.source) = Some(Source {
            stream: rt.subscribe(SUBSCRIPTION_BATCHES),
            clock: rt.clock(),
            network,
        });
    }

    /// Records that simulation time has reached `t`.
    pub(crate) fn advance(&self, t: SimTime) {
        self.known_ns.fetch_max(t.since_start().as_nanos(), Ordering::Relaxed);
    }

    /// Every neuron some viewer wants membrane data for.
    pub(crate) fn membrane_neurons(&self) -> Vec<u32> {
        let set: HashSet<u32> = lock(&self.slots)
            .iter()
            .flat_map(|s| s.settings.membrane.iter().copied())
            .collect();
        set.into_iter().collect()
    }

    pub(crate) fn attach(self: &Arc<Self>, settings: ViewSettings, n: usize) -> Viewer {
        let (tx, rx) = sync_channel(VIEWER_QUEUE);
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let mut slot = Slot {
            id,
            settings: settings.clone(),
            spike_mask: None,
            membrane_mask: Vec::new(),
            weight_mask: None,
            tx,
            seq: 0,
            dropped_frames: 0,
            deadline: Instant::now() + settings.frame,
            pending: Pending::default(),
            last_t1: 0,
        };
        slot.configure(settings, n);
        lock(&self.slots).push(slot);
        Viewer {
            id,
            frames: rx,
            hub: Arc::downgrade(self),
        }
    }

    pub(crate) fn update(&self, id: u64, settings: ViewSettings, n: usize) {
        if let Some(slot) = lock(&self.slots).iter_mut().find(|s| s.id == id) {
            slot.configure(settings, n);
        }
    }

    fn detach(&self, id: u64) {
        lock(&self.slots).retain(|s| s.id != id);
    }
}

/// A live view of a session: frames arrive as encoded bytes (see `spikes_api::frame`). Dropping it detaches.
#[derive(Debug)]
pub struct Viewer {
    id: u64,
    frames: Receiver<Vec<u8>>,
    hub: Weak<Hub>,
}

impl Viewer {
    /// The viewer's id within its session.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// The next frame, waiting up to `timeout`.
    pub fn recv_timeout(&self, timeout: Duration) -> Option<Vec<u8>> {
        self.frames.recv_timeout(timeout).ok()
    }

    /// A frame if one is waiting.
    pub fn try_recv(&self) -> Option<Vec<u8>> {
        self.frames.try_recv().ok()
    }
}

impl Drop for Viewer {
    fn drop(&mut self) {
        if let Some(hub) = self.hub.upgrade() {
            hub.detach(self.id);
        }
    }
}

impl std::fmt::Debug for Hub {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Hub")
            .field("session", &self.session)
            .finish_non_exhaustive()
    }
}

fn aggregate(hub: &Weak<Hub>) {
    loop {
        let Some(hub) = hub.upgrade() else { return };
        let batch = {
            let source = lock(&hub.source);
            match source.as_ref().map(|s| s.stream.recv_timeout(TICK)) {
                Some(Ok(batch)) => batch,
                Some(Err(_)) | None => {
                    drop(source);
                    std::thread::sleep(TICK); // no runtime (yet, or any more)
                    None
                }
            }
        };
        let source = lock(&hub.source);
        let Some(source) = source.as_ref() else { continue };
        let mut slots = lock(&hub.slots);
        if let Some(batch) = batch {
            for e in &batch {
                route(e, &source.network, &mut slots);
            }
        }
        let now = Instant::now();
        let dropped_events = u32::try_from(source.stream.dropped()).unwrap_or(u32::MAX);
        for slot in slots.iter_mut().filter(|s| s.deadline <= now) {
            slot.deadline = now + slot.settings.frame;
            emit(
                hub.session,
                slot,
                source,
                dropped_events,
                hub.known_ns.load(Ordering::Relaxed),
            );
        }
    }
}

fn route(e: &Emitted<Event>, network: &NetworkInfo, slots: &mut [Slot]) {
    let Some(neuron) = network.index(e.src) else { return };
    let i = neuron as usize;
    let at = e.at.since_start().as_nanos();
    for slot in slots.iter_mut() {
        let p = &mut slot.pending;
        match e.event {
            Event::Spike { at: spike } => {
                if slot.spike_mask.as_ref().is_some_and(|m| m[i]) {
                    let t = spike.since_start().as_nanos();
                    p.spikes.push((neuron, t));
                    p.time(t);
                }
            }
            Event::Membrane { mv } => {
                if slot.membrane_mask.get(i).copied().unwrap_or(false) {
                    p.membrane.push((neuron, at, mv));
                    p.time(at);
                }
            }
            Event::Weight { synapse, after, .. } => {
                if slot.weight_mask.as_ref().is_some_and(|m| m[i]) {
                    p.weights.insert((neuron, synapse), after);
                    p.time(at);
                }
            }
            Event::Transition(t) => {
                p.transitions.push((neuron, at, t == Transition::StartedTonic));
                p.time(at);
            }
        }
    }
}

fn emit(session: u32, slot: &mut Slot, source: &Source, dropped_events: u32, known_ns: u64) {
    let clock = &source.clock;
    let sim = clock.sim_time().since_start().as_nanos().max(known_ns);
    if slot.pending.is_empty() && sim == slot.last_t1 {
        return; // nothing new to show
    }
    let p = std::mem::take(&mut slot.pending);
    let t0 = p.t_min.unwrap_or(slot.last_t1).min(sim.max(slot.last_t1));
    let t1 = p.t_max.unwrap_or(0).max(sim);
    slot.last_t1 = sim;
    let unit = (t1 - t0).div_ceil(u64::from(u32::MAX)).max(1);
    let offset = |t: u64| ((t - t0) / unit) as u32;

    // Spikes: exact if they fit the budget, else counts per neuron, else per cell. The budget is for section
    // payloads; the 64-byte header and 16-byte section headers are on top.
    let budget = (slot.settings.budget_bytes_per_s as f64 * slot.settings.frame.as_secs_f64()) as usize;
    let fixed = p.membrane.len() * 12 + p.weights.len() * 12 + p.transitions.len() * 9;
    let room = budget.saturating_sub(fixed);
    let mut lod = Lod::Exact;
    let mut counts: BTreeMap<u32, u32> = BTreeMap::new();
    if column_bytes(p.spikes.len(), 4) * 2 > room {
        lod = Lod::Counts;
        for &(n, _) in &p.spikes {
            *counts.entry(n).or_default() += 1;
        }
        if column_bytes(counts.len(), 4) + column_bytes(counts.len(), 2) > room {
            lod = Lod::Binned;
            let mut cells: BTreeMap<u32, u32> = BTreeMap::new();
            for (&n, &c) in &counts {
                *cells.entry(source.network.cells[n as usize]).or_default() += c;
            }
            counts = cells;
        }
    }

    let speed = match clock.time_factor() {
        _ if clock.is_paused() => 0.0,
        None => f32::INFINITY,
        Some(f) => (1.0 / f) as f32,
    };
    let lag_ms = clock.horizon().map_or(0.0, |h: SimTime| {
        h.since_start().as_nanos().saturating_sub(sim) as f32 / 1e6
    });
    let header = FrameHeader {
        session,
        seq: slot.seq,
        t0_ns: t0,
        t1_ns: t1,
        wall_unix_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64),
        speed,
        lag_ms,
        lod,
        dropped_events,
        dropped_frames: slot.dropped_frames,
        time_unit_ns: u32::try_from(unit).unwrap_or(u32::MAX),
        ..FrameHeader::default()
    };
    let mut b = FrameBuilder::new(&header);
    match lod {
        Lod::Exact if !p.spikes.is_empty() => {
            let (n, t): (Vec<u32>, Vec<u32>) = p.spikes.iter().map(|&(n, t)| (n, offset(t))).unzip();
            b.spikes(&n, &t);
        }
        Lod::Exact => {}
        Lod::Counts => {
            let (n, c): (Vec<u32>, Vec<u16>) = counts
                .iter()
                .map(|(&n, &c)| (n, u16::try_from(c).unwrap_or(u16::MAX)))
                .unzip();
            b.spike_counts(&n, &c);
        }
        Lod::Binned => {
            let (cell, c): (Vec<u32>, Vec<u32>) = counts.into_iter().unzip();
            b.spike_bins(&cell, &c);
        }
    }
    if !p.membrane.is_empty() {
        let n: Vec<u32> = p.membrane.iter().map(|m| m.0).collect();
        let t: Vec<u32> = p.membrane.iter().map(|m| offset(m.1)).collect();
        let v: Vec<f32> = p.membrane.iter().map(|m| m.2).collect();
        b.membrane(&n, &t, &v);
    }
    if !p.weights.is_empty() {
        let (keys, w): (Vec<(u32, u32)>, Vec<f32>) = p.weights.into_iter().unzip();
        let (post, syn): (Vec<u32>, Vec<u32>) = keys.into_iter().unzip();
        b.weights(&post, &syn, &w);
    }
    if !p.transitions.is_empty() {
        let n: Vec<u32> = p.transitions.iter().map(|x| x.0).collect();
        let t: Vec<u32> = p.transitions.iter().map(|x| offset(x.1)).collect();
        let tonic: Vec<bool> = p.transitions.iter().map(|x| x.2).collect();
        b.transitions(&n, &t, &tonic);
    }
    slot.seq = slot.seq.wrapping_add(1);
    match slot.tx.try_send(b.finish()) {
        Ok(()) | Err(TrySendError::Disconnected(_)) => {}
        Err(TrySendError::Full(_)) => slot.dropped_frames = slot.dropped_frames.saturating_add(1),
    }
}

/// A column's size in a frame: `count × width`, padded to 8 bytes.
fn column_bytes(count: usize, width: usize) -> usize {
    (count * width).div_ceil(8) * 8
}
