# 02 Protocol (`spikes-api`)

## 1. Envelope

All JSON messages are tagged unions generated from Rust (`#[serde(tag = "type")]`, `ts-rs`):

```ts
// client → engine
type ClientMessage = { id: number; request: Request };
// engine → client
type ServerMessage =
  | { type: "response"; id: number; result: Response | { error: ApiError } }
  | { type: "notice"; session: SessionId; notice: Notice };
```

`ApiError = { code: "parse" | "invalid_state" | "not_found" | "build" | "io" | "internal"; message: string;
spans?: Span[] }`. Parse errors carry `{line, column, length, message}` for the editor.

## 2. Requests

| Group | Requests |
|-------|----------|
| Sessions | `ListSessions`, `CreateSession {name, description, inputs}`, `UpdateDescription {session, description}` (re-parse while `Loaded`), `CloseSession` |
| Build | `Build {seed, shards, jitter_us, record_weights}`, `Reset {seed?}`, `GetNetwork` → neurons (index, name, group, kind, position, inhibitory), synapses (post, index, pre, weight, sign, delay), groups, modulators |
| Run | `Start {speed, until_ms?, record}`, `Pause`, `Resume`, `Step {ms}`, `SetSpeed {factor}`, `Stop` |
| Live input | `Stimulate {neurons, mv}`, `ReleaseModulator {site?, position?, modulator, amount}`, `SetInput {input, enabled, rate_hz?…}` |
| Topology | `Connect {pre, post, weight, sign, learning?}`, `Prune {pre, post}` |
| Inspection | `QueryNeuron {neuron}` → potential, thresholds, refractory state, intrinsic plasticity, phase, synapses with weights and eligibility, modulator levels at its position; `Observe {neurons, on}` |
| Viewing | `Attach {session, view}`, `UpdateView {view}`, `Detach` |
| Recordings | `ListRecordings`, `DeleteRecording`; downloads are plain HTTP (or a file path under Tauri) |

`View = { frame_ms, budget_bytes_per_s, spikes: "all" | {neurons} | {region} | "none", membrane: number[],
weights: "none" | "all" | {neurons}, stats: boolean }`.

## 3. Notices

`StateChanged {state}`, `RunProgress {sim_ms, wall_ms, speed}` (≤ 4/s; frames carry the same at frame rate),
`System {severity, kind, message}` (CannotKeepUp, CaughtUp, LookaheadChanged, ActorFailed, TopologyChanged),
`RecordingFinished {id, tables, bytes}`, `ViewerLagging {dropped_frames}`.

## 4. Frames (binary)

Little-endian, 8-byte aligned sections so the client can wrap them in typed arrays without copying.

```
header (64 bytes)
  magic "SPKF", version u16, flags u16
  session u32, frame_seq u32
  t0_ns u64, t1_ns u64            simulation interval covered
  wall_unix_ms u64
  speed f32, lag_ms f32
  lod u8 (0 exact, 1 counts, 2 binned), section_count u8, reserved
  dropped_events u32, dropped_frames u32
sections: { kind u16, reserved u16, count u32, bytes u32, pad } + payload
  1 spikes exact     neuron u32[count], offset_ns u32[count]          (offset from t0)
  2 spike counts     neuron u32[count], count u16[count] (+pad)
  3 spike bins       cell u32[count], count u32[count]
  4 membrane         neuron u32[count], offset_ns u32[count], mv f32[count]
  5 weights          post u32[count], synapse u32[count], weight f32[count]
  6 transitions      neuron u32[count], offset_ns u32[count], tonic u8[count]
  7 modulator        modulator u16, x,y,z f32, amount f32, offset_ns u32 (per release)
  8 stats            events_per_s f64, lookahead_ns u64, pending u64, shards u16, …
```

Columns, not rows: the raster and the 3D view upload `neuron` and `offset_ns` arrays to the GPU directly.

**Why not Arrow IPC for live frames?** The recording is Arrow, and the UI can open recordings with `apache-arrow`
JS. For live frames a fixed, tiny header is cheaper than an IPC schema per frame (or a long-lived IPC stream per
table and viewer), and the client code is about 100 lines. The section layout matches the recording's columns so
converting between them is mechanical.

## 5. Bandwidth

At 16 ms frames, exact spikes cost 8 bytes each. Examples:

| Network | Rate | Spikes/s | Exact | Counts (per active neuron per frame) |
|---------|------|----------|-------|--------------------------------------|
| 10k neurons | 5 Hz | 50k | 400 kB/s | ≤ 300 kB/s |
| 100k neurons | 5 Hz | 500k | 4 MB/s | ≤ 3 MB/s |
| 1M neurons | 5 Hz | 5M | 40 MB/s | binned (≈ 4096 cells × 8 B × 60 = 2 MB/s) |

So the local app gets exact spikes up to ~100k neurons; remote viewers and larger networks switch to counts or
bins, and zooming (a region or neuron selection) brings exact data back for what's visible.

## 6. Versioning

`spikes-api` has a protocol version, sent in the WS upgrade (`?v=1`) and in Tauri's handshake. Frames carry their
version in the header. Breaking changes bump it; the server rejects mismatched clients with a clear message.
