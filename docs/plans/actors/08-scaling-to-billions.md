# Scaling path to billions of neurons

Added 2026-11-22. **Requirement**: start at 10³–10⁶ neurons, but the design must allow growth to **10⁹+ neurons**
over time without rewriting the core. This doc lists what has to be right *from the start* (cheap now, expensive
to retrofit) and what can come later.

## Sizing at 10⁹ neurons (estimates)

| Quantity | 10⁹ neurons × 10³ synapses | 10⁹ × 10⁴ synapses |
|----------|----------------------------|--------------------|
| Synapses | 10¹² | 10¹³ |
| Synapse + edge memory (about 32 bytes each, uncompressed) | about 32 TB | about 320 TB |
| Same, compressed (about 8 bytes each: f16 weight, u16 delay, implicit index) | about 8 TB | about 80 TB |
| Synaptic events/s at 5 Hz | 5·10¹² | 5·10¹³ |
| Cores at about 25 ns/event, real time | about 125k | about 1.25M |
| Nodes (64 cores, 512 GB) | about 2,000 (compute-bound) | about 20,000 |

Real time at this scale is a supercomputer workload. With the adjustable clock, a smaller cluster can still run it
at a larger `time_factor`. The design has to work on **hundreds to thousands of nodes**.

## Decisions to make now (built into phases 0–4)

### 1. Directory by ID block, not per actor
The original plan replicated an 8-byte location *per actor* on every node. At 10⁹ actors that's 8 GB on every node,
so it's ruled out. Instead:
- IDs are allocated in **blocks** (for example 2¹⁶ IDs). Each block is assigned to one `(node, shard)`.
- The directory maps a **block** to its location. 10¹⁰ actors is about 150k blocks, about 2.4 MB, replicated
  everywhere and cheap to look up (`id >> 16`).
- Migrated actors (later) go in a small sparse **override map** that is checked only when the block is flagged.
- No central ID allocator. Each node allocates within its own blocks and requests more blocks in bulk.

### 2. Construction is distributed and procedural
- A billion-neuron network can't be listed neuron by neuron in a `.boo` file, or built by a coordinator. The
  network description needs **populations and connection rules**: "N neurons of type X, distributed in this volume
  with this density; connect with probability p(distance), weights ~ distribution". The `.boo` explicit form stays
  for small networks.
- Every node **builds its own partition** from the rules, using seeded RNGs keyed by `(seed, population, index)`.
  The full network is never held centrally. Synapse construction happens on the receiving side (where the synapses
  live, per decision 4). For each local post-synaptic neuron, the node generates its incoming connections using the
  procedural rule plus a spatial query over the (procedurally known) pre-synaptic positions.
- Neuron **names are optional**. Generated neurons are addressed as `(population, index)`, computed from the ID
  block layout. The string name registry only holds explicitly named neurons.

### 3. No central bottlenecks in the control plane
- **Hierarchical control**: the coordinator talks to a tree of sub-coordinators (for example one per rack or 32
  nodes).
  - Clock-schedule broadcasts, commands, metrics aggregation and system events all go up and down the tree.
  - System events are **aggregated and rate-limited** on the way up. The UI sees "37 nodes can't keep up (worst lag
    12 ms)", not 37 messages.
- Queries and commands can target populations, regions or groups, and are executed in parallel where the data
  lives.

### 4. No all-to-all connections
- A full TCP mesh between N nodes is N² connections. That's fine for 16 nodes, impossible for 2,000.
- Spatial partitioning means most traffic goes to **neighboring nodes**. Each node keeps direct links only to the
  nodes it actually exchanges spikes with (known from its routes). Rare long-range spikes go through
  **relay/aggregation routing** (a spike-exchange tree or overlay). The transport trait gets a routing layer
  between "destination node" and "link".
- On HPC clusters, add an **MPI or RDMA (libfabric/UCX)** transport backend behind the same `Transport` trait.
  That's what NEST and Arbor use at this scale.

### 5. Partitioned recording and observation
- Recording is **partitioned Parquet/Arrow datasets** (one file set per node and time chunk, hive-style
  partitioning), queryable with DuckDB, Polars or Spark. There's never a single file.
- The UI can't show 10⁹ neurons. Taps support **server-side aggregation and level of detail**: population firing
  rates, spatial heat maps (voxel-binned activity), sampled raster subsets, and full detail only for a selected
  small region.
  Aggregation runs on each node and is merged up the control tree.

### 6. Compact state from the start
- Synapse and edge structs are designed for compression from day one: f32 weights (f16/u8 option), delays as
  integer µs·k in u16/u32, edge index implicit by position. The structure-of-arrays (SoA) option stays open.
- Plasticity is optional per connection type. Static synapses skip STDP and release state (about 4–8 bytes per
  synapse).
- **Procedural connectivity** option (as in GeNN): for static random connectivity, regenerate a neuron's outgoing
  targets from its seed at spike time instead of storing them. That trades compute for memory, potentially 10× or
  more less RAM.

### 7. Checkpointing is required, not optional
- With thousands of nodes, node failures during long runs are expected. Partitioned snapshots (each node writes
  its own shard states, queues and RNG states at a coordinated sim time) move from "phase 6, later" to a
  **required capability by the multi-host phase**, with restore onto a different node count via block
  reassignment.

## What can come later
- MPI/RDMA transport, relay routing, hierarchical coordinators (the interfaces exist from the start; the
  implementations arrive when needed).
- Procedural connectivity, compressed weights, SoA layouts.
- Dynamic load balancing by reassigning blocks between nodes.
- GPU execution of neuron updates for dense populations (out of scope now, but shard-local arenas of one actor kind
  would map onto GPU batches).

## Built-in checks
- Phase 1 benchmark: bytes per neuron and per synapse are tracked in CI, so they don't creep up.
- Phase 4: a **"synthetic scale" test** uses procedural construction to create 10⁸ neurons spread over simulated
  nodes (several node processes per machine). It verifies that no node holds O(total network) state, by checking
  memory per node against neurons per node.
