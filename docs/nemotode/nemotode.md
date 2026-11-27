# nemotode: the C. elegans nervous system as a spiking network

`test-data/examples/nemotode.boo` (in the app: Network page › Open an example › **nemotode**) is the nervous system
of a *Caenorhabditis elegans* hermaphrodite, the worm's "female" form (the male has 385 neurons): all **302
neurons**, at their real positions, wired as in the published connectome. This note says where it comes from, how
it was built and tuned, how to drive it, and what it can't do.

| | |
|---|---|
| Neurons | 302 (all of the hermaphrodite's, including its sensory neurons) |
| Synapses | 5,811: 3,638 chemical connections and 1,086 gap junction pairs (both directions) |
| Sets | 17, by sense and role (below) |
| File | 6,193 lines, generated (each neuron's and connection's line carries its source data as a comment) |

## Sources

All public, through the OpenWorm project:

- **Connectome**: Cook, S. J. et al. (2019), "Whole-animal connectomes of both *Caenorhabditis elegans* sexes",
  *Nature* 571:63–71, as the hermaphrodite edge list in OpenWorm's c302:
  [`c302/data/herm_full_edgelist.csv`](https://github.com/openworm/c302/blob/master/c302/data/herm_full_edgelist.csv)
  (source, target, count, `chemical` or `electrical`).
- **Neuron types and transmitters**: owmeta's export in c302,
  [`c302/data/owmeta_cache.json`](https://github.com/openworm/c302/blob/master/c302/data/owmeta_cache.json)
  (`neuron_info`: sensory / interneuron / motor, and neurotransmitters).
- **Positions**: the soma of each neuron's morphology in OpenWorm's CElegansNeuroML,
  [`CElegans/generatedNeuroML2/<name>.cell.nml`](https://github.com/openworm/CElegansNeuroML/tree/master/CElegans/generatedNeuroML2)
  (segment 0, in µm). The repository's `CellsOnly.net.nml` places every cell at the origin, so the positions come
  from the 302 morphology files.

## How the data became a network

- **Names**: the edge list zero-pads motor neuron numbers (`AS01`, `VB03`); the neuron table doesn't (`AS1`,
  `VB3`). Names were normalised to the neuron table's.
- **Neurons only**: 1,573 of the edge list's 7,379 rows connect to muscles (`dBWML1`, …) or other end organs
  (`anal`, `hyp`, …). There are no muscles in the network, so they were left out.
- **Chemical synapses** (3,638 neuron-to-neuron connections): one connection each, `type=chemical` (1 ms synaptic
  delay), weight **0.2 mV per synapse** counted in the electron micrographs, up to 12 mV. Counts average 5.7 and
  reach 71.
- **Gap junctions**: the language has no electrical synapses, so each becomes a fast excitatory synapse,
  `type=gap` (0.1 ms delay, `sign=excitatory`), weight **0.3 mV per junction**, up to 6 mV. The edge list gives
  them in both directions (2,168 directed edges); the 5 listed one way only were mirrored: 2,173 directed
  connections, 1,086 pairs.
- **Signs**: the GABAergic neurons (DD1–6, VD1–13, RMED/L/R/V, RIS, AVL, DVB) use the `gabaergic` neuron type
  (`inhibitory=true`), so their chemical synapses inhibit. All others excite. (Gap junctions excite whatever the
  sender.)
- **Autapses**: 47 neurons connect to themselves in the data; those connections have `self=true`.
- **Unconnected**: CANL and CANR have no connections in this dataset; they're included all the same.
- **Positions**: the morphologies' long axis is y. To lay the worm across the 3D view, the file uses **x = along
  the body** (head at negative x, about −310 µm, to the tail at about +410 µm), **y = ventral to dorsal**, and
  **z = left to right**. (A rotation of the data: distances, and so conduction delays, are unchanged.)
- **Neuron settings** (`neuron` type): threshold 10 mV, decay time 50 ms, refractory period 10 ms, conduction speed
  0.5 m/s, other settings at their defaults (membrane noise 0.1 mV, spike potential 1 mV).

## Sets

Use them in connections, or as a guide to which neurons to stimulate:

| Set | Neurons | What they sense or do |
|---|---|---|
| `sensory`, `interneurons`, `motor` | 111, 119, 147 | owmeta's classification (a neuron can be in more than one) |
| `chemosensory` | ASE, AWA, AWB, AWC, ASH, ADL, ASK, ASI, ASJ, ADF, ASG, PHA, PHB | smell, taste, pheromones |
| `olfactory` | AWA, AWB, AWC | volatile odours |
| `salt` | ASE | water-soluble attractants |
| `thermosensory` | AFD | temperature |
| `touch_anterior` | ALML, ALMR, AVM | gentle touch to the front |
| `touch_posterior` | PLML, PLMR, PVM | gentle touch to the back |
| `nociceptive` | ASH, ADL, FLP, PVD | harsh touch and noxious stimuli |
| `nose_touch` | OLQ, IL1, CEP | touch to the nose |
| `oxygen_co2` | URX, AQR, PQR, BAG | oxygen (URX, AQR, PQR) and CO₂ (BAG) |
| `command_interneurons` | AVA, AVB, AVD, AVE, PVC | locomotion commands: AVA, AVD, AVE backward; AVB, PVC forward |
| `forward_motor` | VB1–11, DB1–7 | excitatory motor neurons for crawling forward |
| `backward_motor` | VA1–12, DA1–9 | excitatory motor neurons for crawling backward |
| `inhibitory_motor` | DD1–6, VD1–13 | GABAergic motor neurons that relax the opposite muscles |
| `pharynx` | I1–I6, M1–M5, MC, MI, NSM | the feeding system, nearly separate from the rest |

(A class name stands for its left/right, dorsal/ventral or numbered members: `ASH` is ASHL and ASHR.)

## Tuning the weights

The per-synapse weights decide whether activity stays near where it starts or takes over the whole worm. They were
chosen by driving one sense at a time and measuring every group's firing rate (Poisson input into every neuron of a
set at 20 Hz per neuron, 12 mV per event; 5 s of simulated time):

| Weights (mV per synapse / per junction) | What happened |
|---|---|
| 1.0 / 0.5 | Runaway: whatever was driven, every group fired at about 70 Hz (all but the pharynx). |
| 0.5 / 0.25 | Still widespread: noxious input drove every group to 20–80 Hz. |
| 0.3 / 0.15 | Contained, and noxious input favoured backward movement (backward motor 28 Hz, forward 9 Hz), but touch reached nothing. |
| 0.2 / 0.1 | Specific (noxious: command interneurons 27 Hz, backward motor 6.5 Hz, forward 0.5 Hz), but odours and touch barely got through. |
| √count scaling (several scales) | Didn't help touch. |
| 0.2 / 0.6 to 1.0 | Touch got through, but strong gap junctions made the whole worm fire together: forward and backward alike. |
| **0.2 / 0.3** | **Chosen**: specific, senses reach the command interneurons, never self-sustaining (below). |

Touch is the hard case: the touch receptor neurons reach their command interneurons mostly through a few gap
junctions (PLMR→PVCR 8 junctions, PLML→PVCL 4), which matter in the worm but carry little weight here.

## What the chosen network does

Same method as the tuning (input to a whole set, 20 Hz per neuron, 12 mV; mean firing rate per group over 5 s):

| Input | All neurons | Command interneurons | Backward motor | Forward motor |
|---|---|---|---|---|
| none | 0.0 Hz | — | — | — |
| a 50 ms kick into every neuron (2 kHz) | dies away: 1 Hz averaged over 5 s (≈ 5 spikes each, all during the kick) | | | |
| `nociceptive` (ASH, ADL, FLP, PVD) | 13.2 Hz | 45.4 Hz | 26.2 Hz | 13.8 Hz |
| `olfactory` (AWA, AWB, AWC) | 7.4 Hz | 24.1 Hz | 13.0 Hz | 6.2 Hz |
| `thermosensory` (AFD) | 1.9 Hz | 7.5 Hz | 3.5 Hz | 1.3 Hz |
| `touch_anterior` at 50 Hz | 2.3 Hz | 10.9 Hz | 5.0 Hz | 1.9 Hz |
| `touch_anterior` at 100 Hz | 5.5 Hz | 22.3 Hz | 11.9 Hz | 5.0 Hz |
| `touch_posterior` at 50 Hz | 1.3 Hz | 4.7 Hz | 2.3 Hz | 1.3 Hz |
| `touch_posterior` at 100 Hz | 4.0 Hz | 15.3 Hz | 7.8 Hz | 4.1 Hz |

So: silent without input; noxious input drives the command interneurons and mostly the backward motor neurons, the
worm's escape reversal; odours and temperature reach the command interneurons too; touch needs 50–100 Hz.

## Driving it in the app

1. **Network page**: Open an example › **nemotode**, then **Apply**.
2. **Run page**, before **Build** (background input is added when the network is built; change it afterwards and
   Build lights up to say it needs a rebuild):

   | Setting | Value |
   |---|---|
   | Background input | on |
   | Neurons starting with | a sensory class, e.g. `ASH` |
   | Hz per neuron | `20` (touch: `50`–`100`) |
   | mV | `12` (above the 10 mV threshold: every input event fires its neuron, refractory period permitting) |
   | Shards, Seed | 2, 1 (the defaults) |

3. **Build**, then **Start** (speed ×1, or *As fast as possible*).

The run checked in the app used exactly this with `ASH` at 20 Hz: 302 neurons, 5,811 synapses, bursts spreading
through the network, nothing dropped.

Background input and stimulation pick neurons by **name prefix**, not by set, so only some rows of the table above
can be repeated exactly:

| Row | Driven in the measurement | "Neurons starting with" | Same? |
|---|---|---|---|
| odours | AWA, AWB, AWC | `AW` | yes |
| temperature | AFD | `AFD` | yes |
| anterior touch | ALM, AVM | `ALM` | ALM only |
| posterior touch | PLM, PVM | `PLM` | PLM only |
| noxious | ASH, ADL, FLP, PVD | `ASH` | ASH only: a weaker response than the table |

To single out a neuron, use **Stimulate neurons starting with** (e.g. `ASHL`, 10–15 mV) during a run.

### Touching the tail (or the head)

The tail's gentle-touch neurons are PLML, PLMR and PVM (the `touch_posterior` set); the head's are ALML, ALMR and
AVM (`touch_anterior`).

**How it was tested.** Not in the app, but with a small benchmark program (since deleted) that built the network
in the engine, gave steady Poisson input to every neuron of the set (12 mV per event, at 20, 50 and 100 Hz per
neuron), ran 5 s of simulated time as fast as possible, and counted each group's spikes:

| Input to the touch neurons | Command interneurons | Backward motor | Forward motor |
|---|---|---|---|
| tail (`touch_posterior`), 20 Hz | nothing gets past the touch neurons | — | — |
| tail, 50 Hz | 4.7 Hz | 2.3 Hz | 1.3 Hz |
| tail, 100 Hz | 15.3 Hz | 7.8 Hz | 4.1 Hz |
| head (`touch_anterior`), 50 Hz | 10.9 Hz | 5.0 Hz | 1.9 Hz |
| head, 100 Hz | 22.3 Hz | 11.9 Hz | 5.0 Hz |

**Repeating it in the app.** Background input, set before **Build**:

| Setting | Tail | Head |
|---|---|---|
| Background input | on | on |
| Neurons starting with | `PLM` | `ALM` |
| Hz per neuron | `50`–`100` | `50`–`100` |
| mV | `12` | `12` |

Then **Build** and **Start**, and follow `PVC`, `AVA`, `AVD` and the motor neurons (`VA`, `VB`, …) in the raster's
**Show neurons starting with**. The prefix leaves out PVM (or AVM), which the test drove too, so expect a somewhat
weaker response than the table.

**A single tap isn't enough.** One **Stimulate** (`PLM`, 12 mV) fires PLML and PLMR once each, but that doesn't
reach the next neuron: their gap junctions onto PVC carry 1.2 mV (PLML, 4 junctions) and 2.4 mV (PLMR, 8) against
a 10 mV threshold. It takes sustained input for those to add up.

**"Reacts" means the command interneurons and motor neurons fire, not that the direction is right.** A real worm
crawls forward when its tail is touched (and backward when its head is). Here, with all but the GABA synapses
excitatory, both drive the backward motor neurons more than the forward ones (see Limits).

### Watching the response

The Run page shows spikes and the population rate, not per-group rates. Type a prefix in the raster's **Show
neurons starting with** to follow one group:

- `AVA` (backward command), `AVB` (forward command), `PVC`;
- `VA` or `DA` (backward motor), `VB` or `DB` (forward motor);
- the driven sensory neurons themselves (`ASH`, …).

In the 3D view the worm is long and thin (its neurons span about 720 µm along the body and 125 µm across):
scroll to zoom into the head, where most neurons are; hover a neuron for its state, click it to inspect it.

## Sensory neurons, input neurons and tonic firing

**The worm's own sensory neurons are in the network.** ASH, AWC, AFD, ALM, PLM and the other sensory neurons are
part of the 302 (111 of them are in the `sensory` set), with their real connections. A stimulus goes straight into
them, by name prefix (`ASH`, `PLM`, …).

**No extra input neurons were added.** When the network was made, one option was to add input neurons that aren't
part of the worm, such as "chemical gradient", "touch front", "touch back" or "temperature", each wired onto the
worm's sensory neurons for that sense. They would give the app's sensors, or the Run page's stimulation, one named
place to feed a stimulus in. The choice was to leave them out and keep the network to the worm's own 302 neurons.

**No neuron fires tonically.** Every neuron uses the default dynamics, `monostable_integrator`: it fires only when
its input pushes it over the 10 mV threshold, then resets. With no input the whole network is silent, and all its
activity comes from what is fed in (background input, stimulation or sensors).

For steady or tonic activity:

- **Steady input**: background input on a sensory class makes those neurons fire continuously at about the rate
  set, as if the stimulus were always there. That's how the tests above drove the network.
- **Tonic dynamics**: giving sensory neurons `dynamics=bistable_integrator` makes them switch into steady firing at
  `tonic_rate` once excited, until their potential decays below `tonic_off_threshold`: a memory of a strong
  stimulus, not a model of any particular worm neuron.

Some real C. elegans sensory neurons do signal tonically; the oxygen-sensing URX neurons are the usual example,
while oxygen stays high. But most of the worm's neurons use graded potentials rather than spikes, so tonic firing
here would be a modelling choice, not something the connectome provides.

## Limits

- **Spikes, not graded potentials.** Most C. elegans neurons signal with graded potentials; few spike. This is a
  spiking abstraction of the real wiring, not a model of the worm's physiology.
- **Signs.** Only the GABAergic neurons inhibit. In the worm, many glutamatergic (and some cholinergic) synapses
  inhibit through chloride channels, depending on the receiving neuron's receptors, which the connectome doesn't
  give. So direction choices that depend on inhibition aren't reproduced: touching the tail doesn't favour forward
  movement the way it does in the worm, and noxious input drives forward motor neurons too, if less.
- **Gap junctions** are approximated as fast excitatory synapses both ways: no electrical coupling, no
  rectification.
- **No neuromodulation** (dopamine, serotonin, neuropeptides) and **no muscles or body**: motor neuron activity is
  the output.
- **Weights** are one number per synapse count, tuned for the behaviour above; nothing in the connectome gives
  real synaptic strengths.

## Regenerating

The file was written by a script (not in the repository) that reads the three sources above. To rebuild or retune
it:

1. Download `herm_full_edgelist.csv` and `owmeta_cache.json` from c302's `c302/data/`, and the 302
   `<name>.cell.nml` files (names from `CellsOnly.net.nml`'s populations) from CElegansNeuroML's
   `CElegans/generatedNeuroML2/`.
2. For each neuron: its type and transmitters from `neuron_info`; its soma from segment 0's `proximal` point;
   position `[y, z, x]` of the morphology's `(x, y, z)`.
3. For each edge between two neurons (names normalised): a `chemical` connection weighing 0.2 mV × count (≤ 12), or
   a `gap` connection weighing 0.3 mV × count (≤ 6); mirror gap junctions listed one way only; `self=true` for
   autapses.
4. Neurons with GABA among their transmitters get the `gabaergic` type.

Changing the two weights (0.2, 0.3) is the main knob: higher spreads activity further, until it runs away (see the
tuning table).
