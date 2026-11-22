# Risks, decisions and prior art

## Decisions made (2026-11-22)

1. **Primary mode is real-time interaction.** The user can speed up, slow down, pause and step the simulation clock
   while it runs. → Live clock schedule (02 §3). Best-effort cross-host synchronization is the default (02 §4).
2. **Scale: ultimately more than 1M neurons across compute clusters.** → Multi-host is phase 4, not an afterthought.
   One process per host. Spatial partitioning and compact synapse storage (phase 6).
3. **Synaptic delays are accurate on a best-effort basis.** Occasional mistimed signals are fine, and timing noise
   is considered necessary.
   - Exact within a node; best effort across hosts, with lateness measured.
   - Strict reproducible mode is optional.
   - Intentional delay jitter is a separate, configurable model parameter.
4. **Release location: on the receiving side** (confirmed). Explained below.
5. **Message types**: one data-plane enum per simulation, nested by actor kind and including region-level messages.
   Control commands and system (infrastructure) events are separate, framework-defined types. See
   [07](07-message-types-and-dynamic-topology.md).
6. **Dynamic topology**: connections may be added or pruned during a run, by the control plane or by actors
   (structural plasticity). See 07 §5.
7. **Overload policy**: never slow the clock automatically. Post a `CannotKeepUp` warning to the system event queue
   for the UI (07 §4).
8. **Recording format**: Apache Arrow (IPC stream while running, Parquet for archiving). See 03 §9.
9. **Scale**: the design must be able to grow to billions of neurons over time (see
   [08](08-scaling-to-billions.md)).
10. **Fixed neuron set**: neurons are not added or removed during a run, only connections.
11. **Region modulation**: excitation and inhibition modulation, and dopamine-style blasts. They are tick-free:
    shard-local modulator fields read lazily by neurons (see [09](09-neuromodulation-and-synapse-signs.md)).
12. **Membrane potential is recorded on every update** (event-driven, no sampling clock), for a selectable set of
    neurons. Curves between updates are reconstructed analytically.
13. **Connection sign is per synapse**: one neuron can have both excitatory and inhibitory connections.
14. **Neuromodulation scope**: all proposed effects (excitatory and inhibitory gains, threshold, modulated STDP,
    three-factor dopamine learning with eligibility traces) and all major modulators (dopamine, acetylcholine,
    noradrenaline, serotonin, plus generic user-defined ones), through a configurable modulator and receptor
    table (09 §2).
15. **Direct depolarizing/hyperpolarizing modulation is needed early**: per-neuron wake-up events with
    threshold-crossing prediction (09 §2b).

## Explanation of decision 4: where the synaptic release decision happens

**What v1 does** (`MonostableIntegrator.sendSpikesStochastically`, `SignalReleaseProbability`):
When neuron A fires, A loops over each outgoing connection (A→B, A→C, …) and, for each one:
1. computes a release probability `p = 1 − exp(−C·V)`, where
   - `C` (facilitation) rises with each of A's action potentials and decays with `τ_c`. It's tracked once for A.
   - `V` (depletion) drops each time *that particular connection* released, and recovers with `τ_v`. It's tracked per
     connection, in A's `releases` map.
2. draws a random number. Only if it's below `p` does A schedule the signal to B. Otherwise B never hears about
   this spike.

So the decision, and the per-connection depletion state, live on the **sending** neuron.

**Why that clashes with the v2 design**: for 1M+ neurons, the plan sends **one message per spike per destination
host or shard**, not one per synapse. The destination then expands it into its local synapses from a route table.
That's what keeps network traffic manageable: about 1,000× fewer messages for a fan-out of 1,000. But if A decides
per connection *before* sending, the outgoing message has to say which of the 1,000 connections released. That
means either one message per synapse again, or a per-spike bitmask of released edges (about 125 bytes per host
for 1,000 edges, plus A must keep depletion state for every outgoing edge).

**The proposal**: keep the same equations, but evaluate them **at the synapse on the receiving side**:
- Each synapse record (already stored with B, holding weight, equilibrium weight and STDP data) also holds
  that synapse's facilitation sum `C`, depletion sum `V` and their last-update times. That's about 12–16 extra
  bytes.
- Every spike from A is delivered to every target synapse (cheaply, through the expansion). On arrival the
  synapse updates `C` (an action potential reached this terminal), computes `p`, and draws with B's RNG.
  - If released: update `V`, and the signal changes B's membrane potential (and STDP).
  - If not: nothing else happens.

**Why this is reasonable, and arguably more faithful**:
- Biologically, vesicle release happens at the **pre-synaptic terminal**. That's at the *end* of A's axon, which is
  physically at the synapse next to B, not at A's cell body. The action potential reaches each terminal at a
  different time (its propagation delay), so evaluating facilitation and depletion when the spike arrives at each
  synapse matches the physics better than evaluating everything at A's firing time.
- The probabilities and dynamics are the same. Each synapse still has its own depletion history, as in v1.

**What changes in behavior** (small, but you should know):
- Facilitation becomes per synapse instead of shared across A's connections. Every synapse sees every action
  potential, so the values are the same, just evaluated at arrival time instead of fire time (shifted by the
  propagation delay).
- Random draws come from B's RNG stream instead of A's. That's statistically equivalent, but traces won't match v1
  exactly.
- Spikes that won't be released still travel to the destination host. That's cheap, because it's one message per
  host anyway, and the expansion cost per synapse is small.

**Alternative, if you'd rather keep v1 semantics exactly**: A decides per edge and ships a release bitmask with
each spike. That works, but adds bandwidth and sender-side state per outgoing edge. I don't recommend it at 1M+
scale.

## Risks

| Risk | Impact | Mitigation |
|------|--------|-----------|
| **Cross-host latency vs short delays** at real time (a 10 µs synapse vs about 50–200 µs LAN latency) | Short cross-host synapses usually arrive late at `time_factor = 1` | Acceptable as noise (decision 3); spatial partitioning so cross-host synapses are long; run-ahead by `L_in`; raise `time_factor`; lateness shown live in the UI |
| **Infrastructure noise isn't the noise you want** (correlated: bursts of lateness under load, or per-link bias) | Can bias the dynamics instead of acting like random noise | Measure per-link lateness distributions; phase 5 comparison with strict mode; intentional jitter as an explicit model parameter; overload policy that slows the clock instead of degrading silently |
| **Load imbalance** between shards or hosts (busy regions spike more) | A slow shard holds the in-node barrier; a slow host falls behind the clock | Per-shard and per-host load metrics; finer shards than cores; rebalance at snapshots; `CannotKeepUp` warning to the UI (the user decides on speed) |
| **Clock skew between hosts** | Up to `skew / time_factor` extra timing error | PTP on the cluster, coordinator offset estimation, metric reporting |
| **macOS timer precision** | Jittery pacing on laptops | Hybrid sleep and spin; Linux for clusters |
| **Memory at 10⁹ synapses** | About 32 GB total | Compact edge and synapse layouts; distribute across 8+ hosts |
| **Generic-heavy Rust API** | Slow development | One `M`/`E` per runtime; small public API; macro for kind registration if needed |
| **Topology churn** (heavy pruning/adding) | Route tables fragment; destination-set updates add traffic | Free lists with generations; batch changes per window; compact during snapshots; summarize in `TopologyChanged` |
| **Silent overload** (clock not slowed by policy) | Results degrade if the user ignores warnings | `CannotKeepUp` and `LatenessHigh` warnings in the system event queue, shown prominently in the UI; lateness written to the recording |

## Remaining open questions

None blocking phases 0–3. New questions will be added as implementation surfaces them.

## Prior art and build vs buy

- **General Rust actor libraries** (`actix`, `ractor` + `ractor_cluster`, `kameo`, `xtra`, `coerce`) use one async
  task and one mailbox per actor on tokio. None offer timestamped delivery, and tokio's timer has **1 ms**
  granularity. They're fine for the control plane but not the data plane.
- **`nexosim`** (formerly `asynchronix`): a deterministic, multi-threaded discrete-event simulator in Rust. Evaluate
  its scheduler design in phase 1. It's not distributed and not real-time, so it can't be the core.
- **NEST, Arbor, Brian2, GeNN**: NEST's min-delay communication interval and Arbor's epochs are the strict-mode
  design. NEST and Arbor also have published spike-exchange designs worth reading before phase 4.
- **SpiNNaker** (Manchester): a real-time neuromorphic platform with 1M+ cores. It also runs real time with
  best-effort packet delivery and accepts occasional late or dropped spikes. It's the closest philosophical match to
  decision 3, and worth reading.
- **PDES literature**: Chandy–Misra–Bryant, YAWNS (conservative), Time Warp (optimistic, not planned).
- **HORUS** (`play-area/rust/horus`): shared-memory IPC reference, now only for the optional same-host
  multi-process work.
- **v1 `SpikesScheduler` and the `actor-timed` Kotlin stub**: the hashed-wheel idea carries over, driven by
  simulation time.
