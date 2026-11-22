# Overview and history

## What Spikes is

Spikes is an **event-based, quasi-real-time spiking neural network** simulator (in the paper draft it's called an
"eSNN"). Its premise is that learning, memory and thought come out of the *dynamics* of continuously active
networks. To capture that, it adds **time and geometry** to the network's topology:

- Every neuron has a 3D location. The distance between two neurons and the axon's conductance speed determine the
  **signal propagation delay** for that connection.
- Time drives the decay of the membrane potential, the decay of connection weights toward equilibrium, intrinsic
  plasticity, synaptic facilitation and depletion, and STDP learning (weight changes that depend on when the
  pre-synaptic and post-synaptic spikes happen relative to each other).
- The simulation is **event-driven**, not time-stepped. A neuron does work only when a signal arrives. On each
  event it decays its state analytically from the last event time to now, then applies the new signal.
- Networks run at a configurable **time factor**: N wall-clock seconds per simulated second. Small networks can run
  at biological real time.

Each neuron is an actor. Signals between neurons are messages, delivered after the propagation delay. That's why the
v2 rebuild starts with an actor framework that supports timed delivery.

## Directory map (`/Users/rob/code/spiking-neural-nets`)

| Path | What it is | Status |
|------|-----------|--------|
| `application/spikes-core` | **Core library** (Scala 2.12, Akka 2.5): neurons, learning, network builder, scheduler, logging | Most mature backend code |
| `application/spikes-utils` | DNA parser (`DnaParser.scala`, about 1,700 lines), validation, coordinate systems | Used by spikes-core |
| `application/spikes-node` | `NeuronGroup` / `SystemManager`: a remote process that hosts actor systems on a range of ports | Distributed mode |
| `application/backend/spikes-server` | Akka-HTTP server: REST to deploy a network, WebSocket per network, Kafka consumer that forwards events to the UI | Backend for the UI |
| `application/spikes-client` | Older Spring Boot/Jersey client that streams Kafka over a WebSocket | Superseded by spikes-server |
| `application/networks/*` | Experiment apps (`wta-simple`, `population-coding`) built on `SeriesRunner` | Experiments |
| `application/old-code/*` | Earlier copies of spikes, parsing, coordinates and descriptions, plus many `.boo` test networks | Historical |
| `prototypes/snn-akka-fsm` | Toy mono- and bistable integrators written with the Akka FSM DSL | Prototype |
| `actor-timed` | Kotlin (2023–24) experiment: coroutine channel latency test plus an empty `HashWheelSender` stub | **First attempt at a new timed actor runtime** |
| `ui/e-spikes-ui` | Electron + React + Redux + Monaco + three.js desktop UI | Latest UI |
| `ui/spikes-ui` | Earlier create-react-app web UI | Superseded |
| `analytics/julia` | `Spikes.jl`: parses neuron-activity logs and plots spike rasters, weights, orbit plots, STDP and WTA learning | Analysis |
| `wiki/` | Dated lab notebook (2017–2018): ring and spiral networks, line sensor, network equations, DNA spec, distributed mode, todo list | Design history |
| `papers/snn-learning` | LaTeX paper draft, "Spikes" (framework and equations) | Draft |
| `papers/external-papers` | Reference papers (SNN overviews, unsupervised learning, benchmarking, etc.) | Reference |
| `notes_bill_goff/` | PDFs from a collaborator: network equations, parameters, winner-take-all | Reference (PDFs not reviewed in detail) |
| `notes/` | Quiver notebooks, mostly Julia setup and real-time plotting experiments | Minor |
| `play-area/rust/learning` | Rust Book exercises | Learning |
| `play-area/rust/horus` | Clone of the third-party HORUS robotics framework (Rust, shared-memory IPC with sub-µs latency) | **Useful reference for shared-memory IPC** |

## Timeline (from file dates and wiki)

- **2016–2017**: Scala/Akka core. Ring, spiral and line-sensor networks. Dynamic connections tried and abandoned in
  favor of "weight stickiness". The time resolution was raised from 1 ms to 100 µs for the spiral experiments by
  increasing the time factor (wiki 2017-05-05).
- **Late 2017**: equations rewritten around the Spike Response Model (SRM). Added stochastic synaptic release
  (Maass facilitation and depletion), noise as Wiener processes, alpha-function rise time, intrinsic plasticity.
  Distributed mode (Akka remoting plus Kafka).
- **2018–2019**: Julia analytics, `SeriesRunner` for batches of runs, spikes-server with Kafka → WebSocket → React UI.
- **2019–2021**: Electron UI (`e-spikes-ui`) with a network editor, a sensor editor and 3D visualization.
- **2023–2024**: `actor-timed` Kotlin experiment, the start of rethinking the runtime around a hashed-wheel timer.
- **2025–2026**: Rust learning, HORUS cloned for reference. v2 (this repo) started.

## Neuron types implemented

- **Monostable integrator** (`nty=mi`): integrates incoming signals and fires once when the threshold is crossed.
- **Bistable integrator** (`nty=bi`): crossing the firing threshold puts it into a *tonic spiking* state, where it
  fires at a set rate using self-scheduled timer messages. It returns to rest when the membrane potential decays
  below the resting threshold.
- Monostable and bistable **resonators** appear in the paper's four-way classification (from Izhikevich) but were
  never implemented.
