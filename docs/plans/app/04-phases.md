# 04 Phases

Each phase ends with something runnable and its acceptance checks automated where possible.

## A1: session engine and CLI (1–2 weeks)

- `spikes-session`: `Engine`, `Session` state machine, run thread, commands through `Control`, recording on/off,
  dense neuron indices, `QueryNeuron`, connect/prune, input definitions (Poisson, periodic, scripted pulses,
  signal-file replay).
- The live-frame aggregator with level of detail, producing frames in the `spikes-api` layout (Rust decoder for
  tests).
- `apps/spikes-cli`: `check`, `run` (paced or as fast as possible, `--record`), `parquet`.
- **Accept**:
  - every v1 test network builds and runs from the CLI; recordings load in Python;
  - a paced 100k-neuron run streams frames at 60 fps with each frame ≤ its budget and frame production ≤ 2 ms;
  - a viewer that never reads loses frames but the run stays on schedule (lag unchanged within noise);
  - state-machine misuse returns errors, never panics (property test over random command sequences).

## A2: protocol, server, TS client (1–2 weeks)

- `spikes-api` types with `ts-rs` generation (checked in CI); frame encoder/decoder in Rust and TS with a shared
  golden-file test.
- `apps/spikes-server` (axum): REST endpoints, WebSocket sessions, multiple viewers, token auth, static UI hosting.
- `packages/spikes-client` (TS): `WebSocketClient`, frame views, reconnect.
- **Accept**:
  - a TS integration test (Vitest + a spawned server) creates, builds, runs, pauses, steps, stimulates, queries
    and stops a session, and checks frames against the recording;
  - two clients watch one session; one disconnecting doesn't affect the other or the run;
  - round trip for a control request < 5 ms locally.

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
