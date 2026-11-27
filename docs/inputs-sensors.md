# Feeding an input file's sensor sources from a program

An input file (`*.inputs.toml`) can have realtime sources of kind `sensor`: a named stream that some other program
feeds while a run goes (an Arduino bridge, a Python script, a filterbank). The UI declares the sensor; your program
claims it by name and sends samples.

```toml
[[source]]
name = "arm"                       # the sensor's name: your program asks for it by this
kind = "sensor"
neurons = { prefix = "motor-" }    # one channel per target neuron, in network order
encoding = { kind = "rate", max_hz = 200, mv = 12 }   # current | rate | threshold | spikes
on_pause = "drop"                  # while paused: drop (default) or buffer what arrives
speed = "resample"                 # or "refuse": only at run speed x1
```

## How it works

1. Build the network and press Start (the Run page). The UI then connects a sensor named `arm` to the session, with
   one channel per target neuron (channel *i* drives the *i*-th target neuron in network order) and the source's
   encoding. Its status on the Run and Inputs pages is "starting: waiting for a program" until samples arrive, then
   "streaming". A run as fast as possible takes no realtime sources (nor does `speed = "refuse"` at another speed).
2. Your program connects to the same server and session and **claims** the sensor by name (`claimSensor` in the
   protocol, `claim_sensor` in `spikes-sensor`). The claim returns a sensor id on *your* connection; binary batches
   (docs/plans/app/02-protocol.md §4.1) go to that id. (Sensor ids belong to a connection, so the UI's id can't be
   used from another connection; that is why a claim exists.)
3. Send batches: `t0_ns` is your clock at the first sample (any origin; keep it increasing), `dt_ns` the spacing,
   values sample-major (`values[s × channels + c]`). The engine anchors your clock to the simulation on the first
   batch (`sim = now + ingress delay + (t - t0) × speed`), keeps your spacing, and counts late batches (it
   re-anchors) and samples dropped while paused. With `encoding = spikes`, send events (channel, offset) instead.
4. The sensor stays connected while any connection holds it. When the run stops, the UI disconnects its handle; stop
   sending (or disconnect yours) too. The next run declares a new sensor of the same name; claim again.

`listSensors(session)` shows `samples`, `inputs`, `batches`, `lateBatches` and `dropped` per sensor.

## Rust, with the `spikes-sensor` crate

```rust
use spikes_sensor::SensorClient;

let mut client = SensorClient::connect("ws://127.0.0.1:7070", None)?;
let session = 0;
// Retry until the run has started and the UI has declared the sensor.
let sensor = client.claim_sensor(session, "arm")?;
let channels = 7u16; // as many as the source has target neurons (see `listSensors`)
let mut t_ns = 0u64;
loop {
    // 10 samples, 1 ms apart, every 10 ms.
    let values = vec![0.5f32; 10 * channels as usize];
    client.send_samples(sensor, t_ns, 1_000_000, channels, &values)?;
    t_ns += 10_000_000;
    std::thread::sleep(std::time::Duration::from_millis(10));
}
# Ok::<(), spikes_sensor::SensorError>(())
```

## TypeScript

```ts
const client = await WebSocketClient.connect("http://127.0.0.1:7070");
const arm = await client.claimSensor(session, "arm");
arm.sendSamples(t0Ns /* bigint */, 1_000_000, 7, new Float32Array(70));
```

## Raw protocol

Over the WebSocket (`/api/ws?v=<protocol version>`): send the text request
`{"id": 1, "request": {"type": "claimSensor", "session": 0, "name": "arm"}}`; the response is
`{"type": "sensor", "sensor": <id>}`. Then send binary input frames (header: sensor id, sequence, `t0_ns`, `dt_ns`,
channels, count; then `f32` values), as in `spikes_api::input::InputBatch`. A program that wants its own sensor
(not one from an input file) uses `connectSensor` with its own targets, as before.
