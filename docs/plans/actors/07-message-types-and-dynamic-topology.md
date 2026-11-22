# Message types, system events and dynamic topology

Added 2026-11-22.

## 1. Four kinds of traffic

"One message type" applies only to the **timed data plane**. It is *not* limited to neuron messages. Region-wide
messages belong there too, because they also happen at a point in simulation time. Infrastructure messages are
separate and owned by the framework.

| Kind | Who defines it | Timed? | Direction | Examples |
|------|----------------|--------|-----------|----------|
| **Data-plane messages `M`** | the simulation (one enum per simulation) | yes: `at`, ordered | actor → actor(s) | spike, tonic tick, sensor input, **neuromodulator pulse to a region**, region-level inhibition |
| **Control commands** | framework, plus a user extension enum | applied at a window boundary or a scheduled sim time | control plane → runtime and actors | build, spawn, connect, prune, set parameter, query, clock speed, pause |
| **System events `SystemEvent`** | **framework** (fixed enum) | stamped with sim time and wall time | runtime → subscribers (UI, logs, optionally actors) | **"can't keep up"**, lateness alert, node joined or lost, lookahead changed, actor failed |
| **Domain events `E`** | the simulation | stamped with sim time | actors → taps (UI, files) | spike emitted, weight changed, membrane sample |

## 2. Why the data plane uses one message enum per simulation

Each shard's time queue stores `Envelope<M>`. The alternative, a different message type per actor kind, forces
type erasure: every message becomes a `Box<dyn Any>`. That costs a heap allocation and a downcast per event, about
20–50 ns, plus a registry of type tags to serialize across hosts. At 10⁹+ events per second, that overhead is the
whole budget. With one enum:
- **no allocation**: messages are moved inline,
- **fixed size**: queue entries are compact and cache-friendly,
- **serialization comes free** from `#[derive(Serialize, Deserialize)]`, the same on every host,
- **static dispatch**: a `match` per message.

### Type safety: nest the enum by actor kind

```rust
#[derive(Clone, Serialize, Deserialize)]
pub enum SimMsg {
    Neuron(NeuronMsg),
    Region(RegionMsg),
    Sensor(SensorMsg),
}
#[derive(Clone, Serialize, Deserialize)]
pub enum NeuronMsg { Spike { amplitude_mv: f32 }, TonicTick { gen: u32 }, External { mv: f32 },
                     Modulate { kind: Modulator, level: f32 } }   // delivered by a region broadcast
#[derive(Clone, Serialize, Deserialize)]
pub enum RegionMsg { Release { kind: Modulator, amount: f32 }, Inhibit { level: f32 } }
```

- A typed handle `ActorRef<K>` only accepts `K::Msg`, so `ctx.send(neuron_ref, NeuronMsg::Spike{…})` compiles and
  `ctx.send(neuron_ref, RegionMsg::…)` doesn't. Conversion into `SimMsg` happens inside `send`.
- Each actor kind's handler receives its own sub-enum. The framework unwraps the outer variant. A mismatched
  variant (which should be impossible through typed refs) is counted and dropped.
- Keep the enum small, 32 bytes or less. Large or rare payloads go through `Arc<…>` or are looked up by ID.
- The framework stays generic over `M`. It never knows about neurons.

## 3. Region-level messages

Messages that affect many neurons at once, such as neuromodulation, region-wide inhibition or a stimulus to a
volume, use the same fan-out machinery as spikes. The difference is that the target set is defined by *group* or
*geometry* instead of by synapses:

- **Groups**: named actor sets (for example a `.boo` group or a "layer"). `ctx.multicast(group, msg, delay)`, or
  from the control plane with `inject_group(...)`, sends one message per shard that hosts members. Each shard
  expands it locally.
- **Spatial broadcast**: `ctx.broadcast_spatial(center, radius, propagation_speed, msg)`. Each shard holding actors
  in that volume uses its local **spatial index** (a grid or k-d tree, built from neuron locations) to find
  members. It computes a delay for each one, `distance / speed`, which models diffusion or volume transmission
  naturally.
- **Shard-local fields**: a broadcast can update per-shard state (for example a modulator field) instead of
  waking actors. Actors read it lazily through `ctx.field(...)` at their next event. This is the tick-free
  mechanism used for neuromodulation; see [09](09-neuromodulation-and-synapse-signs.md).
- **Region actors**: an optional actor that owns region state (for example a dopamine concentration that decays
  over time). It receives messages like any actor, then broadcasts to its region.

## 4. System events (infrastructure)

These are framework-defined and separate from `M`, because they are about the runtime, not the model:

```rust
pub enum SystemEvent {
    CannotKeepUp { node: NodeId, horizon_lag: Duration, requested_time_factor: f64 },  // warning; clock is NOT slowed
    CaughtUp { node: NodeId },
    LatenessHigh { link: (NodeId, NodeId), late_fraction: f32, p99: SimDuration },
    ClockChanged { segment: ClockSegment },
    NodeJoined { node: NodeId }, NodeLost { node: NodeId, reason: String },
    LookaheadChanged { node: NodeId, old: SimDuration, new: SimDuration },
    ActorFailed { id: ActorId, error: String },
    TopologyChanged { added: u64, pruned: u64 },   // summary per interval, not per edge
    TapOverflow { tap: TapId, dropped: u64 },
}
pub struct SystemNotice { pub sim_time: SimTime, pub wall_time: SystemTime, pub severity: Severity, pub event: SystemEvent }
```

- They go onto the **system event queue**, a bounded broadcast channel on the coordinator. Every node forwards its
  local notices there. The UI subscribes through the web or Tauri adapter. Notices are also written to the run
  recording.
- **Overload policy (decided)**: when a node can't keep up, the clock is **not** slowed. A `CannotKeepUp` warning is
  added to the queue (rate-limited, with hysteresis), and `CaughtUp` follows when the node recovers. Meanwhile,
  lateness grows and is reported. The user decides whether to slow the clock.
- Actors can **opt in** to selected system events (`on_system(&SystemNotice)`), for example a region actor that
  adapts when the network is overloaded. They are delivered at the next window boundary. By default actors never
  see them.

## 5. Dynamic topology: adding and pruning connections during a run (decided: required)

### Who initiates
- **Control plane** (UI or scripts): `connect(pre, post, params)`, `prune(pre, post)`, and bulk variants.
- **Actors themselves** (structural plasticity), for example B prunes its synapse from A when the weight has
  stayed near zero: `ctx.connect(...)` / `ctx.prune(edge)`. These become topology commands applied at the next
  window boundary on the shards involved.

### Where the data lives
Synapse state and route-expansion entries both live on the **receiving** side (decision 4), on B's shard. So adding
or pruning A→B is mostly a **local operation on B's shard**:
1. Add or remove B's synapse record and the `(target slot, delay, edge index)` entry in the expansion table for A,
   both on B's shard.
2. Only if this is the *first* edge from A into that shard (or the *last* one removed) does A's
   **destination set** change. That's a small control message to A's node: "start or stop sending A's spikes to
   shard S".

### Consistency rules
- Changes apply at a window boundary on the receiving shard, at a defined sim time `t_apply`.
- Spikes already in flight for a pruned edge are expanded against the current table, so a pruned edge simply stops
  receiving. No special handling is needed. For a new edge, spikes from A sent before A's destination set updates
  may miss it, which is acceptable (decision 3).
- **Stable edge indices**: synapse slots are reused through a free list with a generation counter. Indices stay
  stable for live edges. Compaction happens only during snapshots.
- **Lookahead**: a new short edge *within* a node can shrink `L_node`. The new value takes effect at the next window
  boundary, and a `LookaheadChanged` event is emitted. Across nodes there's no barrier, so a short new cross-host
  edge only affects lateness.
- Topology changes are recorded as domain events (`EdgeAdded`, `EdgePruned` with times) in the run recording, so
  the network's structure can be reconstructed at any point in time.
