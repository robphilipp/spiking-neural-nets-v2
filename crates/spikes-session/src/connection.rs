//! One client's connection to the engine, independent of transport (app plan 01 §4).
//!
//! The WebSocket server and the Tauri app both wrap a [`Connection`]: they hand it JSON requests and binary sensor
//! batches, and send on whatever comes out of its outgoing stream: responses, session notices, and the frames of
//! the viewers this client attached. Dropping the connection detaches its viewers and disconnects its sensors.
//!
//! Requests run in order, on the caller's thread; some block briefly (stopping a run waits for it to end).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use spikes_api::input::InputBatch;
use spikes_api::protocol::{
    self as api, ApiError, ClientMessage, ErrorCode, Notice, Outcome, Request, Response, ServerMessage, SourceError,
};
use tempo_runtime::{SimDuration, SimTime};

use crate::sensors::{Encoding, OnPause, SampleBatch, Sensor, SensorSpec, SpikeBatch};
use crate::session::{BuildSettings, InputSource, InputSpec, RunSettings, Session, SessionNotice, Speed, State};
use crate::viewer::{Selection, ViewSettings, Viewer};
use crate::{Engine, SessionError, SessionId};

/// Something to send to the client.
#[derive(Clone, Debug, PartialEq)]
pub enum Outgoing {
    /// A JSON message (a response or a notice).
    Message(ServerMessage),
    /// A binary frame.
    Frame(Vec<u8>),
}

/// How long background forwarders wait before checking whether the connection is gone.
const POLL: Duration = Duration::from_millis(100);

struct Forwarder {
    stop: Arc<AtomicBool>,
}

impl Drop for Forwarder {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// One client's view of an engine.
pub struct Connection {
    engine: Arc<Engine>,
    recordings: PathBuf,
    out: Sender<Outgoing>,
    /// (session, viewer) → the thread forwarding its frames (it owns the viewer), and the viewer's id for updates.
    viewers: BTreeMap<(u32, u32), (Forwarder, u64)>,
    /// Connection-wide sensor id → sensor.
    sensors: BTreeMap<u32, Sensor>,
    next_sensor: u32,
    /// Sessions whose notices this connection forwards.
    watched: BTreeMap<u32, Forwarder>,
}

impl std::fmt::Debug for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connection")
            .field("viewers", &self.viewers.len())
            .field("sensors", &self.sensors.len())
            .finish_non_exhaustive()
    }
}

impl Connection {
    /// A connection to `engine`; recorded runs go under `recordings`. Returns the stream of things to send.
    pub fn new(engine: Arc<Engine>, recordings: impl Into<PathBuf>) -> (Self, Receiver<Outgoing>) {
        let (out, rx) = channel();
        (
            Self {
                engine,
                recordings: recordings.into(),
                out,
                viewers: BTreeMap::new(),
                sensors: BTreeMap::new(),
                next_sensor: 0,
                watched: BTreeMap::new(),
            },
            rx,
        )
    }

    /// Handles a JSON text message; the response also goes to the outgoing stream. A message that isn't valid
    /// JSON for a [`ClientMessage`] gets an error response with id 0.
    pub fn handle_text(&mut self, text: &str) {
        let message = match serde_json::from_str::<ClientMessage>(text) {
            Ok(message) => message,
            Err(e) => {
                let _ = self.out.send(Outgoing::Message(ServerMessage::Response {
                    id: 0,
                    result: Outcome::Error(error(ErrorCode::Invalid, format!("not a valid request: {e}"))),
                }));
                return;
            }
        };
        let response = self.handle(message);
        let _ = self.out.send(Outgoing::Message(response));
    }

    /// Handles a binary message: a sensor input batch.
    pub fn handle_binary(&mut self, bytes: &[u8]) -> Result<(), ApiError> {
        let batch = InputBatch::decode(bytes).map_err(|e| error(ErrorCode::Invalid, e.to_string()))?;
        let sensor = self
            .sensors
            .get(&batch.sensor())
            .ok_or_else(|| error(ErrorCode::NotFound, format!("sensor {}", batch.sensor())))?;
        match batch {
            InputBatch::Samples {
                t0_ns, dt_ns, values, ..
            } => sensor.send_samples(&SampleBatch {
                t0_ns,
                dt_ns: u64::from(dt_ns),
                values,
            }),
            InputBatch::Events { t0_ns, events, .. } => sensor.send_spikes(&SpikeBatch {
                t0_ns,
                events: events.into_iter().map(|(c, o)| (c, u64::from(o))).collect(),
            }),
        }
        .map_err(api_error)
    }

    /// Handles a request and returns the response.
    pub fn handle(&mut self, message: ClientMessage) -> ServerMessage {
        let result = match self.dispatch(message.request) {
            Ok(response) => Outcome::Ok(response),
            Err(e) => Outcome::Error(e),
        };
        ServerMessage::Response { id: message.id, result }
    }

    fn session(&mut self, id: u32) -> Result<Session, ApiError> {
        let session = self.engine.session(SessionId(id)).map_err(api_error)?;
        self.watch(&session);
        Ok(session)
    }

    /// Forwards a session's notices to this client (once per session).
    fn watch(&mut self, session: &Session) {
        let id = session.id().0;
        if self.watched.contains_key(&id) {
            return;
        }
        let stop = Arc::new(AtomicBool::new(false));
        let notices = session.notices();
        let out = self.out.clone();
        let stopping = Arc::clone(&stop);
        let spawned = std::thread::Builder::new()
            .name(format!("notices-{id}"))
            .spawn(move || {
                while !stopping.load(Ordering::Relaxed) {
                    match notices.recv_timeout(POLL) {
                        Ok(notice) => {
                            let message = ServerMessage::Notice {
                                session: id,
                                notice: notice_dto(notice),
                            };
                            if out.send(Outgoing::Message(message)).is_err() {
                                return;
                            }
                        }
                        Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => return,
                    }
                }
            });
        if spawned.is_ok() {
            self.watched.insert(id, Forwarder { stop });
        }
    }

    fn dispatch(&mut self, request: Request) -> Result<Response, ApiError> {
        match request {
            Request::ListSessions => Ok(Response::Sessions {
                sessions: self.engine.sessions().iter().map(summary).collect(),
            }),
            Request::CreateSession { name, description } => {
                let session = self.engine.create(name, description).map_err(api_error)?;
                self.watch(&session);
                Ok(Response::Session {
                    session: summary(&session),
                })
            }
            Request::UpdateDescription { session, description } => {
                self.session(session)?
                    .update_description(description)
                    .map_err(api_error)?;
                Ok(Response::Done)
            }
            Request::CloseSession { session } => {
                self.viewers.retain(|(s, _), _| *s != session);
                self.watched.remove(&session);
                self.engine.close(SessionId(session)).map_err(api_error)?;
                Ok(Response::Done)
            }
            Request::Build { session, settings } => {
                let s = self.session(session)?;
                let defaults = BuildSettings::default();
                let min_delay = match settings.min_delay_ms {
                    Some(ms) if !(ms.is_finite() && ms >= 0.0) => {
                        return Err(error(ErrorCode::Invalid, format!("minDelayMs must be ≥ 0, not {ms}")));
                    }
                    Some(ms) => SimDuration::from_millis_f64(ms),
                    None => defaults.min_delay,
                };
                s.set_nodes(settings.nodes.clone().unwrap_or_default())
                    .map_err(api_error)?;
                s.build(BuildSettings {
                    seed: settings.seed.map_or(defaults.seed, u64::from),
                    shards: settings.shards.map_or(defaults.shards, |n| n as usize),
                    min_delay,
                    record_weights: settings.record_weights.unwrap_or(defaults.record_weights),
                    ..defaults
                })
                .map_err(api_error)?;
                Ok(Response::Done)
            }
            Request::Reset { session } => {
                self.session(session)?.reset().map_err(api_error)?;
                Ok(Response::Done)
            }
            Request::GetDescription { session } => Ok(Response::Description {
                text: self.session(session)?.description_text(),
            }),
            Request::GetCluster { session } => {
                let s = self.session(session)?;
                let status = s
                    .cluster_status()
                    .ok_or_else(|| error(ErrorCode::Invalid, "the session isn't built on nodes"))?;
                let stats = &status.link;
                let ms = |d: std::time::Duration| d.as_secs_f64() * 1e3;
                let links: Vec<api::ClusterLink> = status
                    .links
                    .iter()
                    .map(|(from, to, a)| api::ClusterLink {
                        from: *from,
                        to: *to,
                        events: a.events,
                        late: a.late,
                        max_late_ms: ms(a.max_late),
                        p99_late_ms: ms(a.p99_late),
                    })
                    .collect();
                let nodes = std::iter::once("coordinator".to_string())
                    .chain(status.nodes.clone())
                    .enumerate()
                    .map(|(i, address)| api::ClusterNode {
                        address,
                        events_sent: stats.sent.get(i).copied().unwrap_or(0),
                        bytes_sent: stats.bytes_sent.get(i).copied().unwrap_or(0),
                    })
                    .collect();
                Ok(Response::Cluster {
                    cluster: api::Cluster {
                        nodes,
                        rounds: stats.rounds,
                        waited_ms: stats.waited.as_secs_f64() * 1e3,
                        received: stats.received,
                        late: links.iter().map(|l| l.late).sum(),
                        lockstep: status.lockstep,
                        links,
                        failure: stats.failure.clone(),
                    },
                })
            }
            Request::GetDeployment { session } => {
                let d = self
                    .session(session)?
                    .deployment()
                    .ok_or_else(|| error(ErrorCode::Invalid, "the network isn't built"))?;
                Ok(Response::Deployment {
                    deployment: api::Deployment {
                        nodes: d
                            .nodes
                            .into_iter()
                            .map(|n| api::DeployedNode {
                                name: n.name,
                                address: n.address,
                                shard_neurons: n.shard_neurons,
                                synapses: n.synapses,
                            })
                            .collect(),
                        neuron_nodes: d.neuron_nodes,
                        neuron_shards: d.neuron_shards,
                        neuron_pins: d.neuron_pins,
                        groups: d
                            .groups
                            .into_iter()
                            .map(|g| api::DeployedGroup {
                                name: g.name,
                                kind: match g.kind {
                                    crate::GroupKind::Population => "population",
                                    crate::GroupKind::Set => "set",
                                    crate::GroupKind::Neuron => "neuron",
                                }
                                .into(),
                                pinned: g.pinned,
                                per_node: g.per_node,
                            })
                            .collect(),
                        declared_nodes: d.declared_nodes,
                        warnings: d.warnings,
                    },
                })
            }
            Request::GetNetwork { session } => Ok(Response::Network {
                network: api_network(&self.session(session)?.network()),
            }),
            Request::PreviewNetwork { text } => {
                let p = crate::preview(&text).map_err(api_error)?;
                let split = |v: &[(u32, u32)]| -> (Vec<u32>, Vec<u32>) { v.iter().copied().unzip() };
                let (incoming, incoming_inhibitory) = split(&p.incoming);
                let (outgoing, outgoing_inhibitory) = split(&p.outgoing);
                Ok(Response::Preview {
                    preview: Box::new(api::NetworkPreview {
                        network: api_network(&p.network),
                        incoming,
                        incoming_inhibitory,
                        outgoing,
                        outgoing_inhibitory,
                        pinned: p.pinned,
                    }),
                })
            }
            Request::Start {
                session,
                speed,
                duration_ms,
                record,
                lockstep,
            } => {
                let s = self.session(session)?;
                if let Some(lockstep) = lockstep {
                    s.set_lockstep(lockstep);
                }
                let record = record.then(|| self.recording_dir(&s));
                s.start(RunSettings {
                    speed: speed_of(speed)?,
                    duration: duration_ms.map(duration).transpose()?,
                    record,
                })
                .map_err(api_error)?;
                Ok(Response::Done)
            }
            Request::Pause { session } => self.session(session)?.pause().map(done).map_err(api_error),
            Request::Resume { session } => self.session(session)?.resume().map(done).map_err(api_error),
            Request::Step { session, ms } => self.session(session)?.step(duration(ms)?).map(done).map_err(api_error),
            Request::SetSpeed { session, speed } => self
                .session(session)?
                .set_speed(speed_of(speed)?)
                .map(done)
                .map_err(api_error),
            Request::Stop { session } => self.session(session)?.stop().map(done).map_err(api_error),
            Request::Stimulate { session, neurons, mv } => self
                .session(session)?
                .stimulate(&neurons, mv)
                .map(done)
                .map_err(api_error),
            Request::ReleaseModulator {
                session,
                modulator,
                amount,
            } => self
                .session(session)?
                .release_modulator(modulator, amount)
                .map(done)
                .map_err(api_error),
            Request::AddInput { session, input } => {
                let input = input_spec(input)?;
                self.session(session)?.add_input(input).map(done).map_err(api_error)
            }
            Request::SetInputs {
                session,
                sources,
                trials,
            } => {
                let trials = trials
                    .into_iter()
                    .map(|t| {
                        Ok(crate::Trial {
                            number: t.number,
                            condition: t.condition,
                            start: duration(t.start_ms)?,
                            end: duration(t.end_ms)?,
                        })
                    })
                    .collect::<Result<Vec<_>, ApiError>>()?;
                let sources = sources
                    .into_iter()
                    .map(|s| {
                        Ok(InputSource {
                            start: duration(s.start_ms)?,
                            input: input_spec(s.input).map_err(|e| {
                                error(ErrorCode::Invalid, format!("input '{}': {}", s.name, e.message))
                            })?,
                            name: s.name,
                        })
                    })
                    .collect::<Result<Vec<_>, ApiError>>()?;
                self.session(session)?.set_inputs(sources, trials).map(done).map_err(api_error)
            }
            Request::GetTrials { session } => Ok(Response::Trials {
                trials: self
                    .session(session)?
                    .trials()
                    .into_iter()
                    .map(|t| api::Trial {
                        number: t.number,
                        condition: t.condition,
                        start_ms: t.start.as_millis_f64(),
                        end_ms: t.end.as_millis_f64(),
                    })
                    .collect(),
            }),
            Request::Fire { session, source } => {
                self.session(session)?.fire(&source).map(done).map_err(api_error)
            }
            Request::Connect {
                session,
                pre,
                post,
                weight,
                inhibitory,
                learning,
            } => self
                .session(session)?
                .connect(pre, post, weight, inhibitory, learning.as_deref())
                .map(done)
                .map_err(api_error),
            Request::Prune { session, pre, post } => {
                self.session(session)?.prune(pre, post).map(done).map_err(api_error)
            }
            Request::QueryNeuron { session, neuron } => {
                let session = self.session(session)?;
                let now = session.sim_time();
                let n = session.query_neuron(neuron).map_err(api_error)?;
                Ok(Response::Neuron {
                    neuron: api::NeuronState {
                        membrane_mv: n.membrane_mv,
                        intrinsic_mv: n.intrinsic_mv,
                        rate_hz: n.rate_hz,
                        last_event_ms: ms_of(n.last_event),
                        last_spike_ms: n.last_spike.map(ms_of),
                        now_ms: ms_of(now),
                        tonic: n.tonic,
                        spikes: u32::try_from(n.spikes).unwrap_or(u32::MAX),
                        synapses: n
                            .synapses
                            .iter()
                            .map(|&(pre, weight, eligibility)| api::SynapseState {
                                pre,
                                weight,
                                eligibility,
                            })
                            .collect(),
                    },
                })
            }
            Request::Observe { session, neurons, on } => self
                .session(session)?
                .observe(&neurons, on)
                .map(done)
                .map_err(api_error),
            Request::Attach { session, view } => {
                let s = self.session(session)?;
                let viewer = s.attach(view_settings(&view)?);
                let full_id = viewer.id();
                let id = u32::try_from(full_id).unwrap_or(u32::MAX);
                let forwarder = self.forward_frames(session, id, viewer)?;
                self.viewers.insert((session, id), (forwarder, full_id));
                Ok(Response::Viewer { viewer: id })
            }
            Request::UpdateView { session, viewer, view } => {
                let s = self.session(session)?;
                let &(_, full_id) = self
                    .viewers
                    .get(&(session, viewer))
                    .ok_or_else(|| error(ErrorCode::NotFound, format!("viewer {viewer}")))?;
                s.update_view_id(full_id, view_settings(&view)?);
                Ok(Response::Done)
            }
            Request::Detach { session, viewer } => {
                self.viewers
                    .remove(&(session, viewer))
                    .ok_or_else(|| error(ErrorCode::NotFound, format!("viewer {viewer}")))?;
                Ok(Response::Done)
            }
            Request::ConnectSensor { session, sensor } => {
                let s = self.session(session)?;
                let spec = SensorSpec {
                    name: sensor.name,
                    targets: sensor.targets,
                    encoding: match sensor.encoding {
                        api::Encoding::Current { gain_mv } => Encoding::Current { gain_mv },
                        api::Encoding::Rate { max_hz, mv } => Encoding::Rate { max_hz, mv },
                        api::Encoding::Threshold { level, mv } => Encoding::Threshold { level, mv },
                        api::Encoding::Spikes { mv } => Encoding::Spikes { mv },
                    },
                    ingress_delay: match sensor.ingress_delay_ms {
                        Some(ms) => duration(ms)?,
                        None => SimDuration::from_millis(20),
                    },
                    on_pause: if sensor.buffer_while_paused {
                        OnPause::Buffer
                    } else {
                        OnPause::Drop
                    },
                };
                let connected = s.connect_sensor(spec).map_err(api_error)?;
                let id = self.next_sensor;
                self.next_sensor += 1;
                self.sensors.insert(id, connected);
                Ok(Response::Sensor { sensor: id })
            }
            Request::ListSensors { session } => Ok(Response::Sensors {
                sensors: self
                    .session(session)?
                    .sensors()
                    .into_iter()
                    .map(|s| api::SensorInfo {
                        id: s.key,
                        name: s.spec.name.clone(),
                        channels: s.spec.targets.clone(),
                        encoding: match s.spec.encoding {
                            Encoding::Current { .. } => "current",
                            Encoding::Rate { .. } => "rate",
                            Encoding::Threshold { .. } => "threshold",
                            Encoding::Spikes { .. } => "spikes",
                        }
                        .into(),
                        samples: s.stats.samples,
                        inputs: s.stats.inputs,
                    })
                    .collect(),
            }),
            Request::DisconnectSensor { sensor, .. } => {
                self.sensors
                    .remove(&sensor)
                    .ok_or_else(|| error(ErrorCode::NotFound, format!("sensor {sensor}")))?;
                Ok(Response::Done)
            }
            Request::ListRecordings => Ok(Response::Recordings {
                recordings: list_recordings(&self.recordings),
            }),
            Request::OpenRecording { id } => {
                let dir = self.recording(&id)?;
                let session = self.engine.open_recording(&dir).map_err(api_error)?;
                self.watch(&session);
                Ok(Response::Session {
                    session: summary(&session),
                })
            }
            Request::ExportParquet { id } => {
                let dir = self.recording(&id)?;
                spikes_record::to_parquet(&dir).map_err(|e| error(ErrorCode::Io, e.to_string()))?;
                Ok(Response::Done)
            }
            Request::Seek { session, ms } => self
                .session(session)?
                .seek(SimTime::from_start(duration(ms)?))
                .map(done)
                .map_err(api_error),
            Request::SaveRun { session, name } => {
                let session = self.session(session)?;
                let (bytes, at) = session.save_timed().map_err(api_error)?;
                let at_ms = at.since_start().as_millis_f64();
                let dir = self.recordings.join(SAVES_DIR);
                std::fs::create_dir_all(&dir).map_err(|e| error(ErrorCode::Io, e.to_string()))?;
                let stamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |d| d.as_millis());
                let name = plain_name(&name);
                let id = format!("{stamp}-{name}-{}ms.{SAVE_EXTENSION}", at_ms.round());
                std::fs::write(dir.join(&id), &bytes).map_err(|e| error(ErrorCode::Io, e.to_string()))?;
                Ok(Response::Saved {
                    save: api::SavedRun {
                        id,
                        name,
                        at_ms: at_ms.round(),
                        bytes: bytes.len() as f64,
                    },
                })
            }
            Request::ListSaves => Ok(Response::Saves {
                saves: list_saves(&self.recordings.join(SAVES_DIR)),
            }),
            Request::OpenSave { id, shards, nodes } => {
                let save = list_saves(&self.recordings.join(SAVES_DIR))
                    .into_iter()
                    .find(|s| s.id == id)
                    .ok_or_else(|| error(ErrorCode::NotFound, format!("saved run '{id}'")))?;
                let bytes = std::fs::read(self.recordings.join(SAVES_DIR).join(&save.id))
                    .map_err(|e| error(ErrorCode::Io, e.to_string()))?;
                let shards = shards.map_or(BuildSettings::default().shards, |n| n as usize);
                let session = self
                    .engine
                    .open_saved(save.name, &bytes, shards, nodes.unwrap_or_default())
                    .map_err(api_error)?;
                self.watch(&session);
                Ok(Response::Session {
                    session: summary(&session),
                })
            }
        }
    }

    /// A recording's directory, from its id (one plain path segment under the recordings folder).
    fn recording(&self, id: &str) -> Result<PathBuf, ApiError> {
        let mut parts = Path::new(id).components();
        let plain = matches!(parts.next(), Some(std::path::Component::Normal(_))) && parts.next().is_none();
        let dir = self.recordings.join(id);
        if !plain || !dir.is_dir() {
            return Err(error(ErrorCode::NotFound, format!("recording '{id}'")));
        }
        Ok(dir)
    }

    fn recording_dir(&self, session: &Session) -> PathBuf {
        let name: String = session
            .name()
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        self.recordings.join(format!("{stamp}-{name}-s{}", session.id()))
    }

    /// Forwards a viewer's frames on a thread of its own, which owns the viewer (updates go through the session by
    /// the viewer's id, so they never wait for this thread). Stopping the forwarder drops, so detaches, the viewer.
    fn forward_frames(&self, session: u32, id: u32, viewer: Viewer) -> Result<Forwarder, ApiError> {
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = Arc::clone(&stop);
        let out = self.out.clone();
        std::thread::Builder::new()
            .name(format!("frames-{session}-{id}"))
            .spawn(move || {
                while !stopping.load(Ordering::Relaxed) {
                    if let Some(frame) = viewer.recv_timeout(Duration::from_millis(20))
                        && out.send(Outgoing::Frame(frame)).is_err()
                    {
                        break;
                    }
                }
                drop(viewer);
            })
            .map_err(|e| error(ErrorCode::Internal, format!("couldn't start a frame forwarder: {e}")))?;
        Ok(Forwarder { stop })
    }
}

fn done(_: ()) -> Response {
    Response::Done
}

fn error(code: ErrorCode, message: impl Into<String>) -> ApiError {
    ApiError {
        code,
        message: message.into(),
        spans: Vec::new(),
    }
}

/// The protocol form of an engine error.
pub fn api_error(e: SessionError) -> ApiError {
    let code = match &e {
        SessionError::Parse(_) => ErrorCode::Parse,
        SessionError::State { .. } => ErrorCode::InvalidState,
        SessionError::NotFound(_) => ErrorCode::NotFound,
        SessionError::Invalid(_) => ErrorCode::Invalid,
        SessionError::Record(_) => ErrorCode::Io,
        SessionError::Timeout => ErrorCode::Timeout,
    };
    let spans = match &e {
        SessionError::Parse(errors) => errors
            .iter()
            .map(|p| SourceError {
                line: p.span.line,
                column: p.span.column,
                message: p.message.clone(),
            })
            .collect(),
        _ => Vec::new(),
    };
    ApiError {
        code,
        message: e.to_string(),
        spans,
    }
}

fn state_dto(state: State) -> api::State {
    match state {
        State::Loaded => api::State::Loaded,
        State::Ready => api::State::Ready,
        State::Running => api::State::Running,
        State::Paused => api::State::Paused,
        State::Failed => api::State::Failed,
    }
}

fn summary(s: &Session) -> api::SessionSummary {
    api::SessionSummary {
        id: s.id().0,
        name: s.name().to_string(),
        state: state_dto(s.state()),
        sim_ms: ms_of(s.sim_time()),
        speed: match s.speed() {
            Speed::AsFastAsPossible => None,
            Speed::Factor(f) => Some(f),
        },
        neurons: s.network().neurons.len() as u32,
        replay_end_ms: s.replay_end().map(ms_of),
        nodes: s.cluster_status().map_or(1, |c| c.nodes.len() as u32 + 1),
    }
}

fn notice_dto(notice: SessionNotice) -> Notice {
    match notice {
        SessionNotice::StateChanged(state) => Notice::StateChanged {
            state: state_dto(state),
        },
        SessionNotice::RunEnded { at, recording } => Notice::RunEnded {
            sim_ms: ms_of(at),
            recording: recording.and_then(|r| r.dir.file_name().map(|n| n.to_string_lossy().to_string())),
        },
        SessionNotice::RecordingFailed(message) => Notice::RecordingFailed { message },
        SessionNotice::Failed(message) => Notice::Failed { message },
        SessionNotice::NetworkChanged => Notice::NetworkChanged,
        SessionNotice::SensorsChanged => Notice::SensorsChanged,
    }
}

fn ms_of(t: SimTime) -> f64 {
    t.since_start().as_millis_f64()
}

/// An API input as the engine runs it.
fn input_spec(input: api::Input) -> Result<InputSpec, ApiError> {
    Ok(match input {
        api::Input::Poisson {
            targets,
            rate_hz,
            mv,
            duration_ms,
        } => InputSpec::Poisson {
            targets,
            rate_hz,
            mv,
            duration: duration_ms.map(duration).transpose()?,
        },
        api::Input::Periodic {
            targets,
            period_ms,
            mv,
            duration_ms,
        } => InputSpec::Periodic {
            targets,
            period: duration(period_ms)?,
            mv,
            duration: duration_ms.map(duration).transpose()?,
        },
        api::Input::Pulses { targets, at_ms, mv } => InputSpec::Pulses {
            targets,
            times: at_ms.into_iter().map(duration).collect::<Result<_, _>>()?,
            mv,
        },
        api::Input::Signal {
            channels,
            formula,
            sample_ms,
            encoding,
            duration_ms,
        } => InputSpec::Signal {
            channels,
            formula,
            sample: duration(sample_ms)?,
            encoding: match encoding {
                api::Encoding::Current { gain_mv } => spikes_signal::SampleEncoding::Current { gain_mv },
                api::Encoding::Rate { max_hz, mv } => spikes_signal::SampleEncoding::Rate { max_hz, mv },
                api::Encoding::Threshold { level, mv } => spikes_signal::SampleEncoding::Threshold { level, mv },
                api::Encoding::Spikes { .. } => {
                    return Err(error(
                        ErrorCode::Invalid,
                        "a signal's encoding is current, rate or threshold, not spikes",
                    ));
                }
            },
            key: 0,
            duration: duration_ms.map(duration).transpose()?,
        },
    })
}

fn duration(ms: f64) -> Result<SimDuration, ApiError> {
    if ms.is_finite() && ms >= 0.0 {
        Ok(SimDuration::from_millis_f64(ms))
    } else {
        Err(error(
            ErrorCode::Invalid,
            format!("a duration must be ≥ 0 ms, not {ms}"),
        ))
    }
}

fn speed_of(speed: Option<f64>) -> Result<Speed, ApiError> {
    match speed {
        None => Ok(Speed::AsFastAsPossible),
        Some(f) if f.is_finite() && f > 0.0 => Ok(Speed::Factor(f)),
        Some(f) => Err(error(ErrorCode::Invalid, format!("speed must be positive, not {f}"))),
    }
}

fn selection(s: &api::Selection) -> Selection {
    match s {
        api::Selection::All => Selection::All,
        api::Selection::Neurons { neurons } => Selection::Neurons(neurons.clone()),
        api::Selection::Nothing => Selection::Nothing,
    }
}

fn view_settings(view: &api::View) -> Result<ViewSettings, ApiError> {
    if !(view.frame_ms.is_finite() && view.frame_ms >= 1.0) {
        return Err(error(
            ErrorCode::Invalid,
            format!("frameMs must be ≥ 1, not {}", view.frame_ms),
        ));
    }
    Ok(ViewSettings {
        frame: Duration::from_secs_f64(view.frame_ms / 1e3),
        budget_bytes_per_s: view.budget_bytes_per_s as usize,
        spikes: selection(&view.spikes),
        membrane: view.membrane.clone(),
        weights: selection(&view.weights),
    })
}

/// Saved runs live in this folder of the recordings folder.
const SAVES_DIR: &str = "saved-runs";
const SAVE_EXTENSION: &str = "spksave";

/// A name as a plain file-name part.
fn plain_name(name: &str) -> String {
    let plain: String = name
        .chars()
        .take(60)
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if plain.is_empty() { "run".into() } else { plain }
}

/// The saved runs in `dir`; their names and times are in their file names (`{stamp}-{name}-{ms}ms.spksave`).
fn list_saves(dir: &Path) -> Vec<api::SavedRun> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut saves: Vec<api::SavedRun> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let id = e.file_name().to_string_lossy().to_string();
            let stem = id.strip_suffix(&format!(".{SAVE_EXTENSION}"))?;
            let (_stamp, rest) = stem.split_once('-')?;
            let (name, at) = rest.rsplit_once('-')?;
            let at_ms = at.strip_suffix("ms")?.parse().ok()?;
            Some(api::SavedRun {
                name: name.to_string(),
                at_ms,
                bytes: e.metadata().ok()?.len() as f64,
                id,
            })
        })
        .collect();
    saves.sort_by(|a, b| a.id.cmp(&b.id));
    saves
}

fn list_recordings(root: &Path) -> Vec<api::Recording> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut recordings: Vec<api::Recording> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir() && e.file_name() != SAVES_DIR)
        .map(|e| {
            let path = e.path();
            // Tables, and the parts of partitioned ones (`spikes/part-00000.parquet`).
            let mut files: Vec<api::RecordingFile> = Vec::new();
            for f in std::fs::read_dir(&path).into_iter().flatten().filter_map(Result::ok) {
                let name = f.file_name().to_string_lossy().to_string();
                let Ok(meta) = f.metadata() else { continue };
                if meta.is_dir() {
                    for part in std::fs::read_dir(f.path()).into_iter().flatten().filter_map(Result::ok) {
                        if let Ok(m) = part.metadata()
                            && m.is_file()
                        {
                            files.push(api::RecordingFile {
                                name: format!("{name}/{}", part.file_name().to_string_lossy()),
                                bytes: m.len() as f64,
                            });
                        }
                    }
                } else {
                    files.push(api::RecordingFile {
                        name,
                        bytes: meta.len() as f64,
                    });
                }
            }
            files.sort_by(|a, b| a.name.cmp(&b.name));
            api::Recording {
                id: e.file_name().to_string_lossy().to_string(),
                path: path.to_string_lossy().to_string(),
                files,
            }
        })
        .collect();
    recordings.sort_by(|a, b| a.id.cmp(&b.id));
    recordings
}

/// A network as the API shows it.
fn api_network(network: &crate::NetworkInfo) -> api::Network {
    api::Network {
        neurons: network
            .neurons
            .iter()
            .map(|n| api::Neuron {
                index: n.index,
                name: n.name.clone(),
                population: n.population.clone(),
                kind: n.kind.to_string(),
                position: n.position,
                inhibitory: n.inhibitory,
                cell: network.cells[n.index as usize],
            })
            .collect(),
        synapses: network
            .synapses
            .iter()
            .map(|s| api::Synapse {
                post: s.post,
                synapse: s.synapse,
                pre: s.pre,
                weight: s.weight,
                inhibitory: s.inhibitory,
            })
            .collect(),
        synapse_count: network.synapse_count,
        modulators: network.modulators.clone(),
        learning_rules: network.learning_rules.clone(),
        sets: network
            .sets
            .iter()
            .map(|(name, members)| api::NeuronSet {
                name: name.clone(),
                members: members.clone(),
            })
            .collect(),
    }
}
