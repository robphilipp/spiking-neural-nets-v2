//! `spikes-server`: the session engine over HTTP and WebSocket (app plan 01 §4).
//!
//! ```text
//! spikes-server [--listen 127.0.0.1:7070] [--recordings ./recordings] [--ui apps/spikes-ui/dist] [--token-file F]
//! ```
//!
//! - `GET /api/health`: `{"ok": true, "version": …, "protocol": 1}`
//! - `GET /api/ws?v=1[&token=…]`: the protocol. Text messages are JSON `ClientMessage` / `ServerMessage`; binary
//!   messages are frames (server → client) and sensor input batches (client → server).
//! - `GET /api/recordings`: recorded runs; `GET /api/recordings/{id}/{file}`: download a table.
//! - `GET /…`: the UI, if `--ui` is given.
//!
//! With `--token-file`, every request needs `Authorization: Bearer <token>` or `?token=<token>` (browsers can't
//! set headers on a WebSocket).

use std::net::SocketAddr;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path as UrlPath, Query, Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use clap::Parser;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use spikes_api::PROTOCOL_VERSION;
use spikes_api::protocol::{ApiError, ErrorCode, Outcome, ServerMessage};
use spikes_session::{Connection, Engine, Outgoing};
use tokio::sync::mpsc;
use tower_http::services::ServeDir;

#[derive(Parser)]
#[command(
    name = "spikes-server",
    version,
    about = "Runs spiking networks and serves them over WebSocket"
)]
struct Args {
    /// Address to listen on. The default only accepts connections from this machine.
    #[arg(long, default_value = "127.0.0.1:7070")]
    listen: SocketAddr,
    /// Where recorded runs go.
    #[arg(long, default_value = "recordings")]
    recordings: PathBuf,
    /// The built UI to serve at `/`.
    #[arg(long)]
    ui: Option<PathBuf>,
    /// A file holding the access token; without it, no token is needed.
    #[arg(long)]
    token_file: Option<PathBuf>,
    /// Serve as a node agent on `--listen` instead: other spikes-servers spread their networks over this machine's
    /// cores (build with `nodes`). Only for a trusted network: agents take no token.
    #[arg(long)]
    node: bool,
}

struct AppState {
    engine: Arc<Engine>,
    recordings: PathBuf,
    token: Option<String>,
}

/// Frames a client may have queued before newer ones are dropped (responses and notices are never dropped).
const FRAME_QUEUE: usize = 16;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    if args.node {
        let listener = std::net::TcpListener::bind(args.listen)?;
        println!("spikes node agent on {}", listener.local_addr()?);
        tokio::task::spawn_blocking(move || spikes_session::cluster::serve_node(listener)).await??;
        return Ok(());
    }
    let token = match &args.token_file {
        Some(path) => Some(std::fs::read_to_string(path)?.trim().to_string()),
        None => None,
    };
    if token.is_none() && !args.listen.ip().is_loopback() {
        eprintln!(
            "warning: listening on {} without --token-file: anyone who can reach it can run code",
            args.listen
        );
    }
    std::fs::create_dir_all(&args.recordings)?;
    let state = Arc::new(AppState {
        engine: Arc::new(Engine::new()),
        recordings: args.recordings.clone(),
        token,
    });
    let listener = tokio::net::TcpListener::bind(args.listen).await?;
    println!("spikes-server listening on http://{}", listener.local_addr()?);
    axum::serve(listener, router(state, args.ui.as_deref()))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

fn router(state: Arc<AppState>, ui: Option<&Path>) -> Router {
    let api = Router::new()
        .route("/api/health", get(health))
        .route("/api/ws", get(websocket))
        .route("/api/recordings", get(recordings))
        .route("/api/recordings/{id}/{file}", get(recording_file))
        .layer(middleware::from_fn_with_state(Arc::clone(&state), authorize))
        .with_state(state);
    match ui {
        Some(dir) => api.fallback_service(ServeDir::new(dir)),
        None => api,
    }
}

#[derive(Deserialize)]
struct Params {
    token: Option<String>,
    v: Option<u16>,
}

async fn authorize(
    State(state): State<Arc<AppState>>,
    Query(params): Query<Params>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Response {
    if let Some(expected) = &state.token {
        let bearer = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));
        if bearer.or(params.token.as_deref()) != Some(expected.as_str()) {
            return (StatusCode::UNAUTHORIZED, "a valid token is needed").into_response();
        }
    }
    next.run(request).await
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "ok": true,
        "version": env!("CARGO_PKG_VERSION"),
        "protocol": PROTOCOL_VERSION,
    }))
}

async fn recordings(State(state): State<Arc<AppState>>) -> Response {
    let (mut connection, _out) = Connection::new(Arc::clone(&state.engine), &state.recordings);
    let message = connection.handle(spikes_api::protocol::ClientMessage {
        id: 0,
        request: spikes_api::protocol::Request::ListRecordings,
    });
    match message {
        ServerMessage::Response {
            result: Outcome::Ok(response),
            ..
        } => Json(response).into_response(),
        other => (StatusCode::INTERNAL_SERVER_ERROR, Json(other)).into_response(),
    }
}

/// One path segment, no tricks.
fn plain(segment: &str) -> bool {
    let mut components = Path::new(segment).components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}

async fn recording_file(
    State(state): State<Arc<AppState>>,
    UrlPath((id, file)): UrlPath<(String, String)>,
) -> Response {
    if !plain(&id) || !plain(&file) {
        return (StatusCode::BAD_REQUEST, "bad path").into_response();
    }
    let path = state.recordings.join(id).join(&file);
    match tokio::fs::read(&path).await {
        Ok(bytes) => {
            let disposition = format!("attachment; filename=\"{file}\"");
            (
                [
                    (header::CONTENT_TYPE, "application/octet-stream".to_string()),
                    (header::CONTENT_DISPOSITION, disposition),
                ],
                bytes,
            )
                .into_response()
        }
        Err(_) => (StatusCode::NOT_FOUND, "no such file").into_response(),
    }
}

async fn websocket(
    State(state): State<Arc<AppState>>,
    Query(params): Query<Params>,
    upgrade: WebSocketUpgrade,
) -> Response {
    if let Some(v) = params.v
        && v != PROTOCOL_VERSION
    {
        return (
            StatusCode::BAD_REQUEST,
            format!("this server speaks protocol version {PROTOCOL_VERSION}, not {v}"),
        )
            .into_response();
    }
    upgrade.on_upgrade(move |socket| serve(socket, state))
}

enum Incoming {
    Text(String),
    Binary(Vec<u8>),
}

/// Bridges one WebSocket to one engine `Connection`, which runs on a thread of its own (its calls block).
async fn serve(socket: WebSocket, state: Arc<AppState>) {
    let (connection, outgoing) = Connection::new(Arc::clone(&state.engine), &state.recordings);
    let (incoming_tx, incoming_rx) = std::sync::mpsc::channel::<Incoming>();
    let (messages_tx, mut messages_rx) = mpsc::unbounded_channel::<String>();
    let (frames_tx, mut frames_rx) = mpsc::channel::<Vec<u8>>(FRAME_QUEUE);

    // The connection: requests in order, on its own thread.
    let errors = messages_tx.clone();
    std::thread::spawn(move || {
        let mut connection = connection;
        for message in incoming_rx {
            match message {
                Incoming::Text(text) => connection.handle_text(&text),
                Incoming::Binary(bytes) => {
                    if let Err(e) = connection.handle_binary(&bytes) {
                        let _ = errors.send(error_message(e));
                    }
                }
            }
        }
    });
    // Its output: messages always, frames only while the client keeps up.
    std::thread::spawn(move || {
        for out in outgoing {
            let sent = match out {
                Outgoing::Message(m) => match serde_json::to_string(&m) {
                    Ok(text) => messages_tx.send(text).is_ok(),
                    Err(_) => true,
                },
                Outgoing::Frame(frame) => {
                    !matches!(frames_tx.try_send(frame), Err(mpsc::error::TrySendError::Closed(_)))
                }
            };
            if !sent {
                return;
            }
        }
    });

    let (mut sink, mut stream) = socket.split();
    let writer = tokio::spawn(async move {
        loop {
            let message = tokio::select! {
                biased;
                Some(text) = messages_rx.recv() => Message::Text(text.into()),
                Some(frame) = frames_rx.recv() => Message::Binary(frame.into()),
                else => break,
            };
            if sink.send(message).await.is_err() {
                break;
            }
        }
    });
    while let Some(Ok(message)) = stream.next().await {
        let forwarded = match message {
            Message::Text(text) => incoming_tx.send(Incoming::Text(text.to_string())),
            Message::Binary(bytes) => incoming_tx.send(Incoming::Binary(bytes.to_vec())),
            Message::Close(_) => break,
            Message::Ping(_) | Message::Pong(_) => Ok(()),
        };
        if forwarded.is_err() {
            break;
        }
    }
    drop(incoming_tx); // ends the connection thread, which drops the Connection and its viewers
    writer.abort();
}

fn error_message(e: ApiError) -> String {
    let message = ServerMessage::Response {
        id: 0,
        result: Outcome::Error(e),
    };
    serde_json::to_string(&message).unwrap_or_else(|_| {
        format!(
            "{{\"type\":\"response\",\"id\":0,\"result\":{{\"error\":{{\"code\":\"{:?}\"}}}}}}",
            ErrorCode::Internal
        )
    })
}
