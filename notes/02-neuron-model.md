# Neuron model and event-driven update

Sources: `spikes-core/.../neurons/{Neuron,MonostableIntegrator,BistableIntegrator,SignalReleaseProbability}.scala`,
`wiki/20171105/notes.md` (SRM equations), `wiki/20171216/dna-defs.md`, `papers/snn-learning/snn-learning.tex`.

## Conventions

- `t*` is the time of the neuron's previous event, and `Δt = t − t*`.
- The resting and reset potential is 0 mV, chosen for convenience.
- Units are handled with `squants` in Scala (mV, ms, µm, m/s, Wb, Hz). v2 needs an equivalent. The simplest option
  is plain `f64` in fixed base units (mV, ms or µs, µm), converted at the parser boundary. The `uom` crate is the
  type-safe alternative.

## The event: a signal arrives at neuron j

A `Signal(timestamp, preSynaptic, value)` comes either from a pre-synaptic neuron (with value ±`spp`, the sign set
by `inh`) or from the "world" (a sensor or environment, which uses weight 1).

1. **Weight update for noise and decay** (connections from other neurons only):
   `w(t) = B_w( w_e + (w(t*) + σ_w(Δt) − w_e) · d_w(Δt) + L )`
   - Weight noise is a Wiener process: `σ_w(Δt) = Ω_w · sqrt(Δt / 1 ms) · N(0,1)`.
   - Decay toward the equilibrium weight `w_e`: `d_w = exp(−Δt/τ_w)` (`WDF=exp`) or 1 (`WDF=zer`).
   - `B_w` is the weight limiter (`WLF=bnd`, a clamp to `[lwb, upb]`, or `unb`).
   - `L` is the learning adjustment from STDP (below).
2. **Membrane potential**: `u = max(u_min, w·s + (u(t*) + σ_u(Δt)) · exp(−Δt/τ_u))`, where the potential noise is
   another Wiener process.
3. **Refractoriness**: inside the absolute refractory period (`t − t_fire < τ_r`), `u = 0`. After it,
   `u −= η₀ / (t − t_fire − τ_r)`, with `η₀` the base refractoriness in Wb.
4. **Intrinsic plasticity** `b` decays as `b · exp(−Δt/τ_b)`.
5. **Fire test**: `u + b ≥ θ`.
   - **Rise-time delay**: the alpha-function response is approximated as linear, so the spike is emitted after
     `τ_rise · (min(θ, u) − u₀) / θ`. This is an extra delay added to each outgoing signal.
   - Reset `u = u_min`. Update intrinsic plasticity: `b += η_b · exp(−(b + b₀))`.
   - **Stochastic synaptic release** for each post-synaptic connection:
     `p = 1 − exp(−C(t)·V(t))`, with facilitation `C(t) = C₀ + α Σ exp(−(t−tᵢ)/τ_c)` over action potentials and
     depletion `V(t) = max(0, V₀ − β Σ exp(−(t−tᵢ)/τ_v))` over releases. Each connection that releases gets a
     signal scheduled after **propagation delay + rise delay**.
   - **Pre-spike STDP**: every signal received since the previous fire contributes a weight increase that depends
     on how long before this spike it arrived. Then the processed-signals buffer is cleared.
6. **No fire**: if the signal arrived after the last fire (and so falls in the post-spike STDP window), apply
   post-spike STDP (a depression), then append the signal to the processed-signals buffer.

### STDP functions (`LRN`)
- `stdp_hard` / `stdp_soft`: exponential windows with amplitudes and periods for inhibition and excitation. Hard
  limits use Heaviside gates at `w_min`/`w_max`. Soft limits scale the change by the distance to the bound.
- `stdp_alpha`: alpha-shaped learning (baseline, time constant, learning rate).
- `flat`: no learning.
- Weight stickiness (`CST=sig`) is a proxy for forming new connections: it slows the decay of large weights.

### Bistable integrator
States are `Depolarized` and `Spiking`. Crossing `lct` (the firing threshold) moves it to `Spiking`. Each tonic
spike is a self-message scheduled at `1/tfr`, which dispatches to all connections. When `u` decays below `rst`
(the resting threshold), it returns to `Depolarized`. This is the main consumer of **self-scheduled timers**, the
other kind of delayed message besides synaptic delays.

## Propagation delay

`delay = distance(pre, post) / csp`, where `csp` is the pre-synaptic neuron's conductance speed. Typical values:
neurons 10–50 µm apart at 1 m/s give **10–50 µs**. This is why v2 needs **µs-resolution delays**: in v1 these
delays were rounded to whole milliseconds, and anything under 200 µs was sent with no delay at all.

## Implications for v2

- The per-event update is a **pure function** of `(state, event time, signal, rng)`. In v2 it should be written as
  plain Rust with no actor-framework types, so it can be unit-tested and benchmarked on its own.
- Every quantity is computed analytically from `Δt`. Event-time accuracy therefore feeds straight into the physics,
  so the timestamp a neuron sees must be the *intended* delivery time, not the time the message happened to arrive.
- Per-neuron state: about 20 scalars plus per-synapse data (weight, equilibrium weight, last release time,
  STDP-function reference) plus the processed-signals buffer. v1 keyed synapses by `ActorRef`/string name. v2
  should key them by dense indices.
- Every random draw (noise, release) should come from a per-actor seeded RNG stream, so runs are reproducible.
- **Decided for v2 (2026-11-22)**: the excitatory/inhibitory sign moves from the neuron (`inh`) to the
  connection, so a neuron can have both kinds of outgoing connection. See
  `docs/plans/actors/09-neuromodulation-and-synapse-signs.md`, which also covers neuromodulation.

## v1 code vs documented model (found while porting, 2026-11-22)

The v1 code differs from the wiki, `dna-defs.md` and the paper in eight places. In brief:
1. The release probability was computed from the neuron's *initial* synapse-timing state, so facilitation and
   depletion never had any effect.
2. Facilitation added its base into the running sum (counted twice).
3. Depletion stored `base − sum` where the sum belongs.
4. Bistable neurons dropped every incoming signal while tonically firing.
5. Bistable neurons had no noise, intrinsic plasticity or stochastic release.
6. Weight stickiness (`CST=sig`) was parsed but never applied.
7. Weight decay used the time since the neuron's last input from any synapse, not since the synapse's own last update.
8. Relative refractoriness was subtracted from the stored potential at every input, so it accumulated.

All eight are fixed in v2 (decided 2026-11-22); details are in `docs/plans/neurons/README.md`.
