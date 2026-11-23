# 04 Phases

Each phase ends with something runnable and its acceptance checks automated where possible.

## A1: session engine and CLI (1–2 weeks)

- `spikes-session`: `Engine`, `Session` state machine, run thread, commands through `Control`, recording on/off,
  dense neuron indices, `QueryNeuron`, connect/prune, input definitions (Poisson, periodic, scripted pulses,
  signal-file replay).
- The live-frame aggregator with level of detail, producing frames in the `spikes-api` layout (Rust decoder for
  tests).
- Sensor ingestion: clock anchoring, the four encodings, late-sample handling, recording of sensor data; an
  in-process sensor API (used by tests and the CLI) that the network adapters later wrap.
- `apps/spikes-cli`: `check`, `run` (paced or as fast as possible, `--record`), `parquet`.
- **Accept**:
  - every v1 test network builds and runs from the CLI; recordings load in Python;
  - a paced 100k-neuron run streams frames at 60 fps with each frame ≤ its budget and frame production ≤ 2 ms;
  - a viewer that never reads loses frames but the run stays on schedule (lag unchanged within noise);
  - state-machine misuse returns errors, never panics (property test over random command sequences);
  - a synthetic sensor sending jittered batches (±5 ms) produces inputs whose spacing matches the sample spacing
    exactly; replaying the recorded sensor data reproduces the run's spikes.

### A1 status: done (2026-11-22)

Crates: `spikes-session` (engine), `spikes-api` (frame format so far), `apps/spikes-cli` (`spikes check | run |
parquet`).

Acceptance results:
- All 26 v1 test networks build and run from the CLI (2 shards, Poisson input, membrane observed, recorded, Parquet);
  every table of every run loads in pyarrow, Parquet spikes sorted by time.
- **100k neurons, 1M synapses, ~5 Hz (≈ 490k spikes/s), paced at real time on 4 shards** (`examples/stream_100k`):
  10 s simulated in 10.01 s, max lag 9 ms; two viewers (whole network at the default 4 MB/s budget, a 300-neuron
  selection) at 62.5 frames/s, p99 gap 18 ms, no dropped frames or events; whole-network frames exact in 624 of 625
  (one fell back to counts) at 3.9 MB/s; encoding 13 µs per frame on average, 0.84 ms at worst (target ≤ 2 ms);
  routing ~100 ns per event.
- A slow viewer loses frames, not time (test); jittered sensor batches keep exact sample spacing; replaying recorded
  sensor inputs reproduces a run's spikes exactly.
- Random command sequences (property test, 2 × 2000 under a watchdog) neither panic nor hang.

Bugs the tests found and fixed:
- A `stop()` racing the run thread's start was lost, leaving a paused run that never ended
  (`StopToken` / `Runtime::run_until_unless_stopped`).
- Stopping a paused run left the clock paused: a later `step()` waited for ever holding the session; a later paced
  start began paused.
- A query reaching the control plane as a run ended waited for the next run (5 s timeout); it now falls back to
  reading the idle runtime.

Decisions and deviations:
- **Default minimum connection delay 100 µs** (`DEFAULT_MIN_DELAY`, CLI `--min-delay`). With raw `distance/speed`
  delays, neurons a few µm apart gave µs synchronization windows and *negative* shard scaling (100k network:
  1 shard 2.3 s, 4 shards 12 s, 8 shards 47 s per simulated second). With 100 µs: 2.3 / 1.3 / 0.78 / 0.94 s on
  1 / 2 / 4 / 8 shards; with 500 µs 8 shards reach 0.49 s. Chemical synapses take ≥ ~0.3 ms, so this costs little
  realism; set it to 0 for raw delays.
- **Synaptic delay** (decided 2026-11-22, after the above): connection delays are now `syd + distance / csp`, with
  `syd` (default 0.5 ms) on neurons and connections. This is the biological fix the 100 µs floor only approximated:
  every connection gets a realistic transmission time, and the shortest delay (hence the synchronization window)
  becomes ~0.5 ms. 100k network: 2.39 / 0.80 / 0.68 s per simulated second on 1 / 4 / 8 shards. The 100 µs floor
  stays as a safety net for networks that set `syd=0`.
- Lifecycle states are `Loaded → Ready → Running ⇄ Paused → Ready` (no separate `Finished`).
- Frames carry `time_unit_ns` so offsets (u32) cover frames spanning more than 4.3 s of simulated time (runs as fast
  as possible).
- Sensor encoding runs on the caller's thread (the connection delivering a batch), not a sensor thread per session;
  clock drift is handled by re-anchoring when a batch would be late.
- Not in A1: connect/prune from the session (neuron actors can't grow synapses yet; moved to A4), membrane
  decimation per viewer, project files (`project.toml`).
- Next performance target: one shard spends ~430 ns per event on the full neuron model (release probability,
  facilitation/depletion, STDP traces, noise); profiling it is worth doing before scaling work.

## A2: protocol, server, TS client (1–2 weeks)

- `spikes-api` types with `ts-rs` generation (checked in CI); frame encoder/decoder in Rust and TS with a shared
  golden-file test.
- `apps/spikes-server` (axum): REST endpoints, WebSocket sessions, multiple viewers, token auth, static UI hosting.
- `packages/spikes-client` (TS): `WebSocketClient`, frame views, reconnect.
- Sensor clients: `crates/spikes-sensor` (Rust) and `clients/python/spikes_sensor`, with an example that streams a
  microphone's filterbank envelopes (Python, `sounddevice` + `numpy`) into a session on the server.
- **Accept**:
  - a TS integration test (Vitest + a spawned server) creates, builds, runs, pauses, steps, stimulates, queries
    and stops a session, and checks frames against the recording;
  - two clients watch one session; one disconnecting doesn't affect the other or the run;
  - round trip for a control request < 5 ms locally;
  - the microphone example drives a cochlea-like input layer in real time, from another machine on the LAN.

### A2 status: done (2026-11-22)

- `spikes-api`: JSON protocol types with TypeScript generated by `ts-rs` (`packages/spikes-client/src/generated`,
  refreshed by `cargo test -p spikes-api`); binary sensor input batches (`SPKS`); frames now carry the viewer id.
- `spikes-session::Connection`: the transport-free protocol endpoint (JSON requests and binary batches in; responses,
  notices and frames out). The server wraps it; the Tauri app will too.
- `apps/spikes-server` (axum): `/api/health`, `/api/ws?v=1`, `/api/recordings` (+ downloads, path-checked), static UI;
  `127.0.0.1` by default, `--token-file` for a bearer token (header or `?token=`); frames dropped for slow clients,
  responses and notices never.
- `packages/spikes-client` (TS): `ProtocolClient` over an abstract transport, `WebSocketClient` with reconnect,
  zero-copy frame decoder, sensor batch encoder.
- Sensor clients: `crates/spikes-sensor` (Rust, blocking) with `examples/tones.rs`; `clients/python/spikes_sensor`
  with `examples/microphone.py` (16 kHz → 64 log bands at 1 ms → rate-coded input; `--synthetic` chirp).

Acceptance:
- Golden files written by Rust; the TS decoder/encoder and the Python encoder match them byte for byte.
- TS integration tests against a real server (5/5 runs): full lifecycle with a recorded run downloaded over HTTP,
  pause/step/resume/speed/stop, parse errors with positions, median request round trip < 5 ms, two clients on one
  session (one leaving doesn't disturb the other or the run), sensors, tokens.
- Python tests against a real server; the microphone pipeline on a synthetic chirp drives all 64 bands.
- **Not verified here:** a real microphone (macOS asks for permission interactively) and a sensor on another
  machine on the LAN. Both need a person at the keyboard: `spikes-server --listen 0.0.0.0:7070 --token-file …`,
  then `python clients/python/examples/microphone.py --server ws://<host>:7070 --token …`.

## A3: Tauri app shell (2 weeks)

- `apps/spikes-ui`: Tauri v2 + React + Vite; `TauriClient` (IPC + Channel) and `WebSocketClient` behind one
  interface; settings for local vs remote.
- Material UI shell and theme (light/dark), the theme → `stream-charts` colour adapter.
- Projects screen, Monaco editor with the ported `spikes` language and engine diagnostics, Run screen with the
  speed slider and timing-health panel, `stream-charts` raster of the selection plus population rates.
- **Accept**:
  - open a v1 project, edit the network with live diagnostics, run it paced, change speed, pause, step, and see
    the raster follow, in the desktop app and in a browser against `spikes-server`;
  - a 100k-neuron network at 5 Hz runs with the raster (a few hundred selected rows) and population rates
    scrolling smoothly at 60 fps on this machine (M-series Mac);
  - if `stream-charts` turns out to limit the raster, the fix goes into the library (it's ours) rather than a
    second charting library.

## A4: views and interaction (2–3 weeks)

- 3D network (instanced neurons, connections for the selection, spike flashes), shared selection, membrane traces,
  weights over time, neuron inspector, stimulus and modulator buttons, connect/prune from the UI.
- **Accept**: the STDP and reward-learning examples can be run and *seen* end to end (weights change in the
  chart after a reward press); selecting a region in 3D switches the raster from counts to exact spikes.

## A5: recordings, replay, inputs (1–2 weeks)

- Recordings screen, replay sessions with seeking, Parquet download; input editor and signal designer (the v1
  sensor-script idea, saved as signal files).
- **Accept**: replaying a recording shows the same raster and weights as the live run did (frame-by-frame
  comparison in a test); a designed signal drives a run reproducibly.

## A6: remote and cluster (with actors phase 4)

- The server becomes the coordinator for multi-host runs: sessions span nodes, frames are aggregated per node and
  merged at the coordinator (counts and bins merge cheaply), recordings are partitioned datasets.
- Packaging: signed desktop builds (macOS first), a server container image.
- **Accept**: the UI controls and watches a 2–4 host run with the same screens; lateness per link shows in the
  timing-health panel.

## Dependencies to add (when needed)

`axum`, `tokio`, `tower-http` (static files, CORS), `ts-rs`, `serde_json` in the server/api; `tauri` v2 with the
`dialog` and `fs` plugins in the app; on the TS side `react`/`react-dom` 19, `@mui/material` 9 with
`@mui/icons-material` and `@emotion/react`/`@emotion/styled`, `stream-charts` 2.1 (with its peers `d3` 7 and
`rxjs` 7), `@reduxjs/toolkit`, `monaco-editor`, `three`, `vitest`, `playwright`.
