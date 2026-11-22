# 01 Architecture

## 1. Crates and apps

| Path | Kind | Depends on | Contents |
|------|------|-----------|----------|
| `crates/spikes-session` | lib | `spikes-network`, `spikes-record`, `spikes-dna`, `tempo-runtime` | `Engine` (many sessions), `Session` (one network: state machine, commands, clock, recording, live frames), input actors that need files (signal replay), the CLI's logic |
| `crates/spikes-api` | lib | `serde`, `ts-rs` | Protocol types only: `Request`, `Response`, `Notice`, frame header and layout constants, error codes. No `tempo` types leak out (times are `u64` ns, neurons are dense `u32` indices). |
| `apps/spikes-cli` | bin | `spikes-session` | `spikes check net.boo`, `spikes run net.boo --for 10s --speed 1 --record runs/x`, `spikes parquet runs/x` |
| `apps/spikes-server` | bin | `spikes-session`, `spikes-api`, `axum`, `tokio` | HTTP + WebSocket adapter; static hosting of the built UI; later the cluster coordinator (actors phase 4) |
| `apps/spikes-ui` | Tauri v2 app | `spikes-session`, `spikes-api`, `tauri` | `src-tauri/`: the IPC adapter; `src/`: the TypeScript UI (also built as a plain web app for the server) |

Only the adapters use tokio; the engine stays thread-based like the runtime (actors decision "no tokio in the
runtime"). Adapters await the runtime's `Reply` futures directly.

## 2. The session engine

```rust
let engine = Engine::new(EngineConfig { recordings_dir, max_sessions, .. });
let id = engine.create(CreateSession { name, description: boo_text, inputs, options })?; // parse errors → line/col
let session = engine.session(id)?;
session.build(BuildSettings { seed, shards, jitter, record_weights })?;
let viewer = session.attach(ViewSettings { frame_ms: 16, budget_bytes: .., membrane: vec![..], .. });
session.start(RunSettings { speed: 1.0, until: None, record: true })?;
session.set_speed(0.25); session.pause(); session.step(ms(5)); session.resume();
session.stimulate(neurons, mv); session.release_modulator(site, kind, amount);
session.connect(pre, post, weight); session.prune(pre, post);
let state = session.query_neuron(n)?;   // potential, thresholds, synapses, modulators at its position
session.stop()?;                        // finishes the recording, converts to Parquet in the background
```

**Lifecycle**: `Loaded` (parsed; can be edited and re-parsed) → `Built` (actors exist; positions and synapses are
queryable for the 3D view) → `Running` ⇄ `Paused` → `Finished` (run reached `until` or was stopped). `reset`
rebuilds from the description with the same or a new seed. Invalid transitions return a typed error.

**Threads per session**:
- the **run thread** owns the `Runtime` and calls `run_until`; it stops at window boundaries for commands that
  need `&mut Runtime` (connect/prune at build time, observe changes before start);
- everything else during a run goes through the runtime's `Control` handle (clock, inject, query, update,
  change_route, observe) from any thread;
- the **recorder thread** (`spikes-record`, lossless subscription);
- one **aggregator thread** per session (below), shared by all viewers of that session.

**Name ↔ index**: the engine numbers neurons densely in description order (`u32`) and keeps the actor-id map, so
the protocol and the UI never see actor ids. Synapses are `(post index, synapse index)`.

## 3. The live-frame aggregator

Input: a bounded runtime subscription (lossy; drops are counted) plus system events. Output: one frame per viewer
per display interval.

Per frame it builds, for each viewer:
- **Spikes** in the interval, as `(neuron u32, offset u32 ns from frame start)`.
- **Membrane** samples only for that viewer's selected neurons (every update, as recorded; capped per neuron per
  frame by decimation that keeps extrema).
- **Weights**: the latest value of each synapse that changed in the interval (coalesced), optionally only for a
  selected neuron set.
- **Transitions** and **modulator releases**.
- **Stats**: sim time, wall time, speed, lag behind the clock, lookahead, events/s, frames and events dropped.

**Level of detail**, chosen per frame against the viewer's byte budget (default 4 MB/s local, 1 MB/s remote):
1. *exact*: every spike;
2. *counts*: spikes per neuron in the frame (`u16`, only neurons that spiked);
3. *binned*: spike counts per spatial cell (the runtime's spatial grid) or per group.
The frame says which level it used. The raster view shows exact spikes when it gets them and density otherwise;
the 3D view only ever needs counts.

Viewers with a selection (zoomed raster, a 3D frustum, an inspected neuron) get filtered frames, so zooming in
restores exact data for what is on screen.

Slow viewers: each viewer has a queue of at most 3 frames; older frames are dropped and counted. Dropping a frame
loses data only for that viewer.

## 4. Adapters

**WebSocket (`spikes-server`)**
- `GET /api/health`, `GET /api/sessions`, `POST /api/sessions` (create from `.boo` text), `GET
  /api/recordings`, `GET /api/recordings/{id}/{table}.parquet` (download), `GET /` (the UI).
- `GET /api/sessions/{id}/ws`: upgrade. Text messages are JSON `Request`/`Response`/`Notice`; binary messages are
  frames. Several clients can watch one session; control commands from any of them apply (last write wins, every
  client sees the resulting notices).
- Binds `127.0.0.1` by default. Remote use: `--listen 0.0.0.0:7070 --token-file …` (bearer token on HTTP and in
  the WS upgrade), TLS by a reverse proxy or `--tls-cert/--tls-key`.

**Tauri (`apps/spikes-ui/src-tauri`)**
- `request(json) -> json` command (same `Request`/`Response` types).
- `attach(session, view, channel: Channel)`: frames are sent as raw bytes (`InvokeResponseBody::Raw`), notices as
  JSON on a second channel.
- File dialogs and project files through Tauri's fs/dialog plugins, scoped to the project directory.
- The engine runs in-process with all cores available; the window can also connect to a remote server instead
  (settings → "Run on: this computer | server …").

## 5. Projects and storage

```
my-project/
  project.toml          # name, network file, inputs, default run settings (seed, shards, speed, duration)
  networks/*.boo
  inputs/*.toml         # input actors: Poisson, periodic, scripted pulses, signal files
  signals/*.arrow       # recorded or designed signals (t_ns, neuron, mv)
  runs/<date>-<name>/   # recordings when running locally (spikes-record layout + Parquet)
```

v1's `.spikes` project files are imported (network path, sensor file, time factor, duration). Remote runs keep
their recordings on the server under `recordings_dir/<session>/`; the UI lists and downloads them.

## 6. Failure handling

- Parse and build errors come back as structured errors with spans, shown inline in the editor.
- A panicking actor produces an `ActorFailed` notice; the run continues (runtime panic isolation).
- A panicking session thread marks the session `Failed` with the message; other sessions are unaffected.
- Recording errors (disk full) stop recording, not the run, and raise a notice.
- If the server process dies, the UI shows "disconnected" and reconnects; sessions are not persisted across
  restarts in this plan (checkpointing comes with actors phase 4).
