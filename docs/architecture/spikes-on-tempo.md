# Spikes on tempo: sessions, runs and live frames

`spikes-network` turns a `.boo` description into tempo actors: neurons, input actors and modulator fields.
`spikes-session` wraps a built runtime in a **session**, which can be built, run, paused, stepped, stopped, inspected,
viewed live and recorded. `spikes-server` (or the desktop app) exposes sessions to the UI.

## A session's parts

```mermaid
flowchart TB
    subgraph Session
        direction TB
        ST["phase: Empty · Ready(built runtime) · Running(control, clock) · Failed"]
        HUB["Hub (frame aggregator thread)<br/>slots: one per viewer"]
        INP["inputs: sources, trials, sensors"]
        RECD["recorder (lossless subscription)"]
        CO["cluster coordinator (optional)"]
    end
    RT["tempo Runtime<br/>(neurons, inputs, fields)"]
    RT -- "EventStream (bounded):<br/>spikes, membrane, weights, transitions" --> HUB
    RT -- "lossless EventStream" --> RECD
    RT -- "ClockHandle" --> HUB
    HUB -- "binary frames per viewer" --> WS["server: WebSocket per viewer"]
    WS --> UI["UI live data → charts, 3D view, stats"]
```

## Starting and running

```mermaid
sequenceDiagram
    autonumber
    participant UI
    participant API as spikes-server
    participant S as Session
    participant RTh as Run thread
    participant RT as tempo Runtime (shards)
    participant H as Hub

    UI->>API: start {speed, duration, record}
    API->>S: start(settings)
    S->>S: set clock: unpaced, or follow Running(anchor now, factor)
    S->>S: phase = Running{control, clock}
    S->>H: run_started()
    S->>RTh: spawn (with a stop token taken now)
    S-->>UI: StateChanged(Running)
    RTh->>RT: run_until_unless_stopped(end, token)
    loop windows
        RT->>H: event batches
        H->>UI: a frame per viewer every frame period (spikes, membrane, weights, lag)
    end
    alt Stop pressed
        UI->>API: stop
        API->>S: stop(): clock.stop() bumps the stop epoch, waits for Ready
        RT-->>RTh: returns at the next barrier
    else duration reached
        RT-->>RTh: returns at end
    end
    RTh->>H: run_ended() (the lag is frozen from here)
    RTh->>S: apply leftover control commands, hub.advance(now), phase = Ready
    S-->>UI: StateChanged(Ready)
    H->>UI: final frame (up to the run's last time)
```

## From events to the UI's charts

```mermaid
sequenceDiagram
    participant SH as Shards
    participant ES as EventStream (bounded)
    participant AG as Hub aggregator thread
    participant VW as Viewer (per WebSocket)
    participant LV as UI LiveView

    loop each window
        SH->>ES: one batch of emitted events
        Note over ES: full buffer → batch dropped, counted ("Dropped … events")
    end
    loop every tick
        AG->>ES: recv batch (timeout)
        AG->>AG: route each event into every viewer slot's pending frame<br/>(its spike / membrane / weight selections)
        AG->>AG: slot due? lag = clock horizon − sim (while running, frozen after)<br/>budget too tight → counts, then cells (level of detail)
        AG->>VW: encoded frame (seq, t0, t1, speed, lag, lod, drops)
        Note over VW: slow viewer → frame dropped, counted, time is never lost
        VW->>LV: binary frame
        LV->>LV: run gate → stream-charts data sources,<br/>stats (timing panel, 5 per second)
    end
```

The frame header's **behind the clock** (`lag_ms`) is the clock's horizon minus the simulated time the frame
reaches. It is measured only while a run goes. A run's clock keeps its mapping after the simulation stops, so after
the end, frames carry the lag the run last had.

## Code map

| Concern | Where |
| --- | --- |
| Building actors from a description | `crates/spikes-network/src/build.rs` |
| Neuron and input actors, messages, events | `crates/spikes-network/src/{neuron,inputs,messages}.rs` |
| Session phases, start / stop / pause / step | `crates/spikes-session/src/session.rs` |
| Frame aggregator, viewers, level of detail | `crates/spikes-session/src/viewer.rs` |
| Frame format | `crates/spikes-api/src/frame.rs` |
| UI side | `apps/spikes-ui/src/{live,runGate,chartSettings}.ts` |
