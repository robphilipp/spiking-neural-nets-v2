# Plan: timed actor framework (Rust)

Status: **draft, revised after first review**, 2026-11-22. The decisions recorded in
[06](06-risks-open-questions-prior-art.md#decisions-made-2026-11-22) are: real-time interaction with an adjustable
clock is primary; target is 1M+ neurons on a cluster; synaptic delays are best-effort; occasional timing noise is
acceptable and even desirable.
Background on the v1 system is in `../../../notes/` (especially `04-v1-runtime-architecture.md` and
`06-lessons-and-v2-implications.md`).

## The idea in one paragraph

Build a **timestamped-message actor runtime**. Every message carries the exact simulation time at which it must be
delivered (ns integers, µs-or-better resolution). Each actor processes its messages strictly in timestamp order and
sees `now()` equal to the timestamp of the message it is handling. Actors are plain structs stored in per-core
**shards**. A shard is a single-threaded event loop with its own time-ordered queue, so an actor costs a few
hundred bytes and no task or mailbox. Synchronization has **two tiers**:
- **Inside a node**, shards use exact conservative time windows, bounded by the smallest delay between shards (the
  *lookahead*, which comes from axon propagation delays).
- **Between nodes**, there is **no global barrier**. Every node follows a shared, user-adjustable clock schedule,
  and cross-host messages are delivered on time when the network allows. Otherwise they are delivered slightly late,
  and the lateness is measured and shown as timing noise. Slowing the clock makes it smaller.

An optional strict mode extends the exact windows across nodes for reproducible runs. Every actor has a stable
`ActorId` that a replicated directory maps to `(node, shard, slot)`.

## Documents

| Doc | Contents |
|-----|----------|
| [01-requirements.md](01-requirements.md) | Goals, non-goals, scale targets |
| [02-time-model-and-synchronization.md](02-time-model-and-synchronization.md) | Simulation time, ordering, lookahead windows, pacing, external inputs |
| [03-architecture.md](03-architecture.md) | IDs and directory, actor storage, shard runtime, time queues, fan-out, transports, control plane, observability, failures |
| [04-api-sketch.md](04-api-sketch.md) | Proposed Rust API with a neuron example |
| [05-implementation-phases.md](05-implementation-phases.md) | Milestones, deliverables, acceptance criteria, benchmarks |
| [06-risks-open-questions-prior-art.md](06-risks-open-questions-prior-art.md) | Decisions, risks, remaining questions, build vs buy |
| [07-message-types-and-dynamic-topology.md](07-message-types-and-dynamic-topology.md) | Data-plane message enum, region messages, system events, adding and pruning connections during a run |
| [08-scaling-to-billions.md](08-scaling-to-billions.md) | What must be right now so the design can grow to 10⁹+ neurons |
| [09-neuromodulation-and-synapse-signs.md](09-neuromodulation-and-synapse-signs.md) | Neuromodulation explained and modelled tick-free; per-synapse sign; membrane recording; fixed neuron set |

## Key decisions (proposed)

1. **Simulation time is logical**, `SimTime(u64 ns)`, carried on every message. It is never read from the wall clock
   inside an actor. Late delivery is explicit and measured.
2. **A live, adjustable clock**: a piecewise-linear wall↔sim schedule shared by all nodes, supporting speed up,
   slow down, pause and step.
3. **Shard-per-core and shared-nothing.** Actors live in typed arenas inside a shard. There's no lock on the hot
   path.
4. **Two-tier synchronization**:
   - exact conservative windows inside a node,
   - real-time best-effort between nodes (no global barrier), with an optional run-ahead margin equal to the
     inbound lookahead,
   - an optional strict global mode for reproducibility.
   Events are ordered deterministically by `(time, sender, seq)` within every queue.
5. **Two planes**:
   - The *data plane* carries timed, ordered, high-volume messages.
   - The *control plane* carries untimed async request/response (build, connect, query, start/stop) and applies
     changes at window boundaries.
6. **One transport trait** with three backends: in-process SPSC rings, same-host shared memory (UDS first), and
   network (TCP first, QUIC later).
7. **`u64` ActorIds allocated in blocks**, plus a replicated directory `block → (node, shard)`. String names (such as
   `input-11`) live only in a control-plane registry.
8. **Fan-out at the destination**: a multicast sends one message per destination node and shard, which expands it
   through local route tables with per-edge delays. Per-synapse release state (facilitation and depletion) lives
   with the synapse on the receiving side (see 06, "Explanation of decision 4").
9. The framework is **generic** and doesn't depend on neurons. Neurons are just one actor kind built on top of it.
10. **Multi-host is a first-class target.** One process per host with many shards is the normal deployment.
    Multiple processes on one host is lower priority.
11. **Billions of neurons is the long-term target.** The decisions that would block it are made now: a block-based
    directory, distributed procedural construction, hierarchical control, no all-to-all links, partitioned
    recording, checkpointing. See [08](08-scaling-to-billions.md).
12. **No global ticks anywhere.** All state (membrane, weights, modulator concentrations, eligibility traces) is
    computed analytically at event times. See [09](09-neuromodulation-and-synapse-signs.md).
