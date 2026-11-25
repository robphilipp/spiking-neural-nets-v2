# The network description language, restructured

Status: done (2026-11-25): language, builder, migration (`spikes migrate`), placement on nodes, editor. Replaces the v1-derived language (`spikes-dna`, `.boo` files). No compatibility with
the old form: a one-time `spikes migrate` converts old files, and the test networks are converted with it.

## Why

A review of the current language found:

1. **No reuse.** Every listed neuron repeats every parameter (`ring.boo`: 12 keys × 25 neurons, differing only in
   name, position and one `mpd`; `line-sensor-layers-wta.boo`: ~20 keys × 69 neurons). Populations share
   parameters, but only for randomly placed neurons.
2. **Two half-mechanisms each for neurons and connections.** `NRN` (one by one, explicit positions) vs `POP`
   (random placement only); `CON` (always all-to-all between id sets) vs `RUL` (random fan-in or probability only),
   with different keys for the same things (`nid`/`nam`, `prn`/`prp`). No one-to-one pattern, no structured
   placement (grids, lines, rings), so the v1 files compute ring coordinates by hand.
3. **A synapse's settings live in three places.** Decay, limits, release and weight noise on the *receiving
   neuron*; weight, learning, stickiness, sign and delay on the *connection*; delay and sign on the *sending
   neuron*. So all of a neuron's inputs share one decay, limit and release model, and release was on for every
   neuron by default (which also kept synapses from the compact storage).
4. **Structure lives in name patterns** (`input-1{1:7:1}`), and groups (`GRP`) are left over from v1's host/port
   distribution.
5. **Cryptic keys** (`mst`, `mpd`, `rfp`, `spp`, `csp`).

The syntax itself (records, lists, numbers with units, id ranges, comments) is good, and the editor's completions,
hover help and diagnostics are generated from the schema. It stays, with two small additions (points and
`uniform(…)`).

## The model

Six kinds of things, each named and referred to by name:

| Section | What |
|---|---|
| `neuron_types` | everything about a neuron except its name, position and node |
| `synapse_types` | everything about a synapse: weight, sign, delay, learning, stickiness, decay, limits, release, noise |
| `neurons` / `populations` | neurons: a type (plus overrides), positions, and optionally a node |
| `sets` | named sets of neurons, for connections and placement |
| `connections` | from a set of neurons to another, with a pattern and a synapse type (plus overrides) |
| `learning_rules`, `stickiness`, `modulators`, `nodes` | as before (`nodes` is new) |

**Types and overrides.** A neuron type or synapse type can `extend` another. A neuron, population or connection
names its `type` and may set any of the type's keys itself, which then replace the type's. A nested record (e.g.
`decay=(…)`) is replaced whole, not merged (its kind decides which keys apply). Everything not set takes the
documented default.

**Units.** As now: a number may carry a unit (`5 mV`, `2 s`, `40 µm`); without one it's in the base unit (mV, ms,
µm, m/s, Hz).

## Syntax additions

- **Points**: a list of three numbers, `[40, 0, 10]` (cartesian x, y, z in µm), or a record for other coordinate
  systems: `(r=40, phi=90 deg, z=10)` cylindrical, `(r=40, theta=30 deg, phi=90 deg)` spherical. The coordinate
  system follows from the keys (replacing `LOC=(cst=…, px1=…, px2=…, px3=…)`).
- **Random values**: `uniform(0, 2)` (replacing `r0:2`); `normal(1, 0.1)` too.

Everything else is unchanged: `(key=value, …)` records, `[…]` lists, `//` and `/* */` comments, id ranges
(`input-1{1:7:1}`, `{a, b}`).

## Keys

All keys are lower-case words with underscores. Old names in brackets.

### `nodes` (new)

`(name=…)`: a machine the network may run on, named so populations can be pinned to it (see Placement on nodes).

### `neuron_types` (new) and neuron keys

`name`, `extends`, then:

| Key | Old | Meaning (default) |
|---|---|---|
| `dynamics` | `nty` | `monostable_integrator` (default), `bistable_integrator`, `monostable_resonator`, `bistable_resonator` |
| `inhibitory` | `inh` | its outgoing connections are inhibitory unless a synapse type says otherwise (`false`) |
| `threshold` | `mst` | firing threshold (10 mV); integrators and resonators |
| `tonic_on_threshold`, `tonic_off_threshold`, `tonic_rate` | `lct`, `rst`, `tfr` | bistable integrator (15 mV, 10 mV, 10 Hz) |
| `refractory_period` | `rfp` | absolute refractory period (20 ms) |
| `relative_refractoriness` | `rfb` | area of the relative-refractoriness kernel (0.08 µWb) |
| `resting_potential` | `mnp` | minimum membrane potential (0 mV) |
| `decay_time` | `mpd` | membrane potential decay time constant (250 ms) |
| `rise_time` | `mpr` | time from threshold crossing to spike (2 ms) |
| `membrane_noise` | `mpn` | Wiener noise per √ms (0.1 mV) |
| `spike_potential` | `spp` | spike amplitude sent along outgoing connections (1 mV) |
| `conduction_speed` | `csp` | axon conduction speed: delay adds `distance / conduction_speed` (10 m/s) |
| `intrinsic_plasticity` | `ipb`, `ipl`, `ipd` | `(step=…, amplitude=…, decay_time=…)` |
| `homeostasis` | `HIP` | `(target_rate=…, strength=…, window=…, min_shift=…, max_shift=…)` |
| `resonance` | `RES` | `(frequency=…, damping_time=…, limit_cycle=…, separatrix=…)`; resonators |
| `receptors` | `RCP` | `[(modulator=…, excitatory_gain=…, inhibitory_gain=…, threshold_gain=…, plasticity_gain=…, eligibility_time=…, learning_rate=…, direct_potential=…)]` |
| `releases` | `REL` | `(modulator=…, amount=…)`: a modulator released at each spike |

Gone from neurons: `nid`, `grp`, `LOC` (see `neurons`/`populations`); `WDF`, `WLF`, `SRP`, `wnm`, `syd` (moved to
synapse types).

### `neurons`

Neurons one by one: `name` (required), `type`, `position` (required), `node`, and any neuron key as an override.

### `populations`

Many neurons of one type: `name` (required; its neurons are `<name>-0`, `<name>-1`, …), `type`, `node`, any
neuron key as an override, and one placement:

| Placement | Meaning |
|---|---|
| `positions=[…]` | these points, in order (with `count` omitted); or `positions=[(name=…, at=…), …]` to name them |
| `random=(box=(corner=[…], size=[…]))` or `random=(ball=(center=[…], radius=…))` | `count` neurons at random (`seed`, default 1) |
| `grid=(origin=[…], spacing=[…], shape=[nx, ny, nz])` | a regular grid (`count` = nx·ny·nz) |
| `line=(from=[…], to=[…])` | `count` neurons evenly from one point to another |
| `ring=(center=[…], radius=…, axis=z)` | `count` neurons evenly around a circle (also `start_angle`) |

### `sets`

`(name=…, members=…)`: named sets of neurons. Members are any mix of populations, sets, neuron names and id
ranges: `members=[h1, h2, input-1{1:7:1}]`. Connections, and placement on nodes, refer to them.

### `synapse_types` (new) and synapse keys

`name`, `extends`, then:

| Key | Old | Meaning (default) |
|---|---|---|
| `weight` | `cnw` | initial weight: a number, `uniform(…)` or `normal(…)` (required, here or on the connection) |
| `equilibrium` | `eqw` | the weight decay relaxes toward (the initial weight) |
| `sign` | `typ` | `excitatory` or `inhibitory` (from the sending neuron's `inhibitory`) |
| `delay` | `syd` | synaptic delay; the total is `delay + distance / conduction_speed` (0.5 ms) |
| `learning` | `lrn` | a learning rule's name (none) |
| `stickiness` | `cst` | a stickiness function's name (none) |
| `decay` | `WDF` | `(time=…)`: exponential decay toward the equilibrium (none) |
| `limits` | `WLF` | `(min=…, max=…)` (unbounded) |
| `release` | `SRP` | `(facilitation_base, facilitation_step, facilitation_time, depletion_base, depletion_step, depletion_time)`: stochastic release (none — **off by default**, unlike v1) |
| `weight_noise` | `wnm` | noise added to the weight at each signal (0) |

### `connections`

Unifies `CON` and `RUL`:

| Key | Old | Meaning |
|---|---|---|
| `from`, `to` | `prn`/`prp`, `psn`/`psp` | sending and receiving neurons: populations, sets, names, id ranges, or a list of them |
| `pattern` | | `all_to_all` (default), `one_to_one` (the i-th sender to the i-th receiver; equal counts), `fan_in` (each receiver gets `count` inputs from different senders), `random` (each pair with `probability`) |
| `count`, `probability` | `cnt`, `prb` | for `fan_in`, `random` |
| `within` | `rad` | only pairs closer than this |
| `self` | `aut` | a neuron may connect to itself (`false`) |
| `type` | | a synapse type |
| any synapse key | | overrides of the type's |

Random patterns are expanded per receiver with their own random stream, as rules are now (the same on every node).

### `learning_rules`, `stickiness`, `modulators`

As now, with long names: `learning_rules=[(name=…, rule=stdp_hard | stdp_soft | stdp_alpha, depression=…,
depression_time=…, potentiation=…, potentiation_time=…)]` (alpha: `baseline`, `time`, `rate`);
`stickiness=[(name=…, time=…)]` (sigmoid); `modulators=[(name=…, clearance_time=…, reach=…, speed=…, latency=…,
falloff=…)]`. `flat` (no learning) and no stickiness need no definition.

## Placement on nodes

A network can run over several machines (node agents). The engine itself is the node `main`.

- `nodes=[(name=camera), (name=cortex)]` names the other machines the network expects.
- A neuron or population with `node=camera` runs there; a set can be pinned too: `sets=[(name=v1, members=…,
  node=cortex)]`. Everything not pinned is spread over all the nodes (`main` included) by position and expected
  work, as now, accounting for what's pinned.
- Which machine is `camera` is decided where the network is run, not in the file: the Settings page maps node
  names to node agents (`camera=10.0.0.5:7071, cortex=…`). A description that pins nothing runs anywhere, on any
  number of nodes, as now.

Pinning keeps a sensor's input neurons on the machine the sensor is attached to, or a big population on a big
machine; the rest is balanced automatically.

## Examples

The ring (50 lines in the old form):

```
(
neuron_types=[(name=hidden, threshold=5 mV, refractory_period=20 ms, decay_time=2500 ms, conduction_speed=1 m/s)],
synapse_types=[
    (name=plastic, weight=1.5, learning=stdp, stickiness=sig,
     decay=(time=10 s), limits=(min=0, max=5)),
],
populations=[
    (name=h1, type=hidden, count=5, line=(from=[40, 0, 0], to=[40, 0, 40])),
    (name=h2, type=hidden, count=5, line=(from=(r=40, phi=2.1, z=0), to=(r=40, phi=2.1, z=40))),
    (name=h3, type=hidden, count=5, line=(from=(r=40, phi=3.15, z=0), to=(r=40, phi=3.15, z=40))),
    (name=h4, type=hidden, decay_time=2400 ms, count=5, line=(from=(r=40, phi=4.2, z=0), to=(r=40, phi=4.2, z=40))),
    (name=h5, type=hidden, count=5, line=(from=(r=40, phi=5.25, z=0), to=(r=40, phi=5.25, z=40))),
],
connections=[
    (from=h1, to=h2, type=plastic), (from=h2, to=h3, type=plastic), (from=h3, to=h4, type=plastic),
    (from=h4, to=h5, type=plastic), (from=h5, to=h1, type=plastic),
],
learning_rules=[(name=stdp, rule=stdp_hard, depression=0.035, depression_time=3 ms, potentiation=0.035, potentiation_time=3 ms)],
stickiness=[(name=sig, time=10 s)],
)
```

Cortex-like populations, with the excitatory population pinned to a node:

```
(
nodes=[(name=big)],
neuron_types=[
    (name=pyramidal, threshold=10 mV, decay_time=20 ms, refractory_period=4 ms, conduction_speed=1 m/s),
    (name=basket, extends=pyramidal, inhibitory=true, threshold=8 mV, decay_time=10 ms, refractory_period=2 ms),
],
synapse_types=[
    (name=exc, weight=uniform(0.2, 0.6), learning=stdp, limits=(min=0, max=2)),
    (name=inh, weight=uniform(1, 2)),
],
populations=[
    (name=exc, type=pyramidal, count=3200, node=big, random=(box=(corner=[0, 0, 0], size=[800, 800, 200]))),
    (name=inh, type=basket, count=800, random=(box=(corner=[0, 0, 0], size=[800, 800, 200])), seed=2),
],
connections=[
    (from=exc, to=exc, pattern=fan_in, count=50, type=exc),
    (from=inh, to=exc, pattern=random, probability=0.2, within=150 µm, type=inh),
    (from=exc, to=inh, pattern=fan_in, count=40, within=200 µm, type=exc, weight=uniform(0.5, 1)),
],
learning_rules=[(name=stdp, rule=stdp_hard, depression=0.01, depression_time=30 ms, potentiation=0.01, potentiation_time=20 ms)],
)
```

## Implementation

1. **Language**: the new schema (long names; generated editor schema), parser (points, `uniform`/`normal`),
   description model (types, overrides, populations with placements, sets, unified connections, synapse types),
   errors with positions as now. Removes the old keys.
2. **Builder**: synapse parameters per synapse type (the engine's parameter table is per type already; fixed
   types get the compact storage), the four patterns, the placements.
3. **Migration**: `spikes migrate old.boo` (from the old parser, kept only for it), then the test networks and
   examples are converted and the old form removed.
4. **Nodes**: `nodes`, `node=` on neurons, populations and sets; named node agents in Settings and the protocol;
   pinned partitions in the runtime's placement.
5. **UI**: examples, the editor (from the schema), the network summary.
