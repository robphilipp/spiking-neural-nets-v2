# Architecture

```
┌──────────────────────────── node (one OS process) ────────────────────────────┐
│  Control plane (tokio): Coordinator/NodeAgent, Directory, NameRegistry, API  │
│        │ commands applied at window boundaries                               │
│  ┌─────▼─────┐  SPSC rings  ┌───────────┐            ┌───────────┐          │
│  │ Shard 0   │◀────────────▶│ Shard 1   │ ◀────────▶ │ Shard N-1 │  (1/core) │
│  │ arenas    │              │ arenas    │            │ arenas    │          │
│  │ timeQueue │              │ timeQueue │            │ timeQueue │          │
│  │ routes    │              │ routes    │            │ routes    │          │
│  └─────┬─────┘              └─────┬─────┘            └─────┬─────┘          │
│        └──────── outbound batches per remote node ─────────┘                │
│                         │ Transport (shm/UDS | TCP/QUIC)                     │
│  Window barrier (atomics in-process; all-reduce across nodes)                │
│  Event taps ──▶ aggregator ──▶ UI adapter / file sinks                       │
└──────────────────────────────────────────────────────────────────────────────┘
```

## 1. Identity and location

- `ActorId(u64)`, allocated in **blocks** (for example 2¹⁶ IDs per block). Each node allocates IDs from its own
  blocks, so there's no central allocator. The top values are reserved (`WORLD`, `COORDINATOR`, …).
- `Location { node: u32, shard: u16, slot: u32 }`. `node` is u32 so the design scales past 65k nodes.
- **Directory** = `block → (node, shard)`, plus a small sparse override map for migrated actors. It's replicated on
  every node: about 2.4 MB for 10¹⁰ actors. A per-actor table (8 bytes × 10⁹ = 8 GB per node) would not scale to
  billions; see [08](08-scaling-to-billions.md). The block table, its epoch and the overrides are distributed
  through the control plane.
- **Why not hard-code the location in the ID?** A block is a level of indirection, so blocks (and overrides) can be
  reassigned for rebalancing or restore onto a different node count. Lookup stays a shift plus an indexed load.
- `ActorRef` is a `Copy` handle `{ id, cached_location }`. Sending through it skips the directory lookup.
  A stale cache (epoch mismatch) is detected at the destination node and forwarded (later, with migration).
- **NameRegistry** (control plane only): `String → ActorId` for `.boo` names like `input-11`, plus pattern queries
  (the regex lookups that v1's `Network` actor did). Never used on the hot path.

## 2. Actor storage ("lightweight")

- An actor is a plain struct implementing `Actor`. There's no task and no mailbox per actor.
- A shard holds **typed arenas**, one `Vec<A>` per registered actor kind (for example `Vec<Neuron>`,
  `Vec<InputSensor>`). `slot` encodes `(kind: u8, index: u24)` or the shard keeps a `slot → (kind, idx)` table.
  Same-kind actors are contiguous, which is cache-friendly and allows later structure-of-arrays (SoA) layouts.
- Dispatch: `match kind { … }` through a small generated function table. That's static dispatch inside each kind.
  A `Box<dyn AnyActor>` fallback kind stays available for rare or heterogeneous actors.
- Per-actor framework overhead: `seq` counter (8 bytes), RNG state (about 40–136 bytes depending on the RNG; can be
  created lazily from `(seed, id)`), and flags. Target: 64 bytes or less, plus user state.

## 3. Messages

- **Data plane**: each runtime instance has one user message type `M` (usually an enum such as
  `enum SimMsg { Spike{…}, Tick{gen}, Inject{…} }`). `M: Send + 'static`, and `M: Serialize + Deserialize` when
  multi-process. A single enum keeps dispatch static, makes serialization trivial and keeps queues monomorphic.
  The cost is that every actor kind shares `M`, which suits a simulation.
- `Envelope<M> { at: SimTime, src: ActorId, seq: u64, dst: ActorId, msg: M }`. Keep `M` small, 32 bytes or less if
  possible. Large payloads go through `Arc` or route tables.
- **Control plane**: type-erased `Box<dyn Any + Send>` requests with typed reply futures. They're low volume.
- **System events** (infrastructure: overload, lateness, node loss) are a framework-defined enum on a separate
  queue.
- The full design, including region messages, typed refs and dynamic topology, is in
  [07](07-message-types-and-dynamic-topology.md).

## 4. Per-shard time queue

The queue must support fast insert/pop-min for keys `(at, src, seq)`, with many events in the near future (µs to
ms) and some far away (timers up to hours).

- **Baseline**: `BinaryHeap` (or a 4-ary heap) of compact 24-byte keys pointing into a slab of envelopes. Simple,
  O(log n).
- **Target**: a **hierarchical timing wheel / calendar queue**:
  - Level 0 has 1 µs buckets × 4,096 (about 4 ms). Higher levels hold coarser buckets. Far-future events go to a
    heap.
  - Popping a bucket sorts its few entries by full key, so ns ordering and tie-breaking are preserved.
  - Average O(1) insert and pop.
  - This descends from v1's hashed wheel (and the `actor-timed` stub), but it runs in **simulation time**, so there
    is no tick thread and no sleep-precision problem.
- Behind a `TimeQueue` trait, with benchmarks to choose between them (phase 1). Radix heaps are a third candidate,
  since keys never go below the current time.

## 5. Fan-out (routes)

v1 sent one scheduled message per synapse. v2 adds **routes**: precomputed one-to-many edges with per-edge delay
and an edge index.

- `ctx.fanout(route, msg)` sends **one** envelope per *destination shard* (and per destination node, for remote
  ones). It carries `(route_id, src, at_send, msg)`.
- The destination shard's `RouteTable[route_id] = [(target_slot, delay, edge_idx)]` expands it locally into
  per-target events at `at_send + delay`. The handler receives `edge_idx` so the neuron can find synapse-specific
  state (weight, STDP).
- **Per-edge stochastic release** (Maass facilitation/depletion) happens at the *sender* in v1. In v2 it moves to
  the receiving side. Each synapse record (already stored with the post-synaptic neuron for weight and STDP) also
  holds its facilitation and depletion state. It makes the release draw when the spike arrives, which is when the
  action potential reaches that terminal. See 06, "Explanation of decision 4", for why and what changes.
- The lookahead is computed from route-edge delays plus declared minimum delays for plain sends.

## 6. Transports

```rust
trait Transport {   // one instance per remote peer (node or process)
    fn send_batch(&self, window: WindowId, frames: &[u8]) -> Result<()>;
    fn poll_batches(&self, sink: &mut dyn FnMut(WindowId, &[u8]));
    fn barrier_report(&self, report: WindowReport);           // for the distributed window protocol
}
```

| Path | Mechanism | Notes |
|------|-----------|-------|
| Same shard | push directly into the local time queue | no copy, no serialization |
| Other shard, same process | per-pair SPSC ring of `Envelope<M>` (moved, not serialized) | N² rings; fine up to about 64 shards. Batch per window. |
| Other process, same host | **UDS first** (simple). Then a shared-memory SPSC ring (`memmap2`, POSIX shm) | HORUS in `play-area/rust/horus` is a reference for shm IPC |
| Other host | **TCP** with length-delimited, per-window batched frames (tokio). **QUIC** (`quinn`) later | one connection per peer pair. Frames can be compressed. |

- Serialization: `postcard` or `bincode` to start, with `rkyv` (zero-copy) as an optimization. Only messages that
  cross a process boundary are serialized.
- Batching per window is natural. A window's outbound messages to one peer become one frame, so the per-message
  network cost amortizes to bytes.
- **Topology / membership**: start with a static cluster config (the next generation of `.boo` `GRP`: node name →
  address). A node joins by connecting to the coordinator, which hands out shard and directory assignments.
  Dynamic membership is out of scope.

## 7. Cross-node synchronization

**Default: real-time, best-effort (see 02 §4, Tier 2).**
- The coordinator owns the **clock schedule** (a list of `{effective_wall, sim_anchor, time_factor}` segments) and
  broadcasts changes with a short future effective time. Nodes keep wall clocks aligned with PTP or NTP, and the
  coordinator estimates and reports residual offsets.
- Each node advances its horizon from the schedule (optionally plus its inbound lookahead `L_in`). Outbound
  cross-node messages are batched per peer every 50–200 µs (wall time) and sent over TCP, later QUIC.
- On receipt, messages are inserted into the target shards' inbound rings. A shard delivers each one at
  `max(at, actor.last_event_time)`, so late ones are delivered at the actor's current time and counted.
- Each node publishes metrics per link: on-time fraction, lateness histogram, horizon lag versus the schedule.
- Overload policy: a node whose horizon lags by more than X ms posts a `CannotKeepUp` system event, and
  `CaughtUp` when it recovers. The clock is never slowed automatically (07 §4).

**Optional: strict mode (reproducible).**
- Every node runs its in-process barrier, then sends one node-level report per window:
  `{ next_event_time, msgs_sent_per_peer, msgs_received_per_peer }`.
- The coordinator, or a tree all-reduce, computes `T_next` and checks that nothing is in flight, then broadcasts
  `Go(T_next)`.
- Per-link null messages (CMB) are a later optimization.

## 8. Control plane

- Runs on a tokio runtime **separate** from the shard threads.
- Operations:
  - `spawn(kind, init) -> ActorId`, `spawn_batch`
  - `connect/route_add`
  - `query(id, Q) -> R`, `inspect(id) -> Debug snapshot`
  - `inject(id, at, msg)`
  - `start(pacing)`, `set_time_factor(f)`, `pause`, `resume`, `step(until)`, `stop`, `reset`. Clock changes become
  clock-schedule segments (02 §3).
  - `snapshot()/restore()`
- Commands are queued and applied by each shard **at the next window boundary**, at a well-defined simulation time,
  so they are deterministic relative to the data plane. Queries are answered at boundaries too, so they see a
  consistent state.
- The web layer (Tauri or axum) calls only this API and subscribes to event taps.

## 9. Observability

- `ctx.emit(event)` writes to a per-shard SPSC ring of user-defined `E` (for example
  `Spike{id, t}`, `Weight{post, pre, w, t}`).
- An aggregator thread merges the shard rings and fans out to subscribers:
  - UI stream: batched per frame interval (for example 16 ms of wall time), optionally sampled or filtered by actor
    set, with a bounded buffer that drops and counts when full. It must never back-pressure the simulation.
  - File sink (run recording): lossless, in **Apache Arrow** format, replacing v1's text logs and Kafka.
- **Recording format** (decided: compact and standard):
  - While running: **Arrow IPC stream** files, written in batches (cheap to append, columnar, typed).
  - After the run, or on demand: converted to **Parquet** (compressed, about 5–10× smaller) for archiving.
  - Both are readable without custom parsers by Julia (`Arrow.jl`, `Parquet2.jl`), Python (`pyarrow`, `polars`,
    `pandas`), R, DuckDB and Spark.
  - One table per event kind with fixed columns, for example:
    - `spikes(neuron_id u64, t_ns u64)`
    - `weights(post u64, pre u64, edge u32, t_ns u64, w f32)`
    - `topology(t_ns, op, pre, post, edge, delay_ns)`
    - `membrane(neuron_id, t_ns, u_mv f32)`, one row per update (no sampling clock), for a selectable neuron set
      (09 §4)
    - `system(t_ns, wall, severity, kind, json)`
  - Metadata tables: `neurons(id, name, kind, x, y, z, group)` and `run(seed, network_description, params, start)`.
  - The UI's live stream can use the same Arrow record batches (Arrow has a JS library), so live and recorded data
    share one schema.
- Runtime metrics: events per second per shard, window count, idle-skip ratio, barrier wait time, pacing lag,
  lookahead violations, queue depths. Exposed with `tracing` plus a metrics snapshot API.

## 10. Failure handling

- A handler panic is caught with `catch_unwind` around dispatch. The actor is marked `Failed`, its later messages
  are dropped and counted, and the error is reported to the control plane. The simulation continues or halts,
  depending on policy.
- A node failure (lost connection, missed barrier timeout) aborts the run with a clear error.
- **Snapshot/restore**: actors are `serde`-serializable. A snapshot is taken at a window boundary: all actor states,
  queues, RNG states, the directory and the input journal. This also answers v1's "dump network state so it can be
  reloaded".

## 11. Partitioning

Partitioning matters more now. Long cross-host edges mean fewer late messages, and long cross-shard edges mean
fewer windows.

- The first version uses explicit placement: the `.boo` group maps to a node, and neurons are spread across shards
  within a node by spatial locality (sort by a Morton/Z-order code of the location, then split into contiguous
  chunks).
- At 1M+ neurons, automatic spatial partitioning (Morton or k-d split of the volume across nodes) becomes the
  default. `.boo` groups act as optional constraints.
- Later: automatic graph partitioning that maximizes the minimum cross-partition delay and balances event load.
  Use a greedy approach or METIS-style tools.

## Proposed crate layout (Cargo workspace)

```
crates/
  tempo-core/        SimTime, ActorId, Location, Envelope, Actor/Context traits (no deps on threads)
  tempo-queue/       TimeQueue trait + heap + timing-wheel impls + benches
  tempo-runtime/     Shard, arenas, window loop, in-process barrier, SPSC rings, pacing, taps
  tempo-transport/   Transport trait, UDS/shm, TCP (later QUIC), codecs
  tempo-cluster/     coordinator, node agent, directory replication, distributed window protocol
  tempo-testkit/     single-threaded reference executor, deterministic trace hashing, assertions
  tempo-bench/       ring / ping-pong / random-LIF benchmarks
```
"tempo" is a working name. Rename freely.
