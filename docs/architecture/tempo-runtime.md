# The tempo runtime

`tempo-runtime` runs millions of small actors in simulated time, in parallel, with results that don't depend on how
many threads run them. This page explains how, from the actor up to the window loop.

## The ideas in one paragraph

An **actor** is a plain struct with a handler. Every message carries the exact **simulation time** it is delivered
at, and messages are handled strictly in `(time, sender, sequence, sub)` order (the `OrderKey`), so actors never see
the wall clock. Actors are grouped into **partitions** (64 by default), and partitions are spread over **shards**,
one thread each. Shards run in parallel through **conservative time windows**: no message between shards can be
shorter than the **lookahead** `L` (the smallest cross-shard delay), so everything in a window `[T, T+L)` can be
processed without hearing from the other shards. At the end of each window the shards meet at a **barrier**,
exchange the messages they made for each other, and agree on the next window. A **clock** decides how far windows
may run: as fast as possible, or paced against the wall clock (speed, pause, step).

## Inside one process

```mermaid
flowchart LR
    subgraph RT["Runtime (one process)"]
        direction TB
        DIR["Directory<br/>ID block → (node, shard)<br/>route → (node, shard)"]
        CLK["ClockHandle<br/>unpaced · running(anchor, factor) · paused"]
        CTRL["Control<br/>per-shard command queues"]
        BAR["WindowBarrier<br/>min-reduce, last one decides"]
        subgraph S0["Shard 0 (thread)"]
            Q0["TimeQueue<br/>(timing wheel)"]
            A0["actor arenas<br/>(per Rust type)"]
            O0["outboxes<br/>per shard, per node"]
            F0["fields (copy)"]
        end
        subgraph S1["Shard 1 (thread)"]
            Q1["TimeQueue"]
            A1["actor arenas"]
            O1["outboxes"]
            F1["fields (copy)"]
        end
        SUB["Subscriptions<br/>EventStream batches"]
        SYS["SystemEvents<br/>(cannot keep up, lookahead changed, …)"]
    end
    S0 <-- "mailboxes at window end" --> S1
    S0 --- BAR
    S1 --- BAR
    BAR --- CLK
    CTRL -. "applied at window boundaries" .-> S0
    CTRL -.-> S1
    S0 -- "emitted events" --> SUB
    S1 -- "emitted events" --> SUB
    S0 --> SYS
```

- **No thread, task or mailbox per actor.** Actors live in per-type arenas on their shard; dispatch is one virtual
  call per ID block, then static calls into the actor's handler. Millions of neurons fit in memory.
- **One time-ordered queue per shard** (`tempo-queue`: a hierarchical timing wheel, or a heap). It always yields the
  smallest `OrderKey`.
- **Fields** (neuromodulators, for example) are shard-local copies, updated by timed broadcasts and read lazily by
  actors at their own time and position.

## IDs, partitions and shards: why the shard count doesn't change results

```mermaid
flowchart LR
    subgraph build["Build (same order every time)"]
        N["neurons 0 … n−1<br/>(spatial order)"] --> P["partitions 0 … 63<br/>(nearby neurons together)"]
        P --> IDS["ActorIds allocated per partition<br/>(ID blocks)"]
    end
    subgraph one["1 shard"]
        S1A["shard 0: partitions 0–63"]
    end
    subgraph four["4 shards"]
        S4A["shard 0: 0–15"]
        S4B["shard 1: 16–31"]
        S4C["shard 2: 32–47"]
        S4D["shard 3: 48–63"]
    end
    IDS --> S1A
    IDS --> S4A & S4B & S4C & S4D
```

An actor's ID depends only on its partition and the order it was spawned in, never on the shard count. Its random
stream is seeded from the run's seed and its ID. Event order is `(time, sender, sequence, sub)`, which doesn't
mention shards. So the same network gives the same results on 1 shard or 16. Contiguous ranges of
spatially-ordered partitions keep most connections inside a shard, and make the cross-shard ones the long, slow
ones, which raises the lookahead.

## Where a message goes

```mermaid
flowchart TD
    SEND["ctx.send_at(to, at, msg)<br/>key = (at, me, my seq, sub)"] --> HOME{"directory:<br/>home of 'to'"}
    HOME -- "this shard" --> LOCAL["push on this shard's queue<br/>(handled this window if at < window end)"]
    HOME -- "another shard, this node" --> LATE1{"at < window end?"}
    HOME -- "another node" --> LATE2{"at < window end?"}
    HOME -- "unknown" --> DROP["dropped, counted"]
    LATE1 -- "no" --> OB["outbox[shard]<br/>→ its mailbox at window end"]
    LATE1 -- "yes (shorter than lookahead)" --> FIX1["delivered at window end instead,<br/>counted as a lookahead violation"] --> OB
    LATE2 -- "no" --> RB["remote outbox[node]<br/>→ Link at window end"]
    LATE2 -- "yes" --> FIX2["moved to window end, counted"] --> RB
```

Fan-out **routes** (one spike to all of a neuron's targets, each with its own delay), **broadcasts** (to a group or
a region of space) and **field updates** are sent as one message per shard (or per node) and expanded where they
arrive, so reaching a million actors costs a handful of messages.

## One window, step by step

Every shard runs the same loop (`Shard::work`). The barrier reduces two values across shards (the earliest pending
event and the proposed lookahead), and the last shard to arrive turns them into the next window's **decision**.
Everyone gets the same decision, so the shards agree on window boundaries even when the decision depends on the
wall clock.

```mermaid
sequenceDiagram
    autonumber
    participant C as Clock
    participant S0 as Shard 0 (leader)
    participant S1 as Shard 1
    participant B as WindowBarrier

    Note over S0,S1: window [T, T+L) agreed at the last barrier
    S0->>C: wait until the clock allows T (paced runs)
    S1->>C: wait until the clock allows T
    Note over S0,S1: while waiting: apply control commands (inspect, inject, step…)
    par in parallel
        S0->>S0: pop and handle every event with time < T+L
        S1->>S1: pop and handle every event with time < T+L
    end
    S0->>S1: flush outbox → S1's mailbox (messages for S1)
    S1->>S0: flush outbox → S0's mailbox
    S0->>S0: publish emitted events (one batch) to subscribers
    S1->>S1: publish emitted events
    S0->>B: arrive(earliest pending, lookahead proposal, stop?)
    S1->>B: arrive(earliest pending, lookahead proposal, stop?)
    Note over B: last to arrive decides:<br/>start = earliest pending event<br/>(paced: min(that, clock horizon), so idle time follows the clock)<br/>end = start + min(lookahead, max paced window)
    B-->>S0: decision {start, end, lookahead, stop}
    B-->>S1: same decision
    S0->>S0: drain mailbox into queue, apply commands
    S1->>S1: drain mailbox into queue, apply commands
    Note over S0,S1: next window [start, end)
```

Unpaced (as fast as possible), idle stretches are skipped: the next window starts at the next pending event. Paced,
windows follow the clock through idle stretches, so input injected meanwhile is handled promptly.

## The clock

```mermaid
stateDiagram-v2
    [*] --> Unpaced: as fast as possible
    [*] --> Running: speed × f
    Running --> Paused: pause (horizon frozen at "at")
    Paused --> Running: resume (re-anchored at now)
    Running --> Running: set speed (re-anchored, no jump)
    Running --> Unpaced: as fast as possible
    Unpaced --> Running: speed × f
    Paused --> Paused: step(Δ) — runs Δ, pauses again
```

Running, the clock maps wall time to simulated time: `horizon(w) = anchor_sim + (w − anchor_wall) / time_factor`.
The horizon is the latest simulated time shards may reach, so it limits how far they run. It never sets the time an
actor sees, which always comes from its message. The leader shard compares the time each window was due with when it
started. If it falls behind by more than a threshold, it publishes `CannotKeepUp`, and `CaughtUp` once it recovers.

Stop is not a clock state: `stop()` bumps a stop epoch, and a run taken with an earlier stop token ends at its next
barrier. The clock itself keeps its mapping. That's why Spikes measures "behind the clock" only while a run goes
([spikes-on-tempo.md](spikes-on-tempo.md)).

## Control and subscriptions: touching a running simulation without stopping it

```mermaid
sequenceDiagram
    participant API as Caller (any thread)
    participant CT as Control
    participant SH as Owning shard
    participant ES as EventStream (subscriber)

    API->>CT: query(actor) / update / inject / route change
    CT->>SH: command on the owner's queue
    Note over SH: applied at the next window boundary,<br/>or while idle waiting for the clock (so a paused run can be inspected)
    SH-->>API: Reply (wait or .await)
    loop every window
        SH->>ES: batch of emitted events
        Note over ES: bounded (UI): a full buffer loses the batch, counted<br/>lossless (recorder): unbounded, never loses
    end
    Note over SH,ES: the simulation never waits for a subscriber
```

## Code map

| Concern | Where |
| --- | --- |
| Time, IDs, order keys | `crates/tempo-core/src/{time,id,envelope}.rs` |
| Queues | `crates/tempo-queue/src/{wheel,heap}.rs` |
| Window loop, send paths, outboxes | `crates/tempo-runtime/src/shard.rs` |
| Barrier and decision | `crates/tempo-runtime/src/barrier.rs`, `decide` in `shard.rs` |
| Partitions and placement | `crates/tempo-runtime/src/placement.rs` |
| Clock | `crates/tempo-runtime/src/clock.rs` |
| Control plane, subscriptions, system events | `crates/tempo-runtime/src/{control,subscribe,system}.rs` |
| Routes, broadcasts, fields | `crates/tempo-runtime/src/{route,broadcast,field}.rs` |
