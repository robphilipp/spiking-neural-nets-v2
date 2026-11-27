//! A small, blocking client for sensor programs: connect to a `spikes-server`, find the neurons to drive, and
//! stream sample batches into a running session (app plan 01 §5).
//!
//! ```no_run
//! use spikes_api::protocol::{Encoding, SensorSpec};
//! use spikes_sensor::SensorClient;
//!
//! let mut client = SensorClient::connect("ws://127.0.0.1:7070", None)?;
//! let session = 0;
//! let targets = client.neurons_named(session, "cochlea-")?;
//! let sensor = client.connect_sensor(session, SensorSpec {
//!     name: "microphone".into(),
//!     targets: targets.into_iter().map(|n| vec![n]).collect(),
//!     encoding: Encoding::Rate { max_hz: 200.0, mv: 3.0 },
//!     ingress_delay_ms: None,
//!     buffer_while_paused: false,
//! })?;
//! // Every 10 ms: 10 samples (1 ms apart) of every channel, sample-major.
//! # let (t0_ns, values) = (0, vec![0.0f32; 640]);
//! client.send_samples(sensor, t0_ns, 1_000_000, 64, &values)?;
//! # Ok::<(), spikes_sensor::SensorError>(())
//! ```

use std::io::ErrorKind;
use std::net::TcpStream;
use std::time::{Duration, Instant};

use spikes_api::input::InputBatch;
use spikes_api::protocol::{ApiError, ClientMessage, Outcome, Request, Response, SensorSpec, ServerMessage};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

/// What went wrong.
#[derive(Debug)]
pub enum SensorError {
    /// Couldn't connect, or the connection broke.
    Connection(String),
    /// The engine refused a request or a batch.
    Api(ApiError),
    /// The engine didn't answer in time.
    Timeout,
    /// An unexpected answer.
    Protocol(String),
}

impl std::fmt::Display for SensorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Connection(why) => write!(f, "connection: {why}"),
            Self::Api(e) => write!(f, "{:?}: {}", e.code, e.message),
            Self::Timeout => f.write_str("the engine didn't answer in time"),
            Self::Protocol(why) => write!(f, "protocol: {why}"),
        }
    }
}

impl std::error::Error for SensorError {}

impl From<tungstenite::Error> for SensorError {
    fn from(e: tungstenite::Error) -> Self {
        Self::Connection(e.to_string())
    }
}

/// Results of sensor operations.
pub type Result<T> = std::result::Result<T, SensorError>;

/// How long a request waits for its answer.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// A connection to a `spikes-server`, for sending sensor data.
#[derive(Debug)]
pub struct SensorClient {
    socket: WebSocket<MaybeTlsStream<TcpStream>>,
    next_id: u32,
    seq: u32,
}

impl SensorClient {
    /// Connects to a server (`ws://host:port` or `http://host:port`).
    pub fn connect(server: &str, token: Option<&str>) -> Result<Self> {
        let base = server.trim_end_matches('/').replacen("http", "ws", 1);
        let mut url = format!("{base}/api/ws?v={}", spikes_api::PROTOCOL_VERSION);
        if let Some(token) = token {
            url.push_str("&token=");
            url.push_str(token);
        }
        let (socket, _) = tungstenite::connect(url.as_str())?;
        Ok(Self {
            socket,
            next_id: 1,
            seq: 0,
        })
    }

    fn set_read_timeout(&mut self, timeout: Option<Duration>) {
        if let MaybeTlsStream::Plain(stream) = self.socket.get_mut() {
            let _ = stream.set_read_timeout(timeout);
        }
    }

    /// The next message, or `None` if none arrives within the read timeout.
    fn read(&mut self) -> Result<Option<Message>> {
        match self.socket.read() {
            Ok(message) => Ok(Some(message)),
            Err(tungstenite::Error::Io(e)) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                Ok(None)
            }
            Err(e) => Err(e.into()),
        }
    }

    /// A message from the engine that isn't the answer we're waiting for: an error without a request is reported;
    /// notices and frames are ignored.
    fn other(message: &Message) -> Result<()> {
        if let Message::Text(text) = message
            && let Ok(ServerMessage::Response {
                id: 0,
                result: Outcome::Error(e),
            }) = serde_json::from_str(text)
        {
            return Err(SensorError::Api(e));
        }
        Ok(())
    }

    /// Sends a request and waits for its response.
    pub fn request(&mut self, request: Request) -> Result<Response> {
        let id = self.next_id;
        self.next_id += 1;
        let text =
            serde_json::to_string(&ClientMessage { id, request }).map_err(|e| SensorError::Protocol(e.to_string()))?;
        self.socket.send(Message::Text(text.into()))?;
        self.set_read_timeout(Some(Duration::from_millis(100)));
        let deadline = Instant::now() + REQUEST_TIMEOUT;
        while Instant::now() < deadline {
            let Some(message) = self.read()? else { continue };
            if let Message::Text(text) = &message
                && let Ok(ServerMessage::Response { id: got, result }) = serde_json::from_str(text)
                && got == id
            {
                return match result {
                    Outcome::Ok(response) => Ok(response),
                    Outcome::Error(e) => Err(SensorError::Api(e)),
                };
            }
            Self::other(&message)?;
        }
        Err(SensorError::Timeout)
    }

    /// The indices of the neurons whose names start with `prefix`, in index order.
    pub fn neurons_named(&mut self, session: u32, prefix: &str) -> Result<Vec<u32>> {
        match self.request(Request::GetNetwork { session })? {
            Response::Network { network } => Ok(network
                .neurons
                .iter()
                .filter(|n| n.name.starts_with(prefix))
                .map(|n| n.index)
                .collect()),
            other => Err(SensorError::Protocol(format!("expected a network, got {other:?}"))),
        }
    }

    /// Connects a sensor to a session; returns its id for [`Self::send_samples`].
    pub fn connect_sensor(&mut self, session: u32, spec: SensorSpec) -> Result<u32> {
        match self.request(Request::ConnectSensor { session, sensor: spec })? {
            Response::Sensor { sensor } => Ok(sensor),
            other => Err(SensorError::Protocol(format!("expected a sensor, got {other:?}"))),
        }
    }

    /// Takes over the sensor called `name` that the UI (or another program) declared for this session, such as a
    /// `sensor` source of an input file; returns its id for [`Self::send_samples`]. Its channels are the ones it
    /// was declared with.
    pub fn claim_sensor(&mut self, session: u32, name: &str) -> Result<u32> {
        match self.request(Request::ClaimSensor {
            session,
            name: name.into(),
        })? {
            Response::Sensor { sensor } => Ok(sensor),
            other => Err(SensorError::Protocol(format!("expected a sensor, got {other:?}"))),
        }
    }

    /// Sends samples on all channels, `dt_ns` apart, sample-major (`values[s × channels + c]`). `t0_ns` is the
    /// sensor's own clock at the first sample; keep it increasing from batch to batch. Doesn't wait for an
    /// answer, but reports an error the engine sent about an earlier batch.
    pub fn send_samples(&mut self, sensor: u32, t0_ns: u64, dt_ns: u32, channels: u16, values: &[f32]) -> Result<()> {
        if channels == 0 || !values.len().is_multiple_of(usize::from(channels)) {
            return Err(SensorError::Protocol(format!(
                "{} values aren't whole samples of {channels} channels",
                values.len()
            )));
        }
        let batch = InputBatch::Samples {
            sensor,
            seq: self.seq,
            t0_ns,
            dt_ns,
            channels,
            values: values.to_vec(),
        };
        self.seq = self.seq.wrapping_add(1);
        self.socket.send(Message::Binary(batch.encode().into()))?;
        self.drain()
    }

    /// Sends events (channel, offset in ns from `t0_ns`) for a sensor with spike encoding.
    pub fn send_events(&mut self, sensor: u32, t0_ns: u64, events: &[(u16, u32)]) -> Result<()> {
        let batch = InputBatch::Events {
            sensor,
            seq: self.seq,
            t0_ns,
            events: events.to_vec(),
        };
        self.seq = self.seq.wrapping_add(1);
        self.socket.send(Message::Binary(batch.encode().into()))?;
        self.drain()
    }

    /// Reads whatever has arrived (notices), without waiting, so it doesn't pile up.
    fn drain(&mut self) -> Result<()> {
        self.set_read_timeout(Some(Duration::from_micros(1)));
        while let Some(message) = self.read()? {
            Self::other(&message)?;
        }
        Ok(())
    }

    /// Closes the connection; the engine disconnects its sensors.
    pub fn close(mut self) -> Result<()> {
        self.socket.close(None)?;
        Ok(())
    }
}
