# Implementation phases

Revised 2026-11-22. Real-time interaction and multi-host scale are now the primary goals, so the live clock and the
multi-host transport move earlier. Multiple processes on one host moves later.

Each phase ends with something runnable and measurable.

## Phase 0: foundations (about 1 week)

- Cargo workspace and crate skeletons (see 03-architecture). CI covers `cargo test`, `clippy -D warnings`, `fmt`,
  and Miri on the `unsafe` crates later.
- `SimTime`, `SimDuration`, `ActorId`, `Location`, `Envelope`, and the ordering key `(at, src, seq)`.
- Benchmark harness (`criterion`) and an `hdrhistogram`-based reporter.
- **Accept**: types are unit-tested, including the ordering property test and overflow checks.

## Phase 1: single-shard core (2–3 weeks)

- `Actor` trait, `Ctx`, typed arenas, kind registration, dispatch.
- `TimeQueue` trait with two implementations: a binary heap and a hierarchical timing wheel. Choose the default by
  benchmark.
- Single-threaded executor running as fast as possible, with `run_until(T)` and `step()`.
- Self-messages, lazy timer cancellation, per-actor RNG streams, `emit` taps (in-memory sink).
- Routes / `fanout` with per-edge delays and optional per-route delay jitter.
- `tempo-testkit`: reference executor, trace recording, `trace_hash()`.
- Toy models: ping-pong, a ring of N actors with µs delays (the wiki's ring networks), and a random LIF network with
  Poisson input and receiver-side stochastic release.
- **Accept**:
  - 1 µs delays are delivered exactly.
  - Overhead per event is 50 ns or less, excluding the handler.
  - At least 10M actors in under 2 GB for a 64-byte actor.

### Phase 1 status: done (2026-11-22)

Crates: `tempo-core` (time, IDs, envelope, order key), `tempo-queue` (heap and timing wheel), `tempo-runtime`
(actors, context, arenas, directory, routes with jitter, timers, per-actor RNG, event output, trace),
`tempo-testkit` (`trace_hash`, ping-pong / ring / LIF models). 49 tests, including property tests that check the
wheel against the heap and determinism tests across queue types and seeds.

Measured on the development Mac (Apple Silicon, one thread, release build):

| Check | Target | Result |
|-------|--------|--------|
| Delay precision | 1 µs exact | exact to 1 ns, for delays from 1 ns to 1 h (both queues) |
| Queue, pop + push, 1M pending | — | wheel 68 ns, heap 480 ns (wheel stays flat; it is the default) |
| Runtime overhead per event (ping-pong) | ≤ 50 ns | heap 25 ns, wheel 44 ns |
| Ring, 10k actors, 1k tokens | — | about 55 ns/event (both) |
| LIF 10k neurons × 100 synapses | — | wheel 118 ns/event (8.5M events/s); 100 ms simulated in 0.56 s on one core |
| 10M actors of 64 bytes | < 2 GB | 1.04 GB (104 bytes/actor: 64 actor + 40 framework), spawned in 0.35 s |

Changes made along the way:
- `OrderKey` gained a `sub` field (route edge position) as the last tie-breaker, so two synapses from one sender
  to the same target with the same delay are ordered deterministically. It is 32 bytes.
- `on_start` runs before the first delivery, after setup (so builders can set routes first).
- `Runtime::reserve` avoids `Vec` doubling overhead when the number of actors is known.

Deferred:
- Typed `ActorRef<K>` / nested message enums (07 §2): phase 2, alongside the shard directory.
- Panic containment per actor (03 §10): phase 2 or 3.
- Evaluating `nexosim`'s scheduler: not done; the wheel already meets the targets.
- Ideas to speed up the LIF case (the slowest): store one fan-out entry and expand it lazily, compact queue
  items, structure-of-arrays neuron state. Revisit with profiling in phase 6.

## Phase 2: multi-shard in one process, plus the live clock (3–4 weeks)

- Shard threads (pinned with `core_affinity` where supported), per-pair SPSC rings, Tier 1 windows with the
  in-process barrier, idle skipping, intra-node lookahead.
- **Clock schedule** (02 §3): real time with `time_factor`, live `set_time_factor`, pause, resume, step. Hybrid
  sleep and spin. Horizon-lag metrics.
- Spatial placement (Morton-order chunks).
- **Accept**:
  - Identical traces for 1–16 shards in as-fast-as-possible mode.
  - At least 6× speedup on 8 cores (100k neurons, 100 synapses each).
  - At `time_factor = 1`, a 10k-neuron network tracks the clock with p99 horizon error of 200 µs or less on macOS.
  - A speed change mid-run is continuous, with no jump in simulation time.

### Phase 2 status: done (2026-11-22)

What was built (in `tempo-runtime`, tests in `tempo-testkit/tests/sharding.rs`):
- **Shards**: one thread per shard, each with its own queue and actors. Windows of length *lookahead* (the shortest
  route delay between shards, less the jitter allowance, or the declared `min_remote_delay` for direct sends).
  A window barrier with a min-reduction finds the next event time across all shards, so idle stretches are skipped.
  Cross-shard messages travel as per-window batches (a mutex-guarded `Vec` per shard pair, swapped once per window).
- **Partitions** (`placement`): IDs are allocated per partition, and partitions map onto shards. The same network
  therefore gets the same IDs, random streams and results on any number of shards. `spatial_partitions` places
  nearby neurons together (Morton order).
- **Clock** (`ClockHandle`): real-time pacing with live `set_time_factor`, `pause`, `resume`, `step` and `stop`,
  callable from any thread while running. Speed changes are continuous. Hybrid sleep, then spin.
- Messages shorter than the lookahead are delivered at the next window start, deterministically, and counted
  (`Stats::lookahead_violations`, `max_lookahead_violation`).
- A panic in one shard releases the others and is re-raised (no hang).

Results (Apple M1 Max: 8 performance + 2 efficiency cores):

| Check | Target | Result |
|-------|--------|--------|
| Same results for any shard count | identical traces, 1–16 shards | identical traces and spikes for 1, 2, 3, 4, 8, 16 shards (LIF, ring; also with bounded jitter) |
| Speedup, 100k neurons × 100 synapses | ≥ 6× on 8 cores | **6.91×** on 8 shards (4.0 → 27.5 M events/s); identical spikes at every shard count |
| Real-time pacing, 10k neurons, factor 1 | p99 window lateness ≤ 200 µs | 1–4 shards: p99 1–3 µs, max 36–672 µs. 6–8 shards: p99 40 µs–8 ms |
| Speed change mid-run is continuous | no jump | tested: slow 10×, pause, step 5 ms, stop, resume; ticks stay regular |
| Single-shard overhead | no regression | unchanged (ping-pong 25/41 ns, LIF 10k 553 ms per 100 ms) |

Notes:
- **Paced runs need spare cores.** Every window waits for the slowest shard, so when all 8 performance cores are
  busy, an occasional OS preemption stalls everyone for milliseconds (and the catch-up windows count as late).
  With headroom the tail is microseconds. Follow-ups: raise thread priority (macOS QoS class) and pin threads
  (Linux), and let shards that finish early park instead of spinning.
- 10 shards on this machine is slower than 8 (the 2 extra cores are efficiency cores; the barrier waits for the
  slowest).
- Spawning slowed from 34 to 92 ns/actor (a hash lookup per spawn for the partition's open block). Only affects
  build time; worth optimizing for billion-actor builds.

Deferred:
- Typed `ActorRef<K>` and nested message enums (07 §2): phase 3, with the control plane.
- Lock-free SPSC rings between shards: the per-window batch swap is cheap at the measured scale; revisit if
  profiling shows it.
- Destination-side route expansion: phase 4, where it saves network traffic.

## Phase 3: control plane, taps, input (2–3 weeks)

- Control plane on tokio: spawn, add_route, query, inspect, inject, plus the clock commands. Applied at window
  boundaries.
- Name registry (`String → ActorId`, pattern queries).
- Event aggregator: a batched, lossy UI stream and a lossless Arrow IPC file sink (with Parquet conversion).
  Metrics API.
- **System event queue** (`SystemEvent` / `SystemNotice`), including the `CannotKeepUp` warning with hysteresis.
- **Dynamic topology**: connect and prune from the control plane and from actors (`ctx.connect` / `ctx.prune`),
  applied at window boundaries; stable edge indices; destination-set updates; `TopologyChanged` events (07 §5).
- Groups and `multicast`. Spatial index, `broadcast_spatial`, and **shard-local fields** (`ctx.field`) for
  tick-free neuromodulation (07 §3, 09 §2).
- Per-neuron `Modulate` wake-up events for direct-potential modulators, plus threshold-crossing self-scheduling
  (09 §2b). The modulator/receptor configuration table.
- Selectable, event-driven membrane recording (09 §4).
- Live-input ingress stamping, plus an optional input journal.
- **Accept**:
  - A minimal CLI (or test harness) can drive the clock live while streaming spikes at 60 frames/s.
  - The simulation never blocks on a slow subscriber.

### Phase 3 status: done (2026-11-22)

What was built (in `tempo-runtime`; tests in `tempo-testkit/tests/live.rs`, harness in `examples/live.rs`):
- **Control plane** (`Control`, from `rt.control()`): `query`, `update`, `observe`, `inject`, `inject_live`,
  `change_route`, and the clock. Commands go to the shard owning the target and run at window boundaries, and also
  while a shard waits for the clock, so a paused simulation can be inspected and changed. Every request returns a
  `Reply` that can be waited on or `.await`ed.
- **Live input**: `inject_live` stamps input at the clock's time plus `ingress_delay` (default 1 ms). Paced
  windows now follow the clock through idle stretches, instead of jumping to the next event, so live input is
  prompt even when nothing is scheduled. The last shard at each barrier decides the next window's start and end,
  so shards agree on window boundaries that depend on the wall clock. An optional **journal** records injected
  input; replaying it reproduces the live run exactly (tested, on a different shard count).
- **Subscriptions** (`rt.subscribe`, `subscribe_filtered`): per-shard, per-window batches into bounded channels.
  A full subscriber loses batches (counted); the simulation never waits.
- **System events** (`rt.system_events`, `recent_system_events`): `CannotKeepUp` / `CaughtUp` (with hysteresis;
  the clock is not slowed), `LookaheadChanged`, `ActorFailed`, `TopologyChanged` (rate-limited per shard).
- **Panic isolation** (`isolate_panics`): a panicking actor is disabled and reported; others continue.
- **Dynamic topology**: routes live on their owner's shard. `Ctx::change_route` / `Control::change_route` /
  `Runtime::change_route` add or prune edges as timed events, deterministic across shard counts (tested with a
  receiver pruning its own input). A shorter new cross-shard edge lowers the lookahead through the barrier's
  second min-reduction, announced with `LookaheadChanged`.
- **Fields** (`Field`, `add_field`, `Ctx::field`, `Ctx::update_field`): shard-local state updated by timed
  broadcasts and read lazily by actors. This is the tick-free mechanism for neuromodulator concentrations.
- **Broadcasts**: groups (`add_group`, `Ctx::multicast`) and spatial (`set_position`, `Ctx::broadcast_spatial`
  with per-actor delay `base + distance / speed`), sent once per shard and expanded there.
- **Observation flag** (`Control::observe`, `Ctx::is_observed`): for selectable, event-driven membrane recording.

Results:

| Check | Target | Result |
|-------|--------|--------|
| Live control while streaming at 60 fps | works | `examples/live`: speed ×¼ → ×2 → pause/query/step 5 ms → resume, live stimulus, new edges; fast subscriber dropped 0 events, ≤ 0.2 ms per frame |
| Simulation never blocks on a slow subscriber | never | stalled subscriber dropped 542k events; simulation stayed on schedule |
| Live input with nothing scheduled | prompt | delivered within milliseconds when the next event is 1 s away (tested) |
| Replay of a live session | identical | journal replay on 3 shards reproduces the 2-shard live run's trace |
| Fields and broadcasts across shard counts | identical | fields (1, 2, 4, 8 shards), spatial and group broadcasts (1, 2, 3, 6 shards) |
| Performance | no large regression | ping-pong 2.52 ms (phase 2: 2.56), ring 60.5 ms (55), LIF 10k 626 ms (553); 100k LIF 7.0× on 8 shards typical |

Lessons:
- The new broadcast variant first made every queued event 64 bytes instead of 24. That cost 20–50% and the
  8-shard target. Rare variants are now boxed, with a compile-time size check on queue items.
- Pacing tails with 4+ shards vary a lot run to run on macOS (p99 from µs to ~1 ms). Building phase 2 and phase 3
  side by side showed the same spread, so it is machine noise, not a regression.

Decisions and deferrals:
- **No tokio in the runtime.** Replies are both blocking and `Future`s, so async layers can use them directly.
- **Neuron-specific parts moved to the neuron crates**: the modulator/receptor table, threshold-crossing
  prediction for direct-potential modulators (`spikes-model`), and the Arrow file writer, which needs the event
  schema (`spikes-record`, as a subscriber).
- **Typed `ActorRef<K>`**: deferred to the neuron crates, where the real message enums exist.
- Pruning never *raises* the lookahead (conservative). Spawning actors during a run isn't supported (neurons are
  fixed, per decision 10). The control plane's directory snapshot refreshes at each run start and on `rt.control()`.

## Phase 4: multi-host, real-time best effort (4–6 weeks)

The main new capability.

- `Transport` trait. TCP backend (tokio, length-delimited, Nagle off), batched per peer every 50–200 µs.
- Coordinator and node agents: static cluster config, node join, directory replication, placement across nodes.
- Distributed clock schedule (02 §3): segment broadcast with a future effective time, plus clock offset
  estimation (NTP/PTP aware).
- Tier 2 delivery: late messages delivered at `max(at, last_event)`, per-link lateness histograms, run-ahead by
  inbound lookahead `L_in`. Overload produces `CannotKeepUp` / `LatenessHigh` system events, and the clock is never
  slowed automatically.
- Dynamic topology across hosts: destination-set updates sent to the pre-synaptic neuron's node.
- Failure detection: abort the run with a clear error. **Partitioned checkpoint/restore** (needed for scale, 08 §7).
- Per-node links only to peers it exchanges spikes with (no all-to-all assumption); a hierarchical-control interface
  (a single level is fine at first).
- Distributed procedural construction (populations plus connection rules, each node builds its own partition).
- Synthetic-scale test: 10⁸ neurons over simulated nodes, checking that no node holds O(network) state (08).
- **Accept**:
  - On a 2–4 host LAN at `time_factor = 1`, more than 99% of cross-host messages with delay ≥ 1 ms are on time.
  - Lateness drops as `time_factor` rises, matching the 02 §4 estimate.
  - A clock change applies on all hosts within 50 ms.

### Phase 4 status: core done (2026-11-23), with app phase A6

Done: the `Link` interface and node placement in `tempo-runtime`; `tempo-cluster` with a TCP mesh (Nagle off,
batched writes) and an in-memory transport; cross-node delivery of messages, routed spikes, broadcasts, field
updates, route changes and injections; the coordinator-driven window protocol (which is also phase 5's strict mode:
exact, identical traces for any partitioning, paced or not); best effort following a shared clock schedule with
per-link lateness histograms; overload notices forwarded from every node; abort with a clear error when a node fails.
Exactness, pacing, stopping and lateness are tested (`crates/tempo-cluster/tests`, `crates/spikes-session/tests/
cluster.rs`).

Not yet: run-ahead by `L_in`; clock-offset estimation (best effort assumes NTP/PTP); per-node links only to peers
that exchange spikes (the mesh is full); partitioned checkpoint/restore; distributed procedural construction (every
node builds the whole description and keeps its share); the 10⁸-neuron synthetic test; the LAN acceptance runs.

## Phase 5: strict mode and validation (2 weeks)

- Strict cross-node windows (reproducible mode). The node-level window report, all-reduce, `Go(T_next)`.
- Validation experiment: run the same network in strict mode and in best-effort mode, and compare spike statistics
  (rates, ISI distributions, weight trajectories). This confirms that infrastructure lateness acts like benign noise.
- **Accept**: identical traces across hosts in strict mode, plus a written comparison report.

### Phase 5 status: done (2026-11-23)

- Strict mode is the cluster's lockstep (phase 4): node reports with sent counts, the coordinator's decision, and
  no window starts before every event for it has arrived. Identical traces for any partitioning are tested (1 and 3
  nodes, in memory and over TCP, several runs in a row; and a recorded spikes session, paced or not).
- Validation experiment and report: `docs/reports/cluster-validation.md` (reproduce with the `validate_cluster`
  example). A recurrent STDP network run in lockstep, in best effort at 1× and 2× real time, with intentional delay
  jitter, and with another seed. Best effort's late spikes changed rates, ISI distributions and weight trajectories
  less than the intentional jitter, and far less than another seed: infrastructure lateness acts like benign noise.
- Found on the way: a network generated in code couldn't be built on nodes (they parsed its placeholder text); nodes
  now get the parsed description.
- Not done: the experiment across separate hosts (it ran on one machine over loopback TCP).

## Phase 6: scale to 1M+ neurons, then toward 10⁹ (ongoing)

- Compact route and synapse storage (8–12 byte edges, a 20–24 byte synapse state), SoA hot state, prefetching.
- Automatic spatial partitioning across nodes (maximize the cross-host minimum delay, balance event load).
- QUIC transport, `rkyv` zero-copy decode, io_uring on Linux. Per-link lookahead and null messages for strict mode.
- Snapshot/restore at window boundaries. Later, rebalancing and migration through directory epochs.
- Multiple processes on one host (UDS/shm) only if profiling shows a need, for example for NUMA isolation.
- Toward 10⁹ (08): hierarchical coordinators, relay routing for long-range spikes, MPI/RDMA transport, procedural
  connectivity, compressed weights, server-side aggregation and level of detail for the UI, partitioned Parquet
  datasets.
- **Accept (goal)**: 10⁶ neurons × 10³ synapses at 5 Hz on a cluster, interactive at some `time_factor`, with
  reported throughput and lateness.

### Phase 6 status: in progress (2026-11-23)

Milestone 1, big networks on one machine, done:
- **Populations and connection rules** in the description language (`POP`, `RUL`; schema-driven, so the editor
  knows them). A population is a neuron record with a count and a region (box or ball); its neurons are placed at
  random and named `<nam>-<i>`. A rule gives each receiving neuron `cnt` distinct senders, or each sender with
  probability `prb`, optionally only within `rad`, with weights from a distribution. Rules stay rules in the
  description: they're expanded when the network is built, per receiving neuron with its own seeded stream, so
  every node of a cluster builds the same network, keeping only its own neurons' synapses and routes.
- Networks over 500,000 synapses don't list them in summaries, over the API or in the UI (counts instead; a
  neuron's own synapses on request).
- Fixed synapses and unused intrinsic plasticity skip work they don't need (exact).
- Measured (`scale` example, 64 GB, 10 cores, 8 shards): **1M neurons × 100 synapses (10⁸) built in 22 s, 14.4 GB**
  (about 140 B per synapse), running at ~5 Hz at 40 s per simulated second (12 M synaptic events/s). The same
  network listed connection by connection took 4.6 GB per 2×10⁷ synapses just to describe.

Milestone 2, memory and speed per synapse (2026-11-24):
- Done: 16-byte route edges (delays as u32 ns, up to `MAX_ROUTE_DELAY` ≈ 4.29 s) and trimmed synapse lists: **120 →
  90 bytes of heap per synapse** (48 synapse state + 8 sender + 16 edge + overheads), measured with a counting
  allocator in the `scale` example.
- Tried and set aside (kept in `git stash`): a 12-byte form for fixed synapses with dynamic state in a side table.
  Every neuron has stochastic release by default (`SRP`, from v1), so almost no synapse is fixed; in real networks
  it made synapses larger (60 bytes) and runs 25% slower.
- Per-event cost is bound by memory latency: ~220 ns an event when the network mostly fits in cache, ~400 ns when it
  doesn't (one shard). The wheel's bucket width (1–64 µs) changes throughput by ±3%; prefetching the next event's
  actor (inline `prfm`) gained nothing, since the stall is the synapse, whose address is in the actor. Real gains
  need a different layout, e.g. synapses grouped by sender and delivered per spike rather than per synapse.
- Decision: leave memory and per-event cost here for now (10⁸ synapses fit on one machine; 10⁹ needs a cluster
  either way).

Snapshots (2026-11-24):
- Runtime: `Runtime::snapshot` / `Runtime::restore` (feature `serde`) save actor states (`Persist`), fields
  (`PersistField`), the framework's per-actor state (random streams, sequence counters), pending events, route
  edges and the time. Keyed by actor ID, so a snapshot restores onto any number of shards; `Snapshot::merge` joins a
  cluster's parts. Restoring needs the same build (description, seed, actors spawned in the same order).
- Network: `spikes_network::snapshot_codec()`. A network with learning, a modulator and inputs continues exactly
  after a restore, on 1 or 3 shards.
- Session: `Session::save` (when ready: built or stopped) writes the description, build settings, inputs added and
  the snapshot; `Engine::open_saved` / `Session::load_saved` rebuild it on any number of shards and restore it.
  Protocol: `saveRun`, `listSaves`, `openSave` (files in the recordings folder's `saved-runs/`). UI: Save on the Run
  page, Continue under Saved runs on the Recordings page.
- Clusters: each node saves its part (`SaveState`), the coordinator merges them; loading restores the whole
  snapshot on every node, each taking its own actors and events. Inputs carry their start time to every node. A run
  saved on 2 nodes or in one process continues exactly in one process, on 2 nodes or on 3 (`openSave` takes `nodes`;
  the UI uses the Settings page's nodes).
- Saving while paused (one process): `Control::snapshot` has every shard save its part at the window start it
  waits at (a consistent cut: all of them wait at the same one); the run stays paused. On a cluster, stop first.

Milestone 3, preparing for 10⁹ synapses on a cluster (2026-11-24):
- `scale` builds on a cluster too (`… [shards per node] local:N` for N agents in this process, or addresses).
- Memory per neuron was ~2 KB, a third of it the description, which every node holds: populations expanded to a
  full description per neuron, and names were copied about seven times (description, the built network's two
  name maps, the session summary's names and its name map). Now a population is one template and 24 bytes of
  position per neuron (`spikes_dna::Neurons`); names of population neurons are derived (`NeuronNames`), shared by
  the built network and the session summary; parameter tables are filled once per template.
- Measured: 1M neurons' description 0.71 → 0.02 GB, built network 1.99 → 0.75 GB; 400k × 25 synapses 144 → 100
  bytes per synapse in one process, 242 → 114 on 2 nodes and the coordinator; 100k × 100 90 → 79 bytes, same speed.
- 10⁹ synapses (~80–90 GB) don't fit on one 64 GB machine, even split into local node agents: running it needs
  machines with ~100 GB between them.

Cluster and recording work while waiting for machines (2026-11-24):
- Window length: the floor for direct messages was the minimum connection delay (100 µs), capping every window
  there. It's now the network's shortest direct-message delay (inputs 1 ms, modulator latencies): windows 0.1 →
  0.51 ms; 100k × 100 synapses 21% faster on 9 shards, 28% on 2 nodes and the coordinator.
- Placement balances work: the space-filling curve is cut by expected incoming connections, not neuron counts
  (`uneven` example, halves differing tenfold: 4.23 → 2.74 s per simulated second on 8 shards).
- `Runtime::sync_wait`: shards wait 3–5% of their time on 2–4 shards, 14% on 6, 34% on 8 of an M1 Max's 8
  performance cores. The waiting comes from scheduling (other processes, efficiency cores), not from the window
  structure, so per-link lookahead (each pair of shards synchronizing on its own shortest delay) isn't done: in a
  spatial network nearly every shard is within a millisecond of every other, and direct messages reach any shard
  in 1 ms, so a slow shard would still hold the others up. Revisit with measurements from real clusters.
- Saving while paused (see snapshots above).
- Recordings: event tables larger than 4M rows export as partitioned Parquet datasets (`spikes/part-*.parquet`,
  one per span of simulated time, each sorted), converted as a stream in bounded memory.

Next: the 10⁹-synapse network on a cluster (needs the machines).

## Benchmarks to keep from day one

| Name | What it measures |
|------|------------------|
| `queue_*` | insert/pop for each `TimeQueue` implementation with realistic delay mixes (µs synaptic, ms timers, s/h decay) |
| `pingpong_{local,cross_shard,tcp}` | per-hop overhead and latency |
| `ring_N` | timing correctness and throughput |
| `lif_random_{10k,100k,1M}` | events/s, shard scaling, barrier share, idle-skip ratio |
| `fanout_1000` | route expansion plus receiver-side release cost per synaptic event |
| `realtime_10k` | horizon error histogram, and continuity across speed changes |
| `cluster_lateness` | on-time fraction and lateness versus `time_factor` and synaptic delay |

## After the framework (pointers, not part of this plan)

1. Neuron model crate: port the integrators as pure functions, with release moved to the receiving side. Validate
   against v1 runs.
2. `.boo` parser in Rust, using the v1 test networks as conformance tests.
3. `spikes-api` protocol plus the Tauri and axum adapters, then the Tauri UI port. The UI gets a clock control
   (speed slider, pause, step) and a timing-health panel (lateness, horizon lag).
