# Lessons from v1 and what they mean for v2

## Pain points in v1

1. **Timing accuracy depended on wall-clock delivery.** Event timestamps were taken on receipt, delays were rounded
   to ms, and delays under 200 µs were dropped. The time factor was a workaround. Results weren't reproducible.
2. **Too heavy per neuron.** Akka classic actors carry a mailbox, dispatcher scheduling and immutable state copied
   on every message. `Map[String, …]` was keyed by actor path names and the actor system was shared.
   This limited network size.
3. **Fan-out was per synapse.** Every spike sent one scheduled message per post-synaptic neuron, including across
   the network, with Kryo serialization for each one.
4. **Distribution was manual.** It needed per-host config files, a port pool, NTP for time alignment and Kafka for
   event collection. Setup was fragile.
5. **Observability was costly.** Every event went out as a log line or JSON message.
6. **Lifecycle gaps.** There was no state snapshot or restore, no clean reuse of groups, and running several
   networks was awkward.

## Keep

- The neuron and learning model, and the `.boo` language (with Monaco support).
- Geometry-derived delays. They are also what makes parallel execution possible (see "lookahead" in the actor plan).
- Groups as a way to say where neurons run.
- The UI concepts: project, network editor, sensor editor, 3D view, raster, weights chart.
- Julia/Python analytics. Give them a better format, such as Arrow/Parquet event files.

## Change

- **Separate simulation time from wall-clock time.** Every message carries its exact delivery timestamp. An actor
  sees `now()` equal to the message's timestamp, and messages are processed in timestamp order. The wall clock
  only controls *pacing*.
- Lightweight actors: plain structs in per-shard arenas, no per-actor task or mailbox, dense integer IDs.
- Timing error is measured instead of hidden. Delays are exact within a host and best effort across hosts, with
  lateness reported. Some timing noise is wanted (decided 2026-11-22). Deterministic replay is an optional strict
  mode.
- Fan-out at the destination: send one spike per destination shard or host, and expand it there into the local
  synapses.
- Replace Kafka with an in-process event bus, sampled or aggregated for the UI and optionally written to files.

## The v2 web layer (still open)

My recommendation is **library-first, with two thin adapters over one protocol**:

```
 spikes-core (Rust lib: actors + neurons + parser)
        │
 spikes-api (protocol types: commands, queries, event batches; serde + specta/ts-rs → generated TS types)
   ┌────┴─────────────────────┐
 Tauri adapter              axum adapter (spikes-node daemon)
 (commands + Channel events, (HTTP for control, WebSocket or
  backend runs in-process)    WebTransport for event streams)
   └────────┬─────────────────┘
      TS client interface used by the UI (two implementations, same API)
```

- **Local use**: the Tauri app embeds the backend. No server to start, and events reach the UI over Tauri's
  channel/IPC.
- **Remote or distributed use**: the same backend runs headless as `spikes-node` on each host. The UI connects to
  one coordinator over WebSocket. Distributed hosts talk to each other through the actor transport, not through
  the web layer.
- Candidates: `axum` (HTTP + WebSocket, built on tokio) is the safe choice. `tonic`/gRPC is good if non-browser
  clients matter. WebTransport/QUIC is worth considering later for high-rate binary streams.
- Event streams should be binary (for example MessagePack, or a compact custom frame) and batched per frame
  interval.

The actor framework doesn't depend on this choice. It only needs an event "tap" and a control-plane API that
either adapter can call.
