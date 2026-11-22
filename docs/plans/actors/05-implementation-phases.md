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

## Phase 5: strict mode and validation (2 weeks)

- Strict cross-node windows (reproducible mode). The node-level window report, all-reduce, `Go(T_next)`.
- Validation experiment: run the same network in strict mode and in best-effort mode, and compare spike statistics
  (rates, ISI distributions, weight trajectories). This confirms that infrastructure lateness acts like benign noise.
- **Accept**: identical traces across hosts in strict mode, plus a written comparison report.

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
