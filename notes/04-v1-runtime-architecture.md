# v1 runtime architecture

## Actors (Akka 2.5, classic)

- **Neuron actors**: `MonostableIntegrator` / `BistableIntegrator` are Akka `FSM`s. Their state data is immutable
  case classes, copied on every event. Connections are `Map[ActorRef, Time]` (post-synaptic ref → propagation
  delay). Weights and STDP functions are `Map[String, …]`, keyed by the pre-synaptic actor's path name.
- **Network** actor: holds the topology (neuron `ActorRef` → location and type), connects neurons, answers queries
  (by regex, by type) and broadcasts `InitializeTime`.
- **NetworkBuilder**: parses a description, creates one `NeuronCreator` per group (locally, or remotely via Akka
  `RemoteScope` deployment), asks each to create its neurons, then sends `Connect` messages.
- **Connection protocol**: the Network sends `Connect(post, weight, eqWeight, distance, learning)` to the
  pre-synaptic neuron. That neuron records the delay and sends `AddPreSynapticRef` to the post-synaptic neuron,
  which stores the weight and the STDP functions. **Synapse state lives on the post-synaptic side. Delay and
  release state live on the pre-synaptic side.**
- **Environment / Sensor** actors: periodically, or on external request, send `Signal(clock.now(), world, value)`
  to input neurons.
- Queries (`WeightsQuery`, `StateQuery`, `TimingQuery`, …) use the ask pattern.

## Time and the scheduler (the most important part for v2)

- `SignalClock(timeFactor, startTime)`: `now() = (wallMillis − startTime) / timeFactor`, which is **integer
  milliseconds** of simulation time. Every neuron holds its own copy, and `InitializeTime`/`SynchronizeTime` align
  start times across hosts (assuming NTP).
- **The timestamp of an incoming signal is the receiving neuron's `clock.now()` when it processes the message**,
  not the time the sender intended. Scheduler error, mailbox queuing, dispatcher contention and GC pauses all end
  up in the physics.
- **Sending a spike**: `delay = round((propagation + rise) in ms) × timeFactor` ms of wall time.
  - If `delay ≥ 200 µs`, it goes through `context.system.scheduler.scheduleOnce(delay)`.
  - Otherwise it's sent immediately, stamped with the fire time.
  - Sub-millisecond delays are therefore **rounded to 0 or 1 ms**.
- `SpikesScheduler` is a copy of Akka's LARS hashed-wheel scheduler with a configurable tick (`tick-duration =
  100000ns`, which is 100 µs, and `ticks-per-wheel = 1024`). Its timer thread sleeps using `Thread.sleep` with
  **ms granularity**. A commented-out alternative wrapped a third-party `HashedWheelTimer` with µs resolution and
  selectable wait strategies (busy-spin, yield, sleep).
- Bistable tonic spiking uses Akka `timers.startSingleTimer` (ms resolution).
- The time factor is the workaround for all of this: a slower simulation means relatively smaller timing error.
  The wiki records raising it from 4 to 10 to get usable results for spiral networks.

**Takeaway**: v1 is a *real-time* simulation whose accuracy depends on how promptly the OS and JVM deliver messages.
It's not reproducible run to run, and timing error grows with load. The `actor-timed` Kotlin experiment (channel
latency measurement plus an empty `HashWheelSender`) shows the start of a fix.

## Distributed mode (wiki 2017-10-01)

- `GRP` entries with `hst`/`prt` place neuron groups on remote hosts. Each remote host runs `spikes-node`
  (`NeuronGroup` → `SystemManager`), which creates an Akka `ActorSystem` per network on a port from a configured
  pool.
- Akka classic remoting over Netty TCP, with Kryo serialization (`KryoInitializer`, custom serializers for squants
  quantities and locations).
- Cross-host time sync depended on NTP plus a `SynchronizeTime` offset.
- Unresolved questions in the wiki: running several networks at once, cleaning up and reusing groups, and dumping
  and reloading network state.
- A TODO in `Network.scala`: route broadcasts through per-group actors instead of messaging each neuron. This is
  the fan-out-per-host idea.

## Event logging

- `EventLogger` sends typed events (`Spiked`, `SignalReceived`, `MembranePotentialUpdate`, `StdpWeightUpdated`,
  `IntrinsicPlasticityUpdated`, `PhaseTransition`, `NetworkTopology`, `ConnectedPostSynaptic`, …) to
  - SLF4J/logback files, as text lines like
    `fire; id: input-1; timestamp: 33.11 ms; signal_intensity: 1.0 mV; last_fire: 0.0 ms`, which the Julia
    analytics parse, or
  - Kafka, as JSON (`SpikeEventsJsonProtocol`). A topic per network/series is consumed by spikes-server and pushed
    down the UI's WebSocket.
- Volume: one sample run produced about 30k receive/update events, 13k learn events and 6.8k fire events, all as
  text. For larger networks, logging becomes a major cost.

## spikes-server (Akka HTTP)

- `POST /network-management/network` with the description returns a network ID.
  `DELETE /network-management/network/{id}` removes it.
- WebSocket at `/web-socket/{id}`, handled by a `NetworkCommander` FSM with states uninitialized → ready → built →
  running:
  - Client → server: `{"command":"build","timeFactor":N}`, `{"command":"start"}`, sensor signals,
    `{"command":"stop"}`.
  - Server → client: build events (neuron created, connection), then a stream of `fire` / `learn` events from Kafka.
- `SeriesRunner` builds N copies of a network (one actor system each) for batch experiments.
