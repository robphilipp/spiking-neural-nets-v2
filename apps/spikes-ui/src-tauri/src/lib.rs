//! The desktop app's backend: the session engine, in process, behind the same protocol the server speaks.
//!
//! A client calls `engine_connect` with two channels and gets a connection id; after that, JSON requests go in
//! through `engine_send`, sensor batches through `engine_send_binary` (a raw request body with the id in the
//! `connection` header), and responses, notices and frames come back over the channels (frames as raw bytes).
//! Several clients can be connected at once. A connection ends when its page stops listening (e.g. a reload),
//! which detaches its viewers.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};

use spikes_api::protocol::{Outcome, ServerMessage};
use spikes_session::{Connection, Engine, Outgoing};
use tauri::ipc::{Channel, InvokeBody, InvokeResponseBody, Request};
use tauri::{Manager, State};

enum Incoming {
    Text(String),
    Binary(Vec<u8>),
}

struct Desktop {
    engine: Arc<Engine>,
    recordings: PathBuf,
    /// Each connection's input, by id; removing one ends that connection.
    links: Arc<Mutex<HashMap<u32, Sender<Incoming>>>>,
    next: std::sync::atomic::AtomicU32,
}

impl Desktop {
    fn send(&self, connection: u32, message: Incoming) -> Result<(), String> {
        let links = self.links.lock().unwrap_or_else(|p| p.into_inner());
        let sender = links
            .get(&connection)
            .ok_or("no such connection: call engine_connect first")?;
        sender
            .send(message)
            .map_err(|_| "the engine connection has ended".to_string())
    }
}

#[tauri::command]
fn engine_connect(state: State<'_, Desktop>, messages: Channel<String>, frames: Channel<InvokeResponseBody>) -> u32 {
    let id = state.next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let (connection, outgoing) = Connection::new(Arc::clone(&state.engine), &state.recordings);
    let (tx, rx) = channel::<Incoming>();
    let errors = messages.clone();
    std::thread::spawn(move || {
        let mut connection = connection;
        for message in rx {
            match message {
                Incoming::Text(text) => connection.handle_text(&text),
                Incoming::Binary(bytes) => {
                    if let Err(e) = connection.handle_binary(&bytes) {
                        let error = ServerMessage::Response {
                            id: 0,
                            result: Outcome::Error(e),
                        };
                        if let Ok(text) = serde_json::to_string(&error) {
                            let _ = errors.send(text);
                        }
                    }
                }
            }
        }
    });
    let links = Arc::clone(&state.links);
    std::thread::spawn(move || {
        for out in outgoing {
            let sent = match out {
                Outgoing::Message(m) => serde_json::to_string(&m).map_or(Ok(()), |text| messages.send(text)),
                Outgoing::Frame(bytes) => frames.send(InvokeResponseBody::Raw(bytes)),
            };
            if sent.is_err() {
                break; // the page has gone
            }
        }
        // End the connection: its input closes, so its thread drops the Connection and detaches its viewers.
        links.lock().unwrap_or_else(|p| p.into_inner()).remove(&id);
    });
    state.links.lock().unwrap_or_else(|p| p.into_inner()).insert(id, tx);
    id
}

#[tauri::command]
fn engine_send(state: State<'_, Desktop>, connection: u32, text: String) -> Result<(), String> {
    state.send(connection, Incoming::Text(text))
}

#[tauri::command]
fn engine_send_binary(state: State<'_, Desktop>, request: Request<'_>) -> Result<(), String> {
    let connection = request
        .headers()
        .get("connection")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .ok_or("the connection header is missing")?;
    match request.body() {
        InvokeBody::Raw(bytes) => state.send(connection, Incoming::Binary(bytes.clone())),
        InvokeBody::Json(_) => Err("send sensor batches as raw bytes".into()),
    }
}

/// Whether the app runs in self-test mode (`SPIKES_SELFTEST=1`): the page then runs a scripted session through
/// the real IPC path (see `src/selftest.ts`).
#[tauri::command]
fn selftest_enabled() -> bool {
    std::env::var_os("SPIKES_SELFTEST").is_some()
}

/// Self-test mode: the page reports its result here; the app prints it and exits.
#[tauri::command]
fn selftest_report(app: tauri::AppHandle, ok: bool, report: String) {
    println!("SELFTEST {} {report}", if ok { "PASS" } else { "FAIL" });
    app.exit(i32::from(!ok));
}

/// Starts the app.
///
/// # Panics
/// If the window system can't be initialised.
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let recordings = app
                .path()
                .app_data_dir()
                .map(|dir| dir.join("recordings"))
                .unwrap_or_else(|_| PathBuf::from("recordings"));
            std::fs::create_dir_all(&recordings)?;
            app.manage(Desktop {
                engine: Arc::new(Engine::new()),
                recordings,
                links: Arc::default(),
                next: std::sync::atomic::AtomicU32::new(0),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            engine_connect,
            engine_send,
            engine_send_binary,
            selftest_enabled,
            selftest_report
        ])
        .run(tauri::generate_context!())
        .expect("error while running the Spikes app");
}
