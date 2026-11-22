# Time model and synchronization

This is the core of the design and the biggest change from v1.

> **Revised 2026-11-22 after review.** The primary mode is **real-time interaction with a user-adjustable clock**.
> Target scale is **1M+ neurons across a cluster**. Synaptic delays are **best-effort accurate**: occasional
> mistimed signals are acceptable, and timing noise is considered biologically necessary. So exact global
> determinism is no longer the governing constraint. It remains available as an optional *reproducible mode* for
> debugging and regression tests.

## 1. Simulation time

- `SimTime(u64)` is nanoseconds since the simulation starts. `SimDuration(u64)` is the same unit. There's no
  floating-point drift.
- Delays are computed in `f64` (`distance / conductance_speed + rise`) and rounded to ns once, at connect or send
  time.
- **Every message carries its intended delivery time `at`.** An actor sees `ctx.now()` equal to the timestamp of the
  message it is handling, never the wall clock. That's the fix for v1, where the receive-time wall clock was used.
- **Timing error is explicit.** If a message can't be delivered at `at` (it arrived late from another host), it is
  delivered as soon as possible. The actor sees the *actual* delivery time, and the lateness is recorded. Lateness
  is bounded, measured, and controllable through the clock speed (§5). It isn't hidden in the physics the way v1's
  scheduler jitter was.

## 2. Ordering

- Events are ordered by `(at, src, seq)`, with `seq` a per-sender counter. Ties are broken deterministically. This
  costs nothing, so it stays in every mode.
- Each actor has its own seeded RNG stream, `(global_seed, actor_id)`.
- An actor **never sees time go backwards**. A late message is delivered at `max(at, actor.last_event_time)`, which
  keeps `Δt ≥ 0` in every model equation.

## 3. The simulation clock (user-adjustable)

The mapping between wall time and simulation time is a **piecewise-linear clock schedule** shared by all nodes:

```
sim(w) = sim_anchor + (w − wall_anchor) / time_factor        // time_factor = wall seconds per sim second
```

- **Speed up or slow down**: the coordinator broadcasts a new segment
  `{effective_wall, sim_anchor, time_factor}`. `effective_wall` is slightly in the future (for example 20–50 ms),
  so every node switches at the same instant. `sim_anchor` is computed so the mapping stays continuous, with no
  jump in simulation time.
- **Pause**: a segment with infinite `time_factor`. **Resume**: a new segment anchored at the wall time of resume.
- **Step**: while paused, `step(until)` lets nodes run to `until` and stop.
- **As fast as possible**: no wall mapping. The horizon is limited only by synchronization (used in reproducible
  mode and for batch experiments).
- Nodes need synchronized wall clocks for segments to switch together. Use PTP on a cluster, or NTP plus offset
  estimation by the coordinator. Residual clock offset δ between two nodes becomes timing noise of `δ / time_factor`
  in simulation time. Report it as a metric.

Each node's **horizon** is `H(w) = sim(w_now)`. A shard may process events with `at ≤ H` (plus intra-node
lookahead, §4), and it waits (hybrid sleep, then spin) when it has nothing left before `H`.

## 4. Two-tier synchronization

Synchronization is cheap inside a node and expensive between nodes, so the two get different treatment.

### Tier 1: inside a node (shards on cores): exact, conservative windows

- The shards of one node use the conservative window protocol. The intra-node lookahead `L_node` is the minimum
  delay of any edge between shards *within* the node. All events in `[T, T + L_node)` are processed in parallel. An
  in-process barrier (spin, then park) costs about 0.3–1 µs.
- Cross-shard messages inside a node are therefore **always on time**. Only cross-host messages can be late.
- Windows never start beyond the horizon `H`, and empty periods are skipped.
- Placing neurons by spatial locality makes `L_node` larger and windows fewer.

### Tier 2: between nodes: real-time, best-effort (the default)

- **There is no global barrier.** Each node advances its own horizon from the shared clock schedule.
- Cross-node messages are batched per destination node per small interval, configurable at about 50–200 µs of wall
  time, and sent over TCP or QUIC.
- On arrival, each message goes into the destination's time queue:
  - **on time**: `at` is still in the future for the target actor, so it's delivered exactly,
  - **late**: delivered immediately at the actor's current time. Lateness is
    `delivered_at − at`, recorded per link in a histogram.
- **On-time condition**, roughly: `network_latency + batching_interval ≤ synaptic_delay × time_factor`.
  - Example: a 10 µs synapse at real time (`time_factor = 1`) across a LAN with 100 µs latency arrives about 90 µs
    late. At `time_factor = 20` it is on time.
  - So **slowing the clock directly improves cross-host accuracy**, which suits a user-adjustable clock. The UI
    shows the late fraction and p99 lateness, so the user can see the accuracy/speed trade-off live.
- **Run-ahead margin (optional)**: a node may process up to `H + L_in`, where `L_in` is the minimum delay on any
  edge *into* that node from other nodes. Messages arriving later than that can't affect events before `H + L_in`.
  Doing so sends outputs earlier and gives cross-host messages more slack, at no accuracy cost. Partitioning so
  cross-host edges are long (geometric placement) makes `L_in` larger.
- **Backpressure / overload**: if a node can't keep up (its horizon lags the wall mapping by more than a
  threshold), the clock is **not** slowed. A `CannotKeepUp` warning goes onto the system event queue for the UI
  (see 07 §4). Lateness grows and is reported until the node recovers or the user changes the speed.

### Reproducible mode (optional, for debugging and regression)

- Tier 2 is replaced by **strict global windows**: per-window reports of `{next_event_time, messages sent and
  received per link}`, then a global `Go(T_next)`. The global lookahead is the minimum cross-node delay.
- This gives exact delivery and identical traces for any partitioning. It's slower across hosts, and pacing can't be
  guaranteed. Run it as fast as possible or at a large `time_factor`.
- It uses the same machinery as Tier 1, extended across nodes, so it adds little code. It's useful for
  checking that the best-effort mode's lateness doesn't change the qualitative results.

### Optional noise injection

Since timing noise is wanted, it should be a *model parameter*, not an accident of hardware:

- An optional per-edge or per-neuron delay jitter distribution (for example Gaussian, with σ in µs) is applied at
  send time, using the actor's RNG.
- Transport lateness is still measured separately, so you can tell **intended noise** apart from **infrastructure
  noise**.

## 5. External input and live interaction

- **Live inputs** (UI sensors, hardware streams) are stamped on ingress as
  `at = sim(w_arrival) + ingress_delay`, then injected into the target's node. Optionally they are written to an
  input journal for replay.
- **Recorded or scripted inputs** (files, generators, Rust input actors) carry their own timestamps.
- **Control changes** (clock speed, pause, parameter edits, new connections) are applied at a scheduled simulation
  time (Tier 1 window boundary, Tier 2 clock segment).

## 6. Timers and cancellation

- A timer is a self-message with a delay. It's always local, so always exact.
- Cancellation is lazy: a generation counter in the actor makes stale timers be ignored.
- Convenience: `ctx.set_timer(key, delay, msg)` / `ctx.cancel_timer(key)`.

## 7. Comparison with v1

| | v1 (Akka) | v2 |
|---|---|---|
| Time an actor sees | wall clock on receipt ÷ factor (integer ms) | intended timestamp (ns); actual delivery time if late |
| Delay < 200 µs | sent immediately (delay lost) | exact within a node; best effort across nodes |
| Timing error | hidden, unmeasured | measured per link (late fraction, histogram), shown in the UI |
| Clock speed | fixed at start | changed live (speed up, slow down, pause, step) |
| Intentional noise | none (only accidental) | explicit, configurable delay-jitter model |
| Reproducible | no | optional strict mode |
| Cross-host time sync | NTP + per-neuron offsets | shared clock schedule; PTP/NTP offset → measured noise |
