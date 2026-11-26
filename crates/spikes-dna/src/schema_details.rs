//! The long help for every key of the description language (the editor shows it on hover, under the key's one-line
//! [`Key::doc`](crate::schema::Key)): what it does, how it changes the network, equations, limits, when it's needed
//! and when it's ignored. Keyed by the record that defines the key (a population's `threshold` is
//! `neuronSettings.threshold`). Markdown, written for readers who know neurons but not this code.
//!
//! Each entry follows the same shape: what the setting is for and what changing it does, in plain words; then,
//! where it helps, the equation (on its own line, symbols explained); then short labelled notes — **Values**,
//! **Ignored**, **Tip**, **Note**.
//!
//! Every key has an entry (a test checks), so a new key can't go undocumented. Keep the text true to the model:
//! the equations are the ones `spikes-model` computes.

/// `(record, key, details)`.
pub static DETAILS: &[(&str, &str, &str)] = &[
    // ----- the network -----
    (
        "network",
        "nodes",
        r"The other machines this network can run on, by name. Naming them here lets you pin neurons, populations and sets to them with `node=` (for example, keep the sensor neurons on the machine the sensors are attached to).

You only give names here. Which machine each name means is set in **Settings → node agents** (`name=host:port`), so the same file works on different clusters.

**Note:** on the engine alone, pins to other nodes are dropped (the Deployment page says so). On a cluster, a pin to a node that no agent is named refuses the build. `main` is the engine itself and can't be declared.",
    ),
    (
        "network",
        "neuron_types",
        r"Reusable sets of neuron settings. Define a type once (say `pyramidal` or `fast_inhibitory`), then give neurons and populations `type=` instead of repeating the same settings.

A neuron or population can still override any setting of its type. A type can build on another with `extends`.

**Note:** an override replaces the whole key. Writing `resonance=(frequency=20 Hz, …)` on a neuron replaces the type's entire `resonance`, so give every field you need.",
    ),
    (
        "network",
        "synapse_types",
        r"Reusable sets of synapse settings (weight, delay, learning, limits, …). Connections use one with `type=` and can override any of its settings. A type can build on another with `extends`.

**Note:** every connection needs a `weight`, from the connection or its type. An override replaces the whole key (a connection's `limits=(…)` replaces the type's `limits`).",
    ),
    (
        "network",
        "neurons",
        r"Neurons defined one at a time, each with its own name and position. Use this for a handful of special neurons; use `populations` for groups.

A network needs at least one neuron here or in `populations`. Neurons are numbered in the order written: these first, then each population's.",
    ),
    (
        "network",
        "populations",
        r"Groups of neurons that share settings, placed in one go: at listed points, at random in a box or ball, on a grid, along a line or around a ring.

Neurons are named `<name>-0`, `<name>-1`, … and the population's name stands for all of them in connections and sets.",
    ),
    (
        "network",
        "sets",
        r"Named groups of neurons that can mix populations and single neurons. Use them to connect groups that aren't a single population, or to pin a group to a machine.

**Note:** sets are read in order, so a set can include only sets defined above it.",
    ),
    (
        "network",
        "connections",
        r"The wiring: each connection makes synapses from some neurons (`from`) to others (`to`) by a pattern, with the synapse settings given (or taken from a synapse type).

**Note:** random weights and random patterns come from the Run page's seed. The same seed builds exactly the same network, however many shards or machines run it. Moving a connection to another place in the list changes its random draws. Two connections between the same pair of neurons make two separate synapses.",
    ),
    (
        "network",
        "learning_rules",
        r"Spike-timing-dependent plasticity (STDP) rules, so synapses can strengthen or weaken with use. Define a rule here and give synapses `learning=<name>`. Without one, synapses don't learn (`flat`, built in).",
    ),
    (
        "network",
        "stickiness",
        r"Functions that protect learned weights from fading. A synapse with `decay` slowly returns to its equilibrium weight; stickiness slows that return for synapses left alone for a long time. Give synapses `stickiness=<name>`. `none` is built in and is the default.",
    ),
    (
        "network",
        "modulators",
        r"Neuromodulators such as dopamine or acetylcholine. Neurons that `releases` one send it out when they spike; it spreads through space and fades; neurons with `receptors` for it change how they respond (more or less excitable, faster or slower learning, reward-driven learning).

**Note:** nothing is simulated on a clock. Each release reaches each neuron at a computed time and then fades exponentially. Releases are remembered for 60 s (or 20 clearance times, if longer).",
    ),
    // ----- nodes -----
    (
        "node",
        "name",
        r"The name used in `node=` to pin neurons to this machine. Settings → node agents connects the name to an actual machine (`name=host:port`).

**Values:** any name except `main`, which is the engine itself.",
    ),
    // ----- neuron settings -----
    (
        "neuronSettings",
        "dynamics",
        r"Integrators add up their inputs; resonators respond to their rhythm. Monostable neurons fire once when they reach the threshold; bistable ones switch into steady firing.

- `monostable_integrator` (default): the classic leaky integrate-and-fire neuron. Inputs add up and leak away (`decay_time`); reaching `threshold` fires one spike and resets it.
- `bistable_integrator`: like the above, but reaching `tonic_on_threshold` switches it into **steady firing** at `tonic_rate`, which lasts until its potential has leaked below `tonic_off_threshold`. A memory of a strong input.
- `monostable_resonator`: its potential rings at a frequency (`resonance`) as it fades. Inputs that arrive in step with the ringing build up; out-of-step inputs cancel. So it responds to input *rhythms*, not just input strength.
- `bistable_resonator`: a resonator that, once it fires, keeps firing once per cycle until a well-timed input knocks it back to rest.

**Ignored:** settings that don't belong to the chosen dynamics are ignored: `threshold` for bistable integrators; `resonance` for integrators; `decay_time` and `rise_time` for resonators; the `tonic_*` settings for everything but bistable integrators.",
    ),
    (
        "neuronSettings",
        "inhibitory",
        r"Makes this neuron's outgoing connections inhibitory: its spikes push the neurons it connects to *away* from firing instead of toward it.

It only sets the default; a connection's `sign=` can override it. It doesn't change how this neuron reacts to its own inputs. In the 3D view, inhibitory neurons have their own colour.",
    ),
    (
        "neuronSettings",
        "threshold",
        r"How high the potential must get for the neuron to fire. A higher threshold needs more (or better-timed) input; a lower one makes the neuron fire more easily.

The neuron fires when

`u + η + b + m ≥ θ × g`

- `u`: the membrane potential
- `η`: relative refractoriness, briefly negative after a spike
- `b`: the excitability shift from intrinsic plasticity and homeostasis
- `m`: direct excitation from modulators
- `θ`: this threshold
- `g`: the receptors' threshold gain (1 without modulators)

**Note:** an integrator can only reach the threshold when an input arrives (or a modulator pushes it). A resonator can also cross on its own, as its ringing swings up after an input; that moment is computed exactly.

**Ignored:** for `bistable_integrator`, which uses `tonic_on_threshold`.",
    ),
    (
        "neuronSettings",
        "tonic_on_threshold",
        r"For bistable integrators: the potential that switches the neuron into steady (tonic) firing. It takes the place of `threshold`.

**Note:** the potential can't rise above this value. Extra input while the neuron fires steadily keeps it topped up (prolonging the firing) rather than raising it further.

**Values:** keep it above `tonic_off_threshold`.

**Ignored:** for other dynamics.",
    ),
    (
        "neuronSettings",
        "tonic_off_threshold",
        r"For bistable integrators: steady firing stops once the potential has leaked below this value. The lower it is, the longer the neuron keeps firing after its input stops.

Without further input, steady firing lasts about

`decay_time × ln(tonic_on_threshold / tonic_off_threshold)`

- `ln`: the natural logarithm. With the defaults (15 mV and 10 mV, 250 ms), about 100 ms.

**Note:** the potential keeps leaking during steady firing, and inputs still add to it, so ongoing input extends the firing. The check happens at each steady spike.

**Values:** keep it below `tonic_on_threshold`. At or above it, firing stops after one spike.

**Ignored:** for other dynamics.",
    ),
    (
        "neuronSettings",
        "tonic_rate",
        r"For bistable integrators: how fast the neuron fires while firing steadily, one spike every `1 / tonic_rate` (10 Hz: every 100 ms).

**Note:** steady spikes ignore the refractory period and the rise time.

**Values:** must be positive.

**Ignored:** for other dynamics. A bistable resonator fires once per resonance cycle instead.",
    ),
    (
        "neuronSettings",
        "refractory_period",
        r"A dead time after each spike, during which the neuron can't fire again. It caps how fast the neuron can fire (20 ms: at most about 50 Hz).

**Note:** input that arrives during this time is **lost**, not saved for later. Integrators are held at `resting_potential`; resonators keep ringing but ignore inputs. Signals arriving then still count for learning (STDP depression). Afterwards, `relative_refractoriness` makes firing harder for a little longer.

**Values:** 0 turns it off. Steady (tonic) firing ignores it.",
    ),
    (
        "neuronSettings",
        "relative_refractoriness",
        r"Makes the neuron harder to fire just after the refractory period ends, easing off over time. It softens the hard cut-off of the refractory period.

A term is added to the potential when it's compared with the threshold:

`η(s) = −η₀ / s`

- `η₀`: this setting (1 µWb is 1 mV·ms)
- `s`: milliseconds since the refractory period ended

With the default 0.08 µWb, firing needs 0.08 mV more 1 ms after the refractory period, and 0.008 mV more after 10 ms. Small unless you raise it.

**Values:** 0 turns it off.",
    ),
    (
        "neuronSettings",
        "resting_potential",
        r"Where the potential goes after a spike, and (for integrators) the lowest it can go.

**Note:** integrators leak toward **0 mV**, not toward this value. With a negative resting potential, inhibition can pull the neuron below 0 down to this floor, so it then needs extra input to recover. With a positive one, the potential never drops below it. During the refractory period an integrator is held here.

Resonators reset here after a spike, but their ringing isn't limited by it.",
    ),
    (
        "neuronSettings",
        "decay_time",
        r"How quickly an integrator forgets its input: the membrane time constant τ. With a long τ, inputs spread out in time still add up; with a short one, only inputs that arrive close together can make the neuron fire, so it acts as a coincidence detector.

Between inputs the potential leaks toward 0 mV:

`u(t + Δ) = u(t) × e^(−Δ/τ)`

- `u(t)`: the potential at time `t`
- `Δ`: the time since then (until the next input)
- `τ`: this setting, `decay_time`

After τ, about 37% of the potential remains; after 3τ, about 5%.

**Values:** 0 or less means no leak.

**Ignored:** for resonators, which use `resonance.damping_time`.",
    ),
    (
        "neuronSettings",
        "rise_time",
        r"For integrators: a short delay between reaching the threshold and the spike leaving, standing in for the time a real membrane takes to rise. An input that only just tips the neuron over fires it sooner than one that lifts it all the way from rest:

`delay = rise_time × (min(θ, u_after) − u_before) / θ`

- `delay`: how long after the input the spike leaves
- `θ`: the threshold
- `u_before`: the potential just before the input (counting the refractoriness, excitability and modulator terms of the threshold test)
- `u_after`: the potential just after it

**Values:** 0 fires at the moment of the input.

**Ignored:** for resonators, and for steady (tonic) spikes.",
    ),
    (
        "neuronSettings",
        "membrane_noise",
        r"Random jitter in the potential, so identical neurons don't behave identically and firing isn't perfectly regular.

The noise grows with the time since the neuron's last event, like a random walk. At each event the potential gets

`σ × √(Δ / 1 ms) × N(0, 1)`

- `σ`: this setting
- `Δ`: milliseconds since the neuron's previous event
- `N(0, 1)`: a standard normal random number

**Note:** noise is added only when something happens to the neuron, so it can't make a quiet integrator fire by itself. The random numbers come from the Run page's seed.

**Values:** 0 turns it off.",
    ),
    (
        "neuronSettings",
        "spike_potential",
        r"The size of the signal this neuron's spikes send. Each neuron it connects to receives

`sign × weight × spike_potential`

- `sign`: +1 for an excitatory synapse, −1 for an inhibitory one
- `weight`: the synapse's current weight
- `spike_potential`: this setting, on the sending neuron

(scaled by the receiver's modulator gains, if any), after the connection's delay.

**Tip:** with the default 1 mV, a synapse's weight is simply how many mV a spike adds to the receiver.",
    ),
    (
        "neuronSettings",
        "conduction_speed",
        r"How fast this neuron's spikes travel along its axon. Together with the distance to each receiver, it sets how long spikes take to arrive:

`arrival delay = delay + distance / conduction_speed`

- `delay`: the synapse's own delay (0.5 ms by default)
- `distance`: the straight-line distance between the two neurons

At 10 m/s, 100 µm takes 10 µs, small next to the synaptic delay. Slow it down (like unmyelinated axons, around 0.5 m/s) to make distance matter.

**Note:** no connection is faster than the build's minimum delay of 100 µs, which keeps multi-shard runs efficient.

**Values:** must be positive.",
    ),
    (
        "neuronSettings",
        "intrinsic_plasticity",
        r"Lets a neuron become easier to fire the more it fires: a simple form of learning within the neuron itself (long-term potentiation of intrinsic excitability).

The neuron keeps an excitability shift `b` that is added to its potential when it's compared with the threshold, so a positive `b` works like a lower threshold. At each spike

`b ← b + amplitude × e^(−(b + step))`

- `b`: the excitability shift, in mV (0 to start with)
- `amplitude`, `step`: settings of this record
- `←`: “becomes”: the new `b` is computed from the old one

so each increase is smaller than the last. Between spikes, `b` fades back toward 0 with `decay_time` (10 minutes by default).

**Note:** off unless `amplitude` is non-zero. `homeostasis` adjusts the same `b`.",
    ),
    (
        "neuronSettings",
        "homeostasis",
        r"Keeps the neuron's firing rate near a target by adjusting its excitability: a neuron that fires too little becomes easier to fire, and one that fires too much becomes harder to fire. Useful to stop learning networks from going silent or running away.

The neuron estimates its own rate `r` and moves its excitability shift `b` (added to the potential when it's compared with the threshold) by

`db/dt = strength × (target_rate − r)`

- `b`: the excitability shift, in mV
- `db/dt`: how fast `b` changes, in mV per second
- `r`: the neuron's current estimate of its own firing rate, in Hz (see `window`)
- `strength`, `target_rate`: settings of this record

within `min_shift` and `max_shift`.

**Note:** the same `b` also fades with `intrinsic_plasticity.decay_time` (10 minutes by default), even without `intrinsic_plasticity`. A neuron that never gets any input still can't fire, however excitable it becomes.

**Tip:** set `max_shift`, or a long-silent neuron's excitability grows without limit.",
    ),
    (
        "neuronSettings",
        "resonance",
        r"Makes the neuron ring at a frequency, like a tuning fork, instead of simply leaking. Inputs that arrive in time with the ringing add up; inputs out of step cancel. So the neuron prefers inputs that repeat at its frequency.

The state is a damped oscillation (potential `u` plus a hidden partner `x`):

`(x + i·u)(t + Δ) = (x + i·u)(t) × e^((−1/damping_time + i·2π·frequency) × Δ)`

- `u`: the membrane potential (what's compared with the threshold)
- `x`: its partner in the oscillation (when `u` is at a peak, `x` is 0, and vice versa)
- `i`: the imaginary unit; the pair `x + i·u` turns in a circle as it shrinks
- `t`: a moment; `Δ`: the time since then
- `damping_time`, `frequency`: settings of this record

In other words: `u` swings up and down at `frequency`, and the swings shrink by `e^(−Δ/damping_time)`. Each input bumps `u`.

**Values:** required for `monostable_resonator` and `bistable_resonator`.

**Ignored:** for integrators.",
    ),
    (
        "neuronSettings",
        "receptors",
        r"How this neuron responds to neuromodulators, one entry per modulator. Each effect is proportional to the modulator's concentration `c` where the neuron is. When there's no modulator, nothing changes.

With several receptors, gains multiply and direct effects add. A gain never goes below zero.

**Note:** the input gains change input from other neurons only. External input (sensors, background input, stimulation from the Run page) isn't scaled.",
    ),
    (
        "neuronSettings",
        "releases",
        r"Makes this neuron release a modulator each time it spikes, from where it is. The release spreads as the modulator describes (`reach`, `speed`, `latency`, `falloff`). It affects only neurons that have a receptor for it, the releasing neuron included.

**Tip:** for reward learning, have a small group of “reward” neurons release dopamine, and give the learning neurons a receptor with three-factor learning.",
    ),
    // ----- neuron types, neurons -----
    (
        "neuronType",
        "name",
        r"The type's name, used in `type=` on neurons and populations, and in `extends` on other types.

**Values:** unique among neuron types.",
    ),
    (
        "neuronType",
        "extends",
        r"Starts this type from another type's settings, so you only write what's different.

**Note:** a setting given here replaces the base type's whole setting (a `resonance` here replaces the base's entire `resonance`). Chains of types are fine; a loop is an error.",
    ),
    (
        "neuron",
        "name",
        r"The neuron's name, used to connect it, to put it in sets, and in the raster and tooltips.

**Values:** unique, and not the name of a population's neuron (`<population>-<i>`).",
    ),
    (
        "neuron",
        "type",
        r"A neuron type to take settings from. Settings written on the neuron replace the type's. Without a type, any setting not given takes its default.",
    ),
    (
        "neuron",
        "position",
        r"Where the neuron is, in µm. Positions matter for timing and reach:

- spikes take longer to reach distant neurons (`distance / conduction_speed`);
- connections can be limited by distance (`within`);
- modulators spread out from where they're released, and may weaken with distance.

**Values:** `[x, y, z]`; or cylindrical `(r=…, phi=…, z=…)`; or spherical `(r=…, theta=…, phi=…)` with θ measured from the +z axis. Other length units are fine (`[0, 0, 1 mm]`).",
    ),
    (
        "neuron",
        "node",
        r"Runs this neuron on a particular machine (a name from `nodes`, or `main` for the engine). Without it, the neuron is placed automatically.

**Note:** a neuron's own pin wins over a set's. On the engine alone, the pin is ignored; on a cluster, pinning to a node that no agent is named refuses the build.",
    ),
    // ----- populations -----
    (
        "population",
        "name",
        r"The population's name. Its neurons are called `<name>-0`, `<name>-1`, … (unless `positions` names them), and the name on its own stands for all of them in `from`, `to` and `members`.

**Values:** unique among populations, sets and neurons.",
    ),
    (
        "population",
        "type",
        r"A neuron type for all of the population's neurons. Settings written on the population replace the type's.",
    ),
    (
        "population",
        "count",
        r"How many neurons the population has.

**Values:** a whole number from 1 to 100,000,000. Needed for `random`, `line` and `ring`. Leave it out for `positions` and `grid`, which set the count themselves.",
    ),
    (
        "population",
        "positions",
        r"Places the neurons at exactly these points, in order. Write `[[x, y, z], …]` for neurons named `<name>-0`, `<name>-1`, …, or `[(name=…, at=…), …]` to name each one.

**Note:** the number of points is the population's size; don't give `count`.",
    ),
    (
        "population",
        "random",
        r"Scatters `count` neurons at random, evenly, inside a box (`box=(…)`) or a ball (`ball=(…)`).

**Note:** the positions depend only on the population's name and `seed`. Changing the Run page's seed or other parts of the file doesn't move them.",
    ),
    (
        "population",
        "grid",
        r"Places neurons on a regular 3D grid: `shape` says how many along each axis, `spacing` how far apart, `origin` where the first one is.

**Note:** neurons are numbered along x first, then y, then z. The size is nx × ny × nz; don't give `count`.",
    ),
    (
        "population",
        "line",
        r"Places `count` neurons evenly along a straight line from `from` to `to`, with one neuron at each end. A single neuron sits at `from`.",
    ),
    (
        "population",
        "ring",
        r"Places `count` neurons evenly around a circle. Neuron `i` (counting from 0) is at angle `start_angle + 360° × i / count`.",
    ),
    (
        "population",
        "seed",
        r"Chooses a different random arrangement for `random` placement. Two populations with the same seed still get different positions, because the name is mixed in.

**Values:** a whole number (default 1).

**Ignored:** for other placements. It never affects the run itself.",
    ),
    (
        "population",
        "node",
        r"Runs all of the population's neurons on one machine (a name from `nodes`, or `main`). Without it, they're spread over the machines automatically.

**Ignored:** on the engine alone.",
    ),
    ("namedPosition", "name", r"This neuron's name, instead of `<population>-<i>`. Must be unique."),
    ("namedPosition", "at", r"Where this neuron is, written like any position: `[x, y, z]` in µm."),
    (
        "randomPlacement",
        "box",
        r"A rectangular box: `corner` is its lowest corner, `size` its lengths along x, y and z. Neurons are spread evenly through it.",
    ),
    (
        "randomPlacement",
        "ball",
        r"A ball: `center` and `radius`. Neurons are spread evenly through its volume.",
    ),
    ("box", "corner", r"The box's lowest corner: the smallest x, y and z, in µm."),
    (
        "box",
        "size",
        r"The box's lengths along x, y and z, in µm.

**Values:** none negative. A length of 0 flattens the box (a sheet or a line).",
    ),
    ("ball", "center", r"The ball's centre, in µm."),
    (
        "ball",
        "radius",
        r"The ball's radius, in µm.

**Values:** must be positive.",
    ),
    ("grid", "origin", r"Where the first neuron of the grid is (default `[0, 0, 0]`)."),
    (
        "grid",
        "spacing",
        r"The distance between neighbouring neurons along x, y and z, in µm: `[dx, dy, dz]`.

**Note:** a spacing of 0 stacks that axis's neurons in the same place.",
    ),
    (
        "grid",
        "shape",
        r"How many neurons along x, y and z: `[10, 10, 1]` is a flat 10 × 10 sheet of 100 neurons.

**Values:** three whole numbers of at least 1, with a product of at most 100,000,000.",
    ),
    ("line", "from", r"Where the first neuron is."),
    ("line", "to", r"Where the last neuron is (when `count` is 2 or more)."),
    ("ring", "center", r"The circle's centre (default `[0, 0, 0]`)."),
    ("ring", "radius", r"The circle's radius, in µm."),
    (
        "ring",
        "axis",
        r"Which axis the circle goes around, which sets the plane it lies in: `z` (default) lies flat in the x–y plane, `x` in the y–z plane, `y` in the z–x plane.",
    ),
    (
        "ring",
        "start_angle",
        r"Where around the circle the first neuron is. Measured in the circle's plane, from its first axis (x, for the default `axis=z`).

**Values:** radians, or degrees with `deg` (`90 deg`).",
    ),
    // ----- sets -----
    (
        "set",
        "name",
        r"The set's name, used in `from`, `to`, and the `members` of later sets.

**Values:** unique among sets, populations and neurons.",
    ),
    (
        "set",
        "members",
        r"The neurons in the set: populations, earlier sets, neuron names and ranges of names, or a list of them. A neuron listed twice is included once.

Ranges add numbers to a name: `input-1{1:7:3}` is `input-11`, `input-14` and `input-17` (start:end:step, end included), and `inhib-{1,3,5}` is `inhib-1`, `inhib-3` and `inhib-5`.",
    ),
    (
        "set",
        "node",
        r"Runs the set's neurons on one machine (a name from `nodes`), except neurons pinned by themselves or by their population.

**Note:** if a neuron is in several pinned sets, the first set wins.

**Ignored:** on the engine alone.",
    ),
    // ----- intrinsic plasticity -----
    (
        "intrinsicPlasticity",
        "step",
        r"Shrinks every increase in excitability: each spike raises the neuron's excitability shift `b` (in mV, added to its potential when it's compared with the threshold) by `amplitude × e^(−(b + step))`, so a larger step means smaller increases (`e^(−step)` times smaller).

**Note:** despite its name, it isn't added to `b`. It has no effect while `amplitude` is 0.",
    ),
    (
        "intrinsicPlasticity",
        "amplitude",
        r"How much easier to fire the neuron becomes with each spike, in mV. Each spike raises the excitability shift `b` by `amplitude × e^(−(b + step))`, and `b` is added to the potential when it's compared with the threshold.

**Values:** 0 (the default) turns it off. Positive values make neurons that fire more likely to fire again.",
    ),
    (
        "intrinsicPlasticity",
        "decay_time",
        r"How long gained excitability lasts. Between spikes, `b` fades back toward 0:

`b ← b × e^(−Δ/τ)`

- `b`: the excitability shift
- `Δ`: the time since the neuron's previous event
- `τ`: this setting, `decay_time`

The default, 10 minutes, makes it long-lasting.

**Note:** it also applies to the shift built up by `homeostasis`.",
    ),
    // ----- resonance -----
    (
        "resonance",
        "frequency",
        r"The frequency the neuron rings at. Inputs about `1 / frequency` apart add up best (at 20 Hz, every 50 ms); inputs at other intervals partly cancel. A bistable resonator fires once per cycle while firing steadily.

**Values:** must be positive.",
    ),
    (
        "resonance",
        "damping_time",
        r"How long the ringing lasts: its size fades as `e^(−Δ/τ)`. When this is several cycles long, the neuron is sharply tuned to its frequency. When it's short, the ringing dies before it matters, and the neuron behaves more like an integrator.

**Values:** 0 or less means the ringing never fades.",
    ),
    (
        "resonance",
        "limit_cycle",
        r"For bistable resonators: the size, in mV, of the steady oscillation the neuron settles into once it fires. It fires once per cycle, at the peak.

**Values:** required for bistable resonators, and larger than `separatrix`.

**Ignored:** for monostable resonators.",
    ),
    (
        "resonance",
        "separatrix",
        r"For bistable resonators: how hard it is to stop the steady firing. An input pulse knocks the oscillation off its cycle. If that leaves it smaller than this radius (in mV), steady firing stops and the neuron rings down to rest. Otherwise it carries on, but with its timing shifted.

**Tip:** a smaller separatrix needs a bigger or better-timed inhibitory pulse to stop the firing.

**Values:** required for bistable resonators, and smaller than `limit_cycle`.",
    ),
    // ----- homeostasis -----
    (
        "homeostasis",
        "target_rate",
        r"The firing rate the neuron is nudged toward. Below it the neuron becomes more excitable; above it, less.

**Values:** required, in Hz.",
    ),
    (
        "homeostasis",
        "strength",
        r"How hard the neuron is nudged toward its target rate, in mV of excitability per Hz of error per second:

`db/dt = strength × (target_rate − r)`

- `b`: the excitability shift, in mV
- `db/dt`: how fast it changes, in mV per second
- `r`: the neuron's estimate of its firing rate, in Hz
- `target_rate`: the rate aimed for

Stronger corrects faster but can overshoot and oscillate.

**Tip:** start well below 1.

**Values:** required.",
    ),
    (
        "homeostasis",
        "window",
        r"How long the neuron averages its firing over when estimating its rate (the time constant of the estimate). Each spike adds `1 / window` to the estimate, which then fades.

**Note:** longer windows give a steadier estimate but react more slowly.

**Values:** default 1 s.",
    ),
    (
        "homeostasis",
        "min_shift",
        r"The least excitable the neuron can become, as a limit on the shift `b` in mV. Stops a fast-firing neuron from being shut down completely.

**Values:** default unlimited.",
    ),
    (
        "homeostasis",
        "max_shift",
        r"The most excitable the neuron can become, as a limit on the shift `b` in mV.

**Tip:** worth setting. Otherwise a neuron that stays silent keeps becoming more excitable, and eventually fires at the slightest input.

**Values:** default unlimited.",
    ),
    // ----- receptors -----
    (
        "receptor",
        "modulator",
        r"Which modulator (from `modulators`) this receptor responds to. Its concentration at the neuron, `c`, drives the effects below.

**Values:** required.",
    ),
    (
        "receptor",
        "excitatory_gain",
        r"Turns excitatory input up or down while the modulator is present. Excitatory input from other neurons is multiplied by

`max(0, 1 + excitatory_gain × c)`

- `c`: the modulator's concentration at the neuron (0 when there's none)
- `max(0, …)`: never below 0, so input is never reversed

Positive values amplify it; negative ones damp it, down to nothing.

**Note:** external input (sensors, background input, stimulation) isn't affected.",
    ),
    (
        "receptor",
        "inhibitory_gain",
        r"Turns inhibitory input up or down while the modulator is present. Inhibitory input from other neurons is multiplied by

`max(0, 1 + inhibitory_gain × c)`

- `c`: the modulator's concentration at the neuron

**Note:** external input isn't affected.",
    ),
    (
        "receptor",
        "threshold_gain",
        r"Raises or lowers the threshold while the modulator is present. The threshold (or a bistable integrator's tonic thresholds) is multiplied by

`max(0, 1 + threshold_gain × c)`

- `c`: the modulator's concentration at the neuron

Negative values make the neuron easier to fire; positive values make it harder.",
    ),
    (
        "receptor",
        "plasticity_gain",
        r"Speeds up or slows down learning on the neuron's incoming synapses while the modulator is present. STDP weight changes are multiplied by

`max(0, 1 + plasticity_gain × c)`

- `c`: the modulator's concentration at the neuron

**Note:** without the modulator, learning runs at its normal rate. With a negative gain, learning stops once the concentration reaches `−1 / plasticity_gain`.

**Ignored:** with three-factor learning (`eligibility_time`), which has its own rate.",
    ),
    (
        "receptor",
        "eligibility_time",
        r"Turns on **reward-based (three-factor) learning**, together with `learning_rate`. Spike timing alone no longer changes the weights. Instead it *marks* the synapses involved (an eligibility trace), and the marks fade with this time constant. Weights only change while the modulator (the reward signal) is present:

`Δw = learning_rate × ∫ e(t) × c(t) dt`

- `Δw`: the change in the synapse's weight
- `e(t)`: the synapse's eligibility trace at time `t`: the STDP changes it has collected, fading with `eligibility_time`
- `c(t)`: the modulator's concentration at the neuron at time `t`
- `∫ … dt`: summed over time (in seconds), so both the size and the duration of the reward count

So a reward that arrives within roughly this time after some activity reinforces that activity.

**Values:** give both `eligibility_time` and `learning_rate`, or neither.

**Note:** synapses need a `learning` rule to be marked at all. Only the neuron's first three-factor receptor is used.",
    ),
    (
        "receptor",
        "learning_rate",
        r"For reward-based learning (with `eligibility_time`): how much the weights change per unit of eligibility × concentration × second.

**Values:** negative values make the modulator act as punishment instead of reward.

**Note:** changes still respect the synapse's `limits`.",
    ),
    (
        "receptor",
        "direct_potential",
        r"Lets the modulator excite (positive) or inhibit (negative) the neuron directly, as if it were an input. While present, `direct_potential × c` is added to the potential when it's compared with the threshold, where `c` is the modulator's concentration at the neuron. It rises and fades with the modulator.

**Note:** it can make the neuron fire between inputs (the moment is computed exactly), but not during the refractory period.",
    ),
    // ----- releases -----
    (
        "modulatorRelease",
        "modulator",
        r"Which modulator (from `modulators`) this neuron releases at each spike.

**Values:** required.",
    ),
    (
        "modulatorRelease",
        "amount",
        r"How much modulator each spike releases (default 1): the concentration added where it's released, before any weakening with distance.

**Note:** concentrations have no unit; receptor gains are per unit of concentration.",
    ),
    // ----- synapse settings -----
    (
        "synapseSettings",
        "weight",
        r"How strong the connection is. Each spike that arrives through it changes the receiver's potential by

`sign × weight × spike_potential`

- `sign`: +1 for excitatory, −1 for inhibitory (see `sign`)
- `spike_potential`: the sending neuron's setting

(with the default spike_potential of 1 mV, the weight is that change in mV).

**Values:** a number, or a random value per synapse: `uniform(low, high)` or `normal(mean, sd)`. Random weights are drawn when the network is built, from the Run page's seed. A negative weight reverses the connection's effect.

**Note:** required, on the connection or its synapse type. Learning, decay and noise change it during a run; `limits` keeps it in bounds.",
    ),
    (
        "synapseSettings",
        "equilibrium",
        r"The weight that `decay` slowly pulls the synapse back to, so learned changes fade toward it.

**Values:** default: the synapse's own starting weight. A distribution draws a separate value for each synapse.

**Ignored:** without `decay`.",
    ),
    (
        "synapseSettings",
        "sign",
        r"Whether the connection excites (`excitatory`) or inhibits (`inhibitory`) the receiver.

**Values:** default: inhibitory if the sending neuron is `inhibitory`, excitatory otherwise. Setting it lets one neuron make both kinds of connection.",
    ),
    (
        "synapseSettings",
        "delay",
        r"The time a spike takes to cross the synapse. The total time from spike to arrival is

`delay + distance / conduction_speed`

- `delay`: this setting
- `distance`: the straight-line distance between the two neurons' positions
- `conduction_speed`: the sending neuron's setting

and never less than 100 µs (the build's minimum delay).

Delays shape timing-dependent behaviour: STDP, resonance, and waves of activity. Longer delays also let shards run more in parallel.

**Values:** default 0.5 ms; not negative.",
    ),
    (
        "synapseSettings",
        "learning",
        r"The STDP rule (a name from `learning_rules`) that changes this synapse's weight according to spike timing.

**Values:** default `flat`, no learning.

**Note:** if the receiving neuron has a three-factor receptor, the rule's changes mark the synapse for reward-based learning instead of changing it directly.",
    ),
    (
        "synapseSettings",
        "stickiness",
        r"A stickiness function (a name from `stickiness`) that protects this synapse's weight from `decay` when it's left alone for a long time. Synapses that are used often decay as usual.

**Values:** default `none`.

**Ignored:** without `decay`.",
    ),
    (
        "synapseSettings",
        "decay",
        r"Makes the weight drift back toward `equilibrium` over time, so learning fades unless it's refreshed:

`w ← w_eq + (w − w_eq) × e^(−Δ/τ)`

- `w`: the weight; `←` means the new weight is computed from the old one
- `w_eq`: the `equilibrium` weight
- `Δ`: the time since the synapse was last used
- `τ`: the decay's `time`

**Note:** applied whenever the synapse is next used (a spike arrives or the receiver fires), covering the time since its last use.

**Values:** default no decay.",
    ),
    (
        "synapseSettings",
        "limits",
        r"Keeps the weight within `min` and `max`. It's re-applied after every change: learning, decay and noise.

**Note:** the bounds also shape learning. `stdp_hard` and `stdp_alpha` stop at them, and `stdp_soft` slows down as the weight gets close.

**Values:** default no limits.",
    ),
    (
        "synapseSettings",
        "release",
        r"Makes the synapse **unreliable**, like a real one: each arriving spike gets through only with some probability, which rises with recent use (facilitation) and falls as the synapse runs out of vesicles (depletion):

`p = 1 − e^(−C × V)`

- `p`: the probability that a spike gets through
- `C`: facilitation, which grows with recent spikes (see `facilitation_base`)
- `V`: available vesicles, used up by recent releases (see `depletion_base`)

With large `C × V`, `p` is close to 1; as `V` runs out, `p` falls toward 0.

**Values:** `release=()` uses standard settings (C₀ = 5, V₀ = 10). With these the synapse nearly always transmits, until a burst wears it out. Default: no `release`, every spike gets through.

**Note:** a spike that isn't released has no effect at all, not even on learning. Random numbers come from the Run page's seed.",
    ),
    (
        "synapseSettings",
        "weight_noise",
        r"Random drift of the weight over time. At each use, the weight changes by

`σ × √(Δ / 1 ms) × N(0, 1)`

- `σ`: this setting
- `Δ`: the time since the synapse was last used
- `N(0, 1)`: a standard normal random number (from the Run page's seed)

before any decay, and is then clamped to the `limits`.

**Tip:** without `decay` or `limits`, the weight wanders without bound.

**Values:** default 0, no noise.",
    ),
    (
        "synapseType",
        "name",
        r"The type's name, used in `type=` on connections and in `extends` on other synapse types.

**Values:** unique.",
    ),
    (
        "synapseType",
        "extends",
        r"Starts this type from another synapse type's settings, so you only write what's different. A setting given here replaces the base's whole setting.

**Note:** a loop of types is an error.",
    ),
    (
        "decay",
        "time",
        r"How quickly the weight returns to equilibrium: after this time, about two-thirds of any learned change has faded (after three times as long, 95%).

**Values:** required.

**Note:** with `stickiness`, synapses left alone fade more slowly.",
    ),
    (
        "limits",
        "min",
        r"The lowest the weight can go.

**Values:** required; not above `max`.

**Note:** a starting weight below it is raised to it the first time the synapse is used.",
    ),
    (
        "limits",
        "max",
        r"The highest the weight can go.

**Values:** required; not below `min`.",
    ),
    // ----- stochastic release -----
    (
        "release",
        "facilitation_base",
        r"C₀, how reliable the synapse is when it hasn't been used recently. Higher means more spikes get through. Recent spikes add to it:

`C = C₀ + Σ facilitation_step × e^(−Δ/facilitation_time)`

- `C`: the current facilitation
- `C₀`: this setting
- `Σ`: a sum over every earlier spike that arrived at the synapse (released or not)
- `Δ`: how long ago that spike arrived

**Values:** default 5.",
    ),
    (
        "release",
        "facilitation_step",
        r"How much each arriving spike (released or not) boosts reliability for the next ones. This is what makes bursts more likely to get through.

**Values:** default 0.1.",
    ),
    (
        "release",
        "facilitation_time",
        r"How long the boost from a spike lasts (its time constant).

**Values:** default 100 ms.",
    ),
    (
        "release",
        "depletion_base",
        r"V₀, the vesicles available when the synapse is fully rested. Each release uses some up:

`V = max(0, V₀ − Σ depletion_step × e^(−Δ/depletion_time))`

- `V`: the vesicles available now
- `V₀`: this setting
- `Σ`: a sum over every earlier successful release
- `Δ`: how long ago that release was

When none are left, nothing is released.

**Values:** default 10.",
    ),
    (
        "release",
        "depletion_step",
        r"How many vesicles each successful release uses. With the defaults (V₀ = 10, step 1), about ten releases in quick succession exhaust the synapse.

**Values:** default 1.",
    ),
    (
        "release",
        "depletion_time",
        r"How quickly used vesicles are replaced (time constant). Longer means a burst leaves the synapse weak for longer.

**Values:** default 100 ms.",
    ),
    // ----- connections -----
    (
        "connection",
        "from",
        r"The neurons that send: populations, sets, neuron names or ranges of names (`in-{1:5:1}`), or a list of them.

**Note:** for `one_to_one`, the order written pairs them with the receivers. Random patterns choose among them.",
    ),
    (
        "connection",
        "to",
        r"The neurons that receive. Written like `from`. The synapses belong to these neurons.",
    ),
    (
        "connection",
        "pattern",
        r"Which senders connect to which receivers:

- `all_to_all` (default): every sender to every receiver.
- `one_to_one`: the first sender to the first receiver, the second to the second, and so on. Needs as many senders as receivers.
- `fan_in`: each receiver gets `count` different senders, picked at random (all of them, if there are fewer).
- `random`: each sender–receiver pair is connected with `probability`.

**Note:** a neuron never connects to itself unless `self=true`. `within` drops pairs that are too far apart (random patterns then choose only among nearby senders). Random choices come from the Run page's seed.",
    ),
    (
        "connection",
        "count",
        r"For `fan_in`: how many senders each receiver gets. If fewer are available (after `within` and `self`), the receiver gets all of them.

**Values:** a whole number; required for `fan_in`, and an error with other patterns.",
    ),
    (
        "connection",
        "probability",
        r"For `random`: the chance that each sender–receiver pair is connected. Each receiver gets about `probability × number of senders` connections.

**Values:** from 0 to 1; required for `random`, and an error with other patterns.",
    ),
    (
        "connection",
        "within",
        r"Only connects neurons at most this far apart (in µm, measured between their positions). Use it for local wiring, such as each neuron connecting only to its neighbours.

**Note:** for `fan_in` and `random`, the random choice is made only among senders within range.

**Values:** default: any distance.",
    ),
    (
        "connection",
        "self",
        r"Lets a neuron that is in both `from` and `to` connect to itself (an autapse).

**Values:** default false.",
    ),
    (
        "connection",
        "type",
        r"A synapse type to take settings from. Settings written on the connection replace the type's.",
    ),
    // ----- learning rules -----
    (
        "learningRule",
        "name",
        r"The rule's name, used in synapses' `learning=`.

**Values:** unique, and not `flat` (built in: no learning).",
    ),
    (
        "learningRule",
        "rule",
        r"Which spike-timing rule to use. All three follow the same idea: a synapse that helped make the receiver fire gets stronger, and one whose spike came just after the receiver fired gets weaker.

- **Strengthening**, when the receiver fires: every spike that arrived through the synapse since the receiver last fired counts, more the closer it was to this spike.
- **Weakening**, when a spike arrives soon after the receiver fired (without making it fire again): more the sooner it came.

The rules differ in their timing windows and in how they treat the weight limits:

- `stdp_hard`: exponential windows; changes simply stop at the synapse's `limits`.
- `stdp_soft`: like `stdp_hard`, but changes shrink as the weight approaches a limit, so weights settle smoothly. Without `limits` it behaves like `stdp_hard`.
- `stdp_alpha`: an alpha-shaped window (`baseline`, `time`, `rate`).

**Note:** a receiver's `plasticity_gain` scales the changes. With three-factor learning, they become marks for reward-based learning instead.",
    ),
    (
        "learningRule",
        "depression",
        r"For `stdp_hard` and `stdp_soft`: how much a badly timed spike weakens the synapse. A spike arriving Δ after the receiver fired, without making it fire again, changes the weight by

`−depression × e^(−Δ/depression_time)`

- `Δ`: the time between the receiver's last spike and this spike's arrival
- `depression`, `depression_time`: settings of this rule

For `stdp_soft`, the change is also multiplied by `w − min`: the weight's distance above its lower limit.

**Values:** required; in weight units, so choose it relative to your weights.",
    ),
    (
        "learningRule",
        "depression_time",
        r"For `stdp_hard` and `stdp_soft`: how long after the receiver fires a spike still weakens the synapse (the window's time constant). Spikes arriving several times later than this barely matter.

**Values:** required.",
    ),
    (
        "learningRule",
        "potentiation",
        r"For `stdp_hard` and `stdp_soft`: how much well-timed spikes strengthen the synapse. When the receiver fires, the weight changes by

`potentiation × Σ e^(−d/potentiation_time)`

- `Σ`: a sum over every spike that came through this synapse since the receiver's previous spike
- `d`: for each of those spikes, how long before the receiver's new spike it arrived (so recent inputs count most)
- `potentiation`, `potentiation_time`: settings of this rule

For `stdp_soft`, the change is also multiplied by `max − w`: the weight's distance below its upper limit.

**Values:** required; in weight units.",
    ),
    (
        "learningRule",
        "potentiation_time",
        r"For `stdp_hard` and `stdp_soft`: how far before the receiver's spike an input still counts for strengthening (the window's time constant). Inputs much earlier than this barely count.

**Values:** required.",
    ),
    (
        "learningRule",
        "baseline",
        r"For `stdp_alpha`: the window's value far from the spike, β. The window is

`α(t) = max(β, e × (1 − β) × x × e^(−x) + β)`, with `x = (t₀ − t) / time`

- `α(t)`: the window's value for a spike `t` away from the receiver's spike
- `β`: this setting, `baseline`
- `e`: Euler's number, 2.718…
- `t₀`: a shift, computed from `β`, that makes α(0) = 0 (no change for exactly simultaneous spikes)
- `time`: this rule's `time` setting

Strengthening sums the window over the inputs since the receiver's previous spike; weakening uses `α(Δ) × e^(−Δ/time)` for a spike arriving Δ after the receiver's spike. Both are multiplied by `rate`.

**Note:** a negative baseline makes poorly timed pairings weaken the synapse.

**Values:** required.",
    ),
    (
        "learningRule",
        "time",
        r"For `stdp_alpha`: the width of the timing window. The window peaks about this long from the spike and fades after a few times this.

**Values:** required.",
    ),
    (
        "learningRule",
        "rate",
        r"For `stdp_alpha`: scales every weight change. Strengthening is never negative and weakening never positive.

**Values:** required; in weight units.",
    ),
    // ----- stickiness -----
    (
        "stickiness",
        "name",
        r"The function's name, used in synapses' `stickiness=`.

**Values:** unique; `none` is built in.",
    ),
    (
        "stickiness",
        "time",
        r"How long a synapse must be left alone before its weight starts to stick. Over a gap Δ, the decay factor `d` becomes

`d + (1 − d) × s(Δ)`, with `s(Δ) = 2 × (1 / (1 + e^(−Δ/time)) − ½)`

- `d`: the fraction of the distance to equilibrium the weight would keep under plain `decay` (`e^(−Δ/τ)`)
- `Δ`: the time since the synapse was last used
- `s(Δ)`: the stickiness, from 0 to 1
- `time`: this setting

`s` runs from 0 for short gaps (normal decay) to 1 for gaps much longer than this time (no decay at all). So synapses that are used often decay normally, while one that isn't used for a long time keeps its weight.

**Values:** required.",
    ),
    // ----- modulators -----
    (
        "modulator",
        "name",
        r"The modulator's name, used by `receptors` and `releases`.

**Values:** unique.",
    ),
    (
        "modulator",
        "clearance_time",
        r"How quickly the modulator is cleared away after a release. Each release that reaches a neuron adds

`c(t) = amplitude × e^(−(t − arrival) / clearance_time)`

- `c(t)`: that release's concentration at the neuron at time `t`
- `amplitude`: the released `amount`, weakened by `falloff` over the distance
- `arrival`: when the release reached the neuron (after `latency` and travel at `speed`)

and the concentration is the sum of all of them. Longer clearance lets releases overlap and build up; shorter makes each one a brief pulse.

**Values:** required; positive.",
    ),
    (
        "modulator",
        "reach",
        r"How far a release spreads, in µm. Neurons farther than this from the releasing neuron aren't affected at all. Use it for local signalling.

**Values:** default: everywhere.",
    ),
    (
        "modulator",
        "speed",
        r"How fast a release spreads. A neuron at distance `d` (from the releasing neuron) receives it `latency + d / speed` after the spike.

**Values:** default: instantly.",
    ),
    (
        "modulator",
        "latency",
        r"The delay between a spike and its release starting to spread.

**Note:** with several shards, the shortest latency in the network (or 1 ms, whichever is less) limits how far ahead shards can run independently. Very short latencies therefore slow down multi-shard runs.

**Values:** default 1 ms.",
    ),
    (
        "modulator",
        "falloff",
        r"Weakens releases with distance. A neuron at distance `d` receives

`amount × e^(−d / falloff)`

- `amount`: what the releasing neuron releases (its `releases.amount`)
- `d`: the distance between the releasing neuron and the receiving one

so at a distance of `falloff`, about a third of the release arrives.

**Values:** default: no weakening (full strength everywhere within `reach`).",
    ),
];

#[cfg(test)]
mod tests {
    use super::DETAILS;
    use crate::schema::RECORDS;

    #[test]
    fn every_key_has_details_and_every_detail_a_key() {
        for r in RECORDS {
            for k in r.keys {
                assert!(
                    DETAILS.iter().any(|(rec, key, _)| *rec == r.name && *key == k.key),
                    "no details for {}.{}",
                    r.name,
                    k.key
                );
            }
        }
        for (rec, key, _) in DETAILS {
            assert!(
                RECORDS.iter().any(|r| r.name == *rec && r.keys.iter().any(|k| k.key == *key)),
                "details for {rec}.{key}, which isn't a key"
            );
        }
    }
}
