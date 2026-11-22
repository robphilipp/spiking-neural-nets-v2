# Plan: neuron crates

Status: **N1–N4 done** (2026-11-22). Next: the app-layer plan (server, Tauri UI). Builds on the actor framework (`docs/plans/actors/`, phases 1–3 done).
The v1 model is described in `notes/02-neuron-model.md` and `notes/03-network-description-language.md`.

## Goal

Get an end-to-end simulator on one machine: a `.boo` network description is parsed, built into `tempo` actors,
run (paced or as fast as possible), and recorded, with the same model as v1 except where v1 doesn't match its own
documented equations (see "v1 discrepancies"). This also tests the framework against the real model before the
multi-host work fixes its API in place.

## Crates

| Crate | Depends on | Contents |
|-------|-----------|----------|
| `spikes-model` | none (no `tempo`) | The equations as plain Rust: units, parameters, neuron and synapse state, the per-event update, STDP rules, weight decay/limits/stickiness, release probability, noise, intrinsic plasticity, receptors and modulator effects, threshold-crossing prediction. Pure functions over plain data plus an `RngCore`, so every equation can be unit-tested on its own. |
| `spikes-dna` | `spikes-model` (parameter types) | The `.boo` parser (explicit networks, as in v1) and the network description model. Later: populations and connection rules for procedurally generated large networks (actors plan 08 §2). |
| `spikes-network` | `tempo-runtime`, `spikes-model`, `spikes-dna` | The simulation's message enum (nested by actor kind) and typed actor handles, the neuron actors (monostable, bistable), input actors (Poisson, periodic, file), modulator fields and release actors, and the builder that turns a description into actors, routes, positions and groups. |
| `spikes-record` | `tempo-runtime`, `spikes-network`, `arrow` | Event schema (spikes, weights, membrane, topology, system) and an Arrow IPC writer that plugs in as a subscriber, plus Parquet conversion. |

Later, in a separate plan: `spikes-api` (protocol types and generated TypeScript), `apps/spikes-node`,
`apps/spikes-server`, `apps/spikes-ui` (Tauri).

## `spikes-model` design

- **Units**: plain `f64` in fixed base units, converted only at the parser boundary: mV, ms, µm, m/s, Hz, and
  weights dimensionless. Times between events come from `tempo` as `SimDuration` and are converted to ms once per
  event. (The `uom` crate was considered; plain floats keep the hot path simple, and the parser enforces units.)
- **Parameters are shared, state is per neuron.** A population's parameters (thresholds, time constants, learning
  rule, receptors) live in one table, and each neuron stores an index into it. That keeps a neuron's hot state
  small: membrane potential, intrinsic plasticity, last event and last fire times, refractory end, and the
  signals since its last spike (for pre-spike STDP).
- **Synapses live with the receiving neuron** (decision 4): weight, equilibrium weight, sign (excitatory or
  inhibitory, per synapse, decision 13), last update time, facilitation and depletion sums, eligibility trace,
  and the index of its learning rule. Target size: 24–32 bytes.
- **The per-event update** follows notes/02. It takes the state, event time, synapse (or "world" input) and RNG,
  and returns what happened: no spike, or a spike with its rise delay, plus weight changes to report. The actor
  layer (`spikes-network`) turns that into `tempo` sends and events.
- **Neuron types**: monostable integrator, bistable integrator (tonic firing via a self-scheduled timer). The
  design leaves room for resonators (see open questions).
- **Modulators** (actors plan 09): a configurable modulator table (dopamine, acetylcholine, noradrenaline,
  serotonin, user-defined), per-population receptors mapping modulators to effects (excitatory and inhibitory
  gain, threshold, plasticity, three-factor learning, direct potential), eligibility traces, and threshold-crossing
  prediction for direct-potential modulation.

## v1 discrepancies (found while porting)

v1's code differs from its documented equations (the wiki, `dna-defs.md` and the paper) in a few places:

| # | Where | v1 code | Documented model | Effect in v1 |
|---|-------|---------|------------------|--------------|
| 1 | `MonostableIntegrator.sendSpikesStochastically` | computes the release probability from `synapseTiming`, the constructor's *initial* value, not the updated one | probability from the current facilitation and depletion | release probability never changed: facilitation and depletion had no effect |
| 2 | `SignalReleaseProbability.addActionPotentialEvent` | stores `decayed sum + magnitude + base` as the sum | `C(t) = C₀ + α Σ e^(−(t−tᵢ)/τc)`: the base is added once, when evaluating | base counted twice (masked by #1) |
| 3 | `SignalReleaseProbability.addFireEvent` | stores `max(0, base − sum)` in the field that should hold the sum | `V(t) = max(0, V₀ − β Σ e^(−(t−tᵢ)/τv))` | depletion state wrong (masked by #1) |
| 4 | `BistableIntegrator`, `Spiking` state | only handles its own tonic-spike timer; incoming signals fall through to "invalid message" and are dropped | the prototype README: "while in the spiking state, incoming signals are added to the membrane potential" | inputs lost during tonic firing |
| 5 | `BistableIntegrator` | no noise, no intrinsic plasticity, no stochastic release | same features as the monostable integrator (the paper treats them as shared) | bistable neurons were noise-free and always released |
| 6 | Weight stickiness (`CST=sig`) | parsed and defined (`SigmoidWeightStickiness`) but never applied | slows weight decay to equilibrium | `cst=` had no effect |
| 7 | `Neuron.weightUpdatedForNoise` | decays a synapse's weight by the time since the *neuron's* last input (from any synapse), and again by the inter-spike interval during STDP | decay over the time since that synapse was last updated | rarely-active synapses barely decayed while their neighbours were busy |
| 8 | `Neuron.refractorinessAdjusted` | subtracts `η₀/(s − τr)` from the *stored* potential at every input (and applies it before the first spike) | `u(t) = u_ε + u_η(t)`: a kernel evaluated at the event, not stored | refractoriness accumulated with each input |

**Decided**: implement the documented model, fixing all eight, and record the differences in
`notes/02-neuron-model.md`. Exact reproduction of v1 runs isn't possible anyway (v1's timing came from the wall
clock), so validation compares behaviour (below).

## Validation

- **Unit tests per equation** against hand-computed values from the wiki and paper formulas: membrane decay,
  refractoriness, rise-time delay, weight decay and limits, stickiness, each STDP rule (including the alpha rule's
  zero offset, found by Newton's method in v1), release probability, intrinsic plasticity, noise statistics.
- **Behavioural tests** using v1's own test networks (`application/old-code/spikes/test_data/*.boo`):
  - ring and spiral networks: spikes travel around the ring with the expected period (wiki 2017-04-22 and later)
  - winner-take-all (`simple-wta.boo`): one output wins and inhibition suppresses the others
  - STDP test (`stdp-test.boo`): weight changes have the expected sign and size for pre-before-post and post-before-pre
- **Determinism**: same results for any shard count (the framework guarantees it; these tests keep the model
  honest, e.g. no `HashMap` iteration order leaking into results).

## Phases

| Phase | Deliverable | Accept |
|-------|-------------|--------|
| N1 | `spikes-model`: units, parameters, synapse and neuron state, monostable and bistable updates, STDP, weights, release, noise, intrinsic plasticity | all equation tests pass; one-neuron scenarios behave as documented |
| N2 | `spikes-dna`: `.boo` parser and description model | parses every v1 test network; ports `DnaParserTest` cases; good error messages with line and column |
| N3 | `spikes-network`: message enum and typed refs, neuron and input actors, builder, modulator fields and receptors | ring, WTA and STDP networks behave as in the wiki; identical results across shard counts |
| N4 | `spikes-record`: Arrow writer as a subscriber, Parquet conversion | a run's spikes and weights load in Python (`pyarrow`) and Julia (`Arrow.jl`) |

Then the app layer (server, Tauri UI) gets its own plan.

## Decisions (2026-11-22)

1. Implement the documented model, fixing all eight discrepancies.
2. Add resonators now.
3. Add homeostatic intrinsic plasticity now (off by default).

## N1 status: done (2026-11-22)

`crates/spikes-model` (depends only on `tempo-core` and `rand_core`), 29 tests:

- **Integrators**: as documented, with the kernel form of refractoriness (#8). The bistable integrator keeps
  integrating input while tonic (#4) and has noise, intrinsic plasticity and release (#5); STDP applies at every
  tonic spike.
- **Monostable resonator**: resonate-and-fire (Izhikevich). Below threshold, `z = x + i·u` evolves as
  `z·e^((b + iω)t)` (closed form between events, no ticks); inputs kick `u`; firing when `u + η + b ≥ θ`. When the
  oscillation will cross threshold *after* an input, the neuron computes the exact time (`crossing::damped_oscillator`)
  and asks for one wake-up then.
- **Bistable resonator**: rests as a resonator; at threshold it moves onto a limit cycle of radius `cycle_mv`
  and fires once per cycle at its peak. A pulse that leaves the state inside the separatrix radius returns it to
  rest; otherwise it stays on the cycle at the new phase. So only well-timed pulses switch it off (tested).
- **Synapses**: decay per synapse (#7), noise, limits, stickiness (#6), sign, Maass release (#1–3).
- **STDP**: constant-memory traces. Hard and soft rules are exactly v1's sums; the alpha rule is exact except
  that v1's per-term clamp is applied to the total.
- **Intrinsic plasticity**: v1's LTP-IE plus homeostasis (`db/dt = κ(r* − r)`, rate trace with window `τ_r`,
  integrated exactly between events, bounded).
- `crossing::two_exponentials`, ready for direct-potential modulators in N3.

New parameters the parser (N2) must accept: resonator frequency and damping, bistable-resonator cycle and
separatrix radii, homeostasis target/strength/window/bounds, and a per-connection sign.

## N2 status: done (2026-11-22)

`crates/spikes-dna`: a hand-written parser (syntax tree with line and column for every value) plus a semantic
layer that converts units, applies defaults, expands id sets and checks references. All problems are reported
together, each with its position.

- **Conformance**: all 26 v1 networks (`test-data/v1/`, copied from the v1 repo: `old-code/spikes/test_data`,
  the 2019 `simple-wta.boo`, `pop-coding.boo`, and the Electron UI's test networks) parse. v1's `DnaParserTest`
  cases are ported (six neurons, eight expanded connections, learning functions, invalid parameters rejected).
- **Lenient where v1 was strict**: missing neuron keys get v1's documented defaults, so 2017-era files (without
  `ipb`, `ipl`, `ipd`, `wnm`) parse; older names `fnc=stdp` (= `stdp_hard`), `SNT` (= `SRP`) and `uwb`
  (= `upb`) are accepted. Without a `GRP` section, neurons run locally in the groups they name.
- **Random weights** (`cnw=r0:1`) are kept as ranges and drawn from the network seed at build time (v1 used an
  unseeded `math.random`).
- **v2 keys**: `nty=mr|br` with `RES=(frq, dmp, lcr, sep)`; `HIP=(tgt, str, win, lwb, upb)`; per-connection
  `typ=exc|inh`; `nam=` on learning and stickiness functions; bare sets (`psn={a, b}`).
- Locations are converted to cartesian µm (cylindrical `(r, φ, z)`, spherical `(r, φ, θ)`, as in v1).

## N3a status: done (2026-11-22)

N3 is split: **N3a** (network actors and builder, done) and **N3b** (neuromodulation: receptors, gains,
three-factor learning, direct-potential modulators, including the model-side hooks).

`crates/spikes-network`:
- **Messages**: `SimMsg` nested by actor kind (`NeuronMsg`, `InputMsg`); typed handles `Ref<T>` (`NeuronRef`,
  `InputRef`) and `Ctx::tell`, which only accepts the target kind's messages (actors plan 07 §2).
- **Events**: `Spike { at }` (including the rise time), `Transition`, `Weight { synapse, pre, before, after }`
  (optional), `Membrane` (observed neurons only).
- **Neuron actor**: a thin adapter to `spikes-model`; model wake-ups become `Timer`-guarded self-messages, spikes
  become `fanout_at(now + rise)`.
- **Inputs**: `PoissonInput`, `PeriodicInput`.
- **Builder**: shared, deduplicated neuron and synapse parameter tables; spatial (Morton) placement; route delays
  `distance / conduction speed` (with an optional minimum); synapse parameters combine the receiver's
  `WDF`/`WLF`/`SRP`/`wnm` with the connection's learning and stickiness; seeded random weights; names → `NeuronRef`.

Validation on v1 networks:
- `ring.boo`: groups fire in order, group 1 at its 2 ms rise time, then 0.15–0.5 ms per hop.
- `stdp-test.boo`: exact potentiation (+0.015) and depression (−0.035·e^(−10.05/40)) predicted from geometry.
- A bistable neuron fires tonically at 50 Hz through runtime wake-ups and returns to rest when predicted.
- `line-sensor.boo` with Poisson and periodic input: identical spikes and weight changes on 1, 2 and 4 shards.

**Finding: v1's ring results were probably delivery artefacts.** With `ring.boo`'s parameters, a lap takes about
1 ms, inside the 20 ms refractory period, so activity stops after one lap (25 spikes). The wiki (2017-04-22)
recorded ~50 ms per hop and sustained circulation. That timing matches v1's millisecond-rounded, wall-clock
message delivery rather than the model. For sustained circulation in v2, the ring needs longer delays (larger
distances or slower conduction) or a shorter refractory period.

## N3b status: done (2026-11-22)

Neuromodulation, end to end (actors plan 09, decisions 11, 14, 15):

- **Model** (`spikes-model::modulation`): receptors with excitatory gain, inhibitory gain, threshold, plasticity,
  three-factor learning and direct potential, each linear in concentration; effects of several receptors combine.
  Concentrations come through a `Modulators` trait. Three-factor learning: STDP changes go to a per-synapse
  eligibility trace, and the weight changes by `rate·∫ e(s)·c(s) ds`, computed exactly from the release history
  when the synapse is next updated. Direct potential is added to the threshold test; integrators predict
  modulator-driven crossings and wake themselves (`crossing::first_crossing` for several modulators).
  `Model::on_modulation` re-evaluates a neuron when a modulator arrives.
- **Language** (`spikes-dna`): `MOD=[(nam, clr, rad, spd, lat, len)]`, neuron `RCP=[(mod, exg, ing, thr, pla,
  elg, rat, dir)]` and `REL=(mod, amt)`.
- **Network** (`spikes-network::modulators`): a shard-local `ModulatorField` of releases; each neuron reads
  concentrations lazily at its position (arrival `t + latency + d/speed`, amplitude `amount·e^(−d/len)`, clearance
  decay). Neurons with `REL` release when they spike; `Network::add_release_site` adds a release point that the
  UI can trigger. Modulators with direct-potential receptors also wake the neurons they reach (spatial broadcast).

Tests: each effect on a single neuron; exact three-factor integral; reward learning in a network (no dopamine,
no learning; dopamine 100 ms after the pairing strengthens the synapse; same on 1 and 2 shards); a direct modulator
firing the neuron it reaches at exactly `release + latency + distance/speed`, not one beyond its reach; a neuron
releasing a modulator when it spikes.

Approximations, documented in the code:
- Three-factor weight changes are applied when the synapse is next updated, then decay toward equilibrium over the
  whole interval since its previous update.
- Releases are kept for 60 s (or 20 clearance times); a synapse idle longer loses older contributions.
- Predictions of direct-potential crossings hold the refractoriness kernel at its current value.

## N4 status: done (2026-11-22)

`crates/spikes-record` (arrow and parquet 60.2, IPC and zstd only):

- `Recorder::start(dir, rt, network, description, run, options)` writes the static tables (`run.arrow`,
  `neurons.arrow`, `synapses.arrow`, as Arrow IPC files), then a writer thread streams `spikes`, `weights`,
  `membrane`, `transitions` and `system` to Arrow IPC stream files (`.arrows`) in batches of 65,536 rows;
  `finish()` closes them. Schemas use `t_ns: u64` and actor ids `u64`, with a format version and units in the
  schema metadata.
- Events come from a new **lossless subscription** in `tempo-runtime` (`Runtime::subscribe_lossless`): an
  unbounded per-window batch channel. The simulation still never waits; if the writer falls behind, memory grows.
- `to_parquet(dir)` converts every table to zstd Parquet, sorted by time with ids breaking ties, so the files
  don't depend on the shard count (tested: 1 and 3 shards give identical spike and weight tables).
- `read_table(path)` reads a table back; `examples/record.rs` records a `.boo` network driven by Poisson input.

Tests: row counts equal the runtime's buffered events for every table, with tiny batches; spike times and weight
changes match exactly; static tables (names, positions in µm, kinds, synapse endpoints, seed, description,
format metadata); Parquet sorted and shard-independent.

Checked outside the tests: a 10 s recording of `stdp-test.boo` loads in Python with `pyarrow` 26 (IPC streams,
IPC files and Parquet, with schema metadata). Julia was not checked: the installed Julia is an x86_64 build and
this machine has no Rosetta.

Not yet recorded: topology changes (connections added or pruned during a run) and modulator releases; both need
events that the network doesn't emit yet.
