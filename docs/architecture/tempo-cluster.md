# tempo over several machines

A cluster runs **one** simulation over several processes or hosts ("nodes"). `tempo-runtime` knows how to split
actors over nodes and when to hand messages to another node. `tempo-cluster` supplies the [`Link`] between them: the
transport and the cross-node window protocol. `spikes-session` adds the application side: a coordinator, node agents,
and forwarding of events to the coordinator. For running a cluster, see [cluster.md](../cluster.md).

## Deployment

```mermaid
flowchart LR
    UI["spikes-ui"] -- "API (WebSocket)" --> C
    subgraph C["Coordinator: spikes-server (node 0)"]
        direction TB
        CS["Session<br/>clock, viewers, recordings,<br/>sensors, API"]
        CR["Runtime: node 0's share<br/>(shards 0…k−1)"]
        CL["StrictLink (coordinator)"]
    end
    subgraph N1["Node agent: spikes-server --node (node 1)"]
        R1["Runtime: node 1's share<br/>(shards 0…k−1)"]
        L1["StrictLink"]
    end
    subgraph N2["Node agent (node 2)"]
        R2["Runtime: node 2's share"]
        L2["StrictLink"]
    end
    CS -. "control protocol<br/>(postcard over TCP):<br/>Prepare, Build, Run, AddInput,<br/>Query, Observe, …" .-> N1
    CS -.-> N2
    CL <== "mesh (TCP, one connection per pair):<br/>spikes, window reports/decisions,<br/>forwarded events" ==> L1
    CL <==> L2
    L1 <==> L2
```

Two kinds of connection:
- **Control** (coordinator → each agent): request/response for building, running and per-neuron requests.
- **Mesh** (every pair of nodes): the simulation's own traffic. Spikes go straight from node to node, not through
  the coordinator. Window reports and decisions go between each node and the coordinator. Emitted events and system
  notices go to the coordinator.

## Placement across nodes

```mermaid
flowchart LR
    P["64 partitions<br/>(spatial order)"] --> NA["node 0: partitions 0–21"]
    P --> NB["node 1: partitions 22–42"]
    P --> NC["node 2: partitions 43–63"]
    NA --> NA0["shard 0"] & NA1["shard 1"]
    NB --> NB0["shard 0"] & NB1["shard 1"]
    NC --> NC0["shard 0"] & NC1["shard 1"]
```

Every node builds the **same network in the same order**, so every node has the same directory: which ID block
lives on which (node, shard). Each node keeps only its own actors. Partitions go to nodes as contiguous ranges, then
to each node's shards. A description can also pin neurons to a named node. IDs don't depend on placement, so a strict
cluster run gives exactly the results of one process.

## Building a cluster

```mermaid
sequenceDiagram
    autonumber
    participant S as Coordinator session
    participant A1 as Agent 1
    participant A2 as Agent 2

    S->>S: bind mesh listener (address the agents reach)
    S->>A1: Prepare
    A1-->>S: Prepared(mesh address)
    S->>A2: Prepare
    A2-->>S: Prepared(mesh address)
    par each agent builds (in its own thread)
        S->>A1: Build{cluster id, node 1, mesh addresses, shards, description, seed, …}
        S->>A2: Build{cluster id, node 2, …}
    end
    S->>S: build node 0's share
    Note over S,A2: every node connects to every other (TcpTransport::connect, cluster id checked)
    A1-->>S: built
    A2-->>S: built
    S->>A1: exchange reach (for lookahead), owners
    S->>A2: exchange reach, owners
    Note over S: ready: a request about one neuron goes to the node that owns it
```

## A run in lockstep (strict, exact)

Each node runs the ordinary window loop. Its **last shard to reach the barrier** calls `Link::decide`, which makes
the barrier span the cluster:

```mermaid
sequenceDiagram
    autonumber
    participant C as Node 0 (coordinator)
    participant N1 as Node 1
    participant N2 as Node 2

    Note over C,N2: all nodes finish window [T, T+L) on their shards
    N1->>N2: spikes for node 2 (window's remote outbox)
    N2->>N1: spikes for node 1
    N1->>C: spikes for node 0
    N1->>C: report{earliest pending, lookahead, stop} + events sent to each node so far
    N2->>C: report + sent counts
    C->>C: reduce reports with its own, decide next window with its clock
    C-->>N1: decision{start, end, lookahead, stop} + events N1 should have received from each node
    C-->>N2: decision + expected counts
    Note over N1,N2: wait until every expected event has arrived (so none is ever late)
    N1->>N1: drain link into shards' queues, start next window
    N2->>N2: drain, start next window
    C->>C: drain, start next window
```

Each window costs a network round trip, so lockstep across a network is slower, but it is exact to the nanosecond.
Runs as fast as possible always use lockstep.

## A run in best effort

```mermaid
sequenceDiagram
    participant C as Coordinator clock
    participant N1 as Node 1
    participant N2 as Node 2

    C->>N1: Run{end, schedule = Running(anchor wall (system time), anchor sim, factor)}
    C->>N2: Run{…same schedule}
    Note over N1,N2: each node follows its own copy of the clock,<br/>no cross-node barrier
    N1->>N2: spikes, sent at each window's end
    Note over N2: arrived on time → queued normally<br/>arrived after its time → delivered at once, counted as late (and how late)
    C->>N1: Clock(new schedule) on speed / pause / resume / step
    C->>N2: Clock(new schedule)
    C->>N1: Stop
    C->>N2: Stop
```

Slower speeds give spikes more slack to cross, so the share of late spikes falls as the speed drops. Clock offsets
between machines show up as lateness, which is why NTP or PTP matters here.

## Seeing the whole network from the coordinator

```mermaid
sequenceDiagram
    participant N as Node k (shards)
    participant NL as Node k link
    participant CL as Coordinator link
    participant P as Coordinator EventPublisher
    participant V as Viewers / recorder

    loop every window
        N->>NL: emitted events (batch), system notices
        NL->>CL: AppMsg::Events (encoded), AppMsg::System
        CL->>P: publish as if local
        P->>V: frames, recordings include node k
    end
    N->>NL: run ended at t (after its last events)
    NL->>CL: RunEnded(t)
    Note over CL: the coordinator's run ends once every node has reported,<br/>so views and recordings are complete
```

## Code map

| Concern | Where |
| --- | --- |
| Node placement, `Link` trait, window report/decision | `crates/tempo-runtime/src/cluster.rs` |
| Strict protocol, best effort, arrival statistics | `crates/tempo-cluster/src/link.rs` |
| Memory and TCP transports | `crates/tempo-cluster/src/transport.rs` |
| Wire format (postcard), field codecs, event forwarding | `crates/tempo-cluster/src/wire.rs`, `lib.rs` |
| Coordinator, node agent, control protocol | `crates/spikes-session/src/cluster.rs` |

[`Link`]: ../../crates/tempo-runtime/src/cluster.rs
