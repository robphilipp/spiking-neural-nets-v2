# Plan: app layer (session engine, server, Tauri UI)

Status: **draft for review**, 2026-11-22. Builds on the actor framework (`docs/plans/actors/`, phases 1–3 done) and
the neuron crates (`docs/plans/neurons/`, N1–N4 done). Background on the v1 UI and server is in
`../../../notes/05-v1-ui.md` and `../../../notes/04-v1-runtime-architecture.md`; the web-layer recommendation this
plan expands is in `../../../notes/06-lessons-and-v2-implications.md`.

## The idea in one paragraph

Put everything a running simulation needs into a **session engine library** (`spikes-session`): parse, build, run
with the live clock, take commands, record to Arrow, and turn the event stream into **compact, binary,
per-frame batches** for viewers. Define one **protocol** (`spikes-api`: JSON requests, responses and notices, plus
binary frames) with TypeScript types generated from the Rust. Serve it two ways with thin adapters: **Tauri IPC**
(the desktop app embeds the engine, no server to start) and **WebSocket** (`spikes-server`, for remote machines,
browsers, and later the cluster coordinator). The **UI is one TypeScript code base** that talks to a `SpikesClient`
interface with two implementations. Kafka is not needed: the runtime's subscriptions are the live queue and the
Arrow/Parquet files are the durable one.

```
 tempo-runtime · spikes-model · spikes-dna · spikes-network · spikes-record      (done)
                                   │
                         spikes-session   engine: sessions, commands, clock, recording, live frames
                                   │
                         spikes-api       protocol types (serde) → generated TypeScript (ts-rs)
                 ┌─────────────────┴──────────────────┐
     apps/spikes-ui/src-tauri                 apps/spikes-server
     (Tauri commands + Channel<bytes>,        (axum: HTTP + WebSocket, serves the
      engine in-process)                       UI as a web app too; later: cluster coordinator)
                 └─────────────────┬──────────────────┘
                     apps/spikes-ui (TypeScript, React)
                     SpikesClient = TauriClient | WebSocketClient
```

## Documents

| Doc | Contents |
|-----|----------|
| [01-architecture.md](01-architecture.md) | Crates and apps, the session engine, threads, the live-frame aggregator, adapters, projects and storage, security |
| [02-protocol.md](02-protocol.md) | Requests, responses, notices, the binary frame format, bandwidth budget and level of detail |
| [03-ui.md](03-ui.md) | Screens, what is ported from v1, state, visualization techniques, the TS client |
| [04-phases.md](04-phases.md) | Milestones with deliverables and acceptance criteria |

## Key decisions (proposed)

1. **Library first.** All behaviour lives in `spikes-session`; the server and the Tauri backend are adapters of a
   few hundred lines each. A headless CLI (`spikes run`) uses the same engine.
2. **One protocol, two transports.** Control is JSON request/response with ids; live data is binary frames. Over
   WebSocket: text messages for JSON, binary messages for frames. Over Tauri: one `request` command and a
   `Channel` carrying raw frame bytes. The UI can't tell the difference.
3. **No Kafka.** Live data: the runtime's bounded subscription, aggregated per wall-clock frame. Durable data: the
   Arrow IPC / Parquet recording. An exporter (Kafka, NATS) can be added later as just another subscriber if some
   outside system needs it.
4. **Frames are budgeted, never back-pressure.** The aggregator sends one frame per display interval (default
   16 ms) and keeps it under a byte budget with level of detail (exact spikes → per-neuron counts → spatial bins).
   Slow clients lose frames, which are counted and shown; the simulation never waits.
5. **Stimulus and sensors run in simulation time.** Built-in input actors (Poisson, periodic, recorded-signal
   replay, scripted pulses) are exact and reproducible; interactive input (buttons, hardware) uses
   `inject_live`. v1's JavaScript sensor scripts become a UI-side *design* tool whose output is saved as a signal
   file the engine replays.
6. **Projects live with the user.** A project is a directory (`project.toml`, `.boo` files, input files);
   recordings are written where the simulation runs and can be downloaded as Parquet.
7. **The UI is React 19 + TypeScript + Vite with Material UI** (`@mui/material`, replacing v1's Fluent UI) and
   **`stream-charts` 2.1** for every chart (raster, membrane, weights, timing health), porting the v1 Monaco
   language support, Redux slices and three.js scenes. Only the 3D view uses WebGL directly.
8. **Generated types.** `ts-rs` derives TypeScript for every protocol type; CI fails if the generated files are
   stale.

## Questions for review

1. **Kafka**: drop it (recommended, decision 3), or keep an optional Kafka exporter from the start?
2. **UI framework**: settled (2026-11-22): React with Material UI and `stream-charts` 2.1.
3. **Sensors**: in-simulation input actors plus signal files (recommended, decision 5), or keep v1-style JS
   sensor scripts that stream from the UI in wall-clock time?
4. **Browser access**: should `spikes-server` also serve the UI as a web app (recommended: nearly free, since the
   UI already speaks WebSocket)?
5. **Order of work**: engine → protocol + server → Tauri app (recommended: everything is testable headless before
   any UI exists), or Tauri first?
