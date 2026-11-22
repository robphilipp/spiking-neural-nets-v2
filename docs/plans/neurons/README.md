# Plan: neuron crates

Status: **N1 done** (2026-11-22); N2–N4 to do. Builds on the actor framework (`docs/plans/actors/`, phases 1–3 done).
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
