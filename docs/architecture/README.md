# Architecture

How the engine is put together, with diagrams (Mermaid: GitHub and most Markdown viewers draw them).

| Document | What it covers |
| --- | --- |
| [tempo-runtime.md](tempo-runtime.md) | The `tempo` actor runtime: actors, IDs and partitions, shards, conservative time windows, the window barrier, the clock, the control plane and subscriptions. |
| [tempo-cluster.md](tempo-cluster.md) | One simulation over several processes or hosts: node placement, building a cluster, the strict (lockstep) window protocol, best-effort runs, and how events reach the coordinator. |
| [spikes-on-tempo.md](spikes-on-tempo.md) | How the Spikes app uses `tempo`: a session, a run, and live frames from the shards to the UI. |

Related: [cluster.md](../cluster.md) (running a cluster, for users), `docs/plans/actors/` (the design as planned).

## The layers

```mermaid
flowchart TB
    subgraph clients["Clients"]
        UI["spikes-ui<br/>(React, browser or Tauri desktop)"]
        CLI["spikes-cli"]
    end
    subgraph server["Server processes"]
        SRV["spikes-server<br/>(HTTP + WebSocket API)"]
        DESK["spikes-desktop<br/>(Tauri: the engine in the app)"]
    end
    subgraph app["Spikes engine (application)"]
        SES["spikes-session<br/>sessions, runs, live frames, recording,<br/>inputs, sensors, cluster coordinator / node agent"]
        NET["spikes-network<br/>neuron and input actors, SimMsg / Event,<br/>build a runtime from a description"]
        DNA["spikes-dna<br/>.boo descriptions"]
        MOD["spikes-model<br/>neuron and synapse models"]
        REC["spikes-record<br/>recordings (Arrow)"]
        SIG["spikes-signal<br/>formulas, encoders, data"]
        API["spikes-api<br/>protocol, frames"]
    end
    subgraph tempo["tempo (generic timed-actor framework, knows nothing of neurons)"]
        RT["tempo-runtime<br/>actors, shards, windows, clock,<br/>control, subscriptions, fields"]
        CL["tempo-cluster<br/>StrictLink, TCP / memory transports"]
        Q["tempo-queue<br/>time-ordered queues (heap, wheel)"]
        CORE["tempo-core<br/>SimTime, ActorId, Envelope, OrderKey"]
    end

    UI -- "WebSocket: requests, binary frames" --> SRV
    UI -- "Tauri commands" --> DESK
    CLI --> SES
    SRV --> SES
    DESK --> SES
    SES --> NET
    SES --> REC
    SES --> API
    SES --> CL
    NET --> DNA
    NET --> MOD
    NET --> SIG
    NET --> RT
    CL --> RT
    RT --> Q
    RT --> CORE
    Q --> CORE
```

The rule that keeps the layers apart: `tempo-*` crates never depend on `spikes-*` crates. `tempo` is a framework for
timed actors in general. Spikes supplies the actors (neurons, inputs), their message type and their event type.
