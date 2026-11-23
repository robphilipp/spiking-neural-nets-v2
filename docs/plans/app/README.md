# Plan: app layer (session engine, server, Tauri UI)

Status: **reviewed**, 2026-11-22 (decisions below). **A1–A3 done**; A4 (views and interaction) next. Builds on the actor framework (`docs/plans/actors/`, phases 1–3 done) and
the neuron crates (`docs/plans/neurons/`, N1–N4 done). Background on the v1 UI and server is in
`../../../notes/05-v1-ui.md` and `../../../notes/04-v1-runtime-architecture.md`; the web-layer recommendation this
plan expands is in `../../../notes/06-lessons-and-v2-implications.md`.

## The idea in one paragraph

Put everything a running simulation needs into a **session engine library** (`spikes-session`): parse, build, run
with the live clock, take commands, record to Arrow, and turn the event stream into **compact, binary,
per-frame batches** for viewers. Define one **protocol** (`spikes-api`: JSON requests, responses and notices, plus
binary frames) with TypeScript types generated from the Rust. Serve it two ways with thin adapters: **Tauri IPC**
(the desktop app embeds the engine, no server to start) and **WebSocket** (`spikes-server`, for remote machines,
browsers, and later the cluster coordinator). **Sensors are external programs** (a microphone, a camera, a robot)
that stream samples into a running network over the same transports; the engine encodes them into neuron input
at the right simulation times. The **UI is one TypeScript code base** that talks to a `SpikesClient`
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
3. **No Kafka (for now).** Live data: the runtime's bounded subscription, aggregated per wall-clock frame. Durable data: the
   Arrow IPC / Parquet recording. An exporter (Kafka, NATS) can be added later as just another subscriber if some
   outside system needs it.
4. **Frames are budgeted, never back-pressure.** The aggregator sends one frame per display interval (default
   16 ms) and keeps it under a byte budget with level of detail (exact spikes → per-neuron counts → spatial bins).
   Slow clients lose frames, which are counted and shown; the simulation never waits.
5. **Sensors live outside the simulation.** A sensor is any program (Python, Rust, the UI itself using the
   browser's microphone) that connects to a session and streams timestamped samples on named channels. The project
   maps channels to neurons and an encoding (current, rate, threshold crossings, spikes); the engine schedules the
   encoded input in simulation time, keeping the timing between samples. Everything a sensor sends is recorded, so
   a run can be replayed exactly. Built-in input actors (Poisson, periodic, scripted pulses) remain for synthetic
   stimulus. See [01 §5](01-architecture.md#5-sensors).
6. **Projects live with the user.** A project is a directory (`project.toml`, `.boo` files, input files);
   recordings are written where the simulation runs and can be downloaded as Parquet.
7. **The UI is React 19 + TypeScript + Vite with Material UI** (`@mui/material`, replacing v1's Fluent UI) and
   **`stream-charts` 2.1** for every chart (raster, membrane, weights, timing health), porting the v1 Monaco
   language support, Redux slices and three.js scenes. Only the 3D view uses WebGL directly.
8. **Generated types.** `ts-rs` derives TypeScript for every protocol type; CI fails if the generated files are
   stale.

## Decisions (2026-11-22)

1. Kafka: dropped for now; an exporter can be added later as a subscriber.
2. UI: React with Material UI and `stream-charts` 2.1.
3. Sensors: external programs streaming into the network (decision 5), e.g. a microphone routed to neurons.
4. `spikes-server` also serves the UI as a web app.
5. Order: engine (A1) → protocol and server (A2) → Tauri app (A3) → the rest.
