# Requirements

## Functional

- **F1. Unique identity.** Every actor has a unique `ActorId` that stays the same wherever the actor runs.
  A directory maps IDs to physical locations.
- **F2. Location-transparent send.** One `send` API works whether the target is in the same shard, another thread,
  another process on the same host, or another host.
- **F3. Delayed messages with µs resolution.** `send_after(target, delay, msg)` delivers `msg` at exactly
  `now + delay` in simulation time. Resolution is 1 ns internally, and accuracy is exact in simulation time.
  Delays from 0 up to hours are supported (weight decay half-lives are 100 s, intrinsic plasticity is 1 h).
- **F4. Self-scheduling and timers.** Actors can schedule messages to themselves, which bistable tonic firing needs.
  Cancellation is cheap.
- **F5. Ordered processing.** An actor handles its messages in non-decreasing timestamp order, and `ctx.now()`
  equals the current message's timestamp.
- **F6. Parallel execution.** Many actors execute at once across all cores, and across processes and hosts.
- **F7. Very many actors.** Each actor is lightweight: per-actor overhead should be well under 100 bytes plus its
  own state, with no OS thread, no async task and no heap-allocated mailbox per actor.
- **F8. Fan-out.** Efficient one-to-many sends with a different delay per edge. A typical neuron has 10 to 1,000
  outgoing connections.
- **F9. Control plane.** Spawn actors, connect them, query their state, and start, pause, step, stop and reset a
  simulation. Inspect a running actor (for the UI).
- **F10. Observability.** Actors emit domain events (spike, weight change, …) to taps that batch them for the UI
  and for file sinks, without stalling the simulation.
- **F11. Live, adjustable simulation clock (primary mode).**
  - Real-time interaction: `time_factor` wall seconds per simulated second.
  - The user can speed up, slow down, pause, resume and single-step **while the simulation runs**, and every node
    switches at the same instant.
  - As fast as possible is a secondary mode, for batch runs and reproducible mode.
- **F12. External input.** Inject messages from outside, such as sensors from the UI or files, with a defined
  timestamping policy. They can be recorded for replay.

## Non-functional

- **N1. Timing accuracy is best effort, and errors are measured.**
  - Delays within a node are exact.
  - Cross-host delays are exact whenever network latency allows, otherwise delivered late. Occasional mistimed
    signals are acceptable; timing noise is considered biologically useful.
  - Lateness (late fraction, histogram per link) is always measured and shown in the UI.
  - Intentional noise is a separate, configurable model parameter.
  - **Reproducibility** (same seed and inputs give the same trace) is an *optional strict mode* for debugging and
    regression, not a constraint on the default mode.
- **N2. Throughput.** In-process: at least 20M delivered events per second per host with 8 or more cores for a
  "toy LIF" actor, which is about 50 ns or less of runtime overhead per event per core. See the benchmarks in
  phase plan.
- **N3. Pacing precision.** A node's horizon tracks the clock schedule within 50 µs on Linux and within
  100–200 µs on macOS (hybrid sleep and spin). A clock-speed change takes effect on all nodes within about 50 ms of
  the request.
- **N3b. Cross-host lateness.** At real time on a LAN, more than 99% of cross-host synaptic messages with a delay
  of at least 1 ms are on time. Shorter delays get better as `time_factor` grows (see 02 §4).
- **N4. Portability.** macOS (development) and Linux (clusters). Windows is nice to have.
- **N5. Safety.** No `unsafe` outside small, audited modules (rings, shared memory). Panics in a handler are
  contained to that actor.
- **N6. Testability.** Includes a single-threaded reference executor and a test kit with a virtual clock and
  message assertions.

## Scale targets (confirmed: ultimately 1M+ neurons on compute clusters)

| Tier | Neurons | Synapses per neuron | Mean rate | Synaptic events/s (sim) | Where |
|------|---------|---------------------|-----------|-------------------------|-------|
| Interactive | 10³–10⁴ | 10–100 | 10 Hz | 10⁵–10⁷ | laptop, real time |
| Medium | 10⁵ | 100 | 10 Hz | 10⁸ | workstation or small cluster |
| **Goal** | **10⁶+** | 1,000 | 5 Hz | 5·10⁹ | **multi-host cluster** |
| **Long-term** | **10⁹+** | 10³–10⁴ | 5 Hz | 5·10¹²+ | **hundreds to thousands of nodes** (see [08](08-scaling-to-billions.md)) |

**Design rule (from the long-term tier):** no component may hold state proportional to the *whole* network on a
single node or process, except a few bytes per ID *block*. No protocol may require all-to-all connections or a
central step per event.

Synaptic events outnumber spike events by the fan-out factor. That's why F8 (expand at the destination) matters
more than raw actor-message throughput.

**Sizing the goal tier (estimates):**
- Compute: 5·10⁹ synaptic events/s at about 20–40 ns each is about 100–200 core-seconds per simulated second.
  Real time therefore needs roughly 8–16 hosts with 16–32 cores each. Fewer hosts work at a larger
  `time_factor`, which the adjustable clock makes a normal operating choice.
- Memory: 10⁹ synapses × (about 24 bytes of synapse state + about 8 bytes of route edge) is about 32 GB, spread
  across hosts (about 2–4 GB per host at 8–16 hosts).
- Network: one message per (spike, destination host), not per synapse. At 5·10⁶ spikes/s with each spike reaching
  a few hosts, that's 10⁷ small messages/s cluster-wide, batched into frames. That's well within 10/25 GbE.

## Non-goals (for the first versions)

- Fault tolerance beyond reporting failures. A host failure aborts the run, which can be resumed from a checkpoint
  later.
- Optimistic synchronization (Time Warp, rollback).
- Live migration of actors between hosts. The design leaves room for it.
- Erlang-style supervision trees, hot code reload, or a general-purpose async actor library for web services.
- GPU execution.
