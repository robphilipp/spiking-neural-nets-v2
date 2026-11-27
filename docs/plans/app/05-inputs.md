# 05 Inputs: input files, simulated and realtime sources, the Inputs view

Status: **reviewed**, 2026-11-27 (decisions below; the open questions answered). **I1 and I2 done** (2026-11-27). Replaces today's three ways of giving a network input (the Run page's
**Stimulate** and **Background input**, and the Inputs page's designed signal and microphone) with one model and
one view.

## Decisions (from the discussion, 2026-11-27)

1. **Both kinds of input.** *Simulated* signals are specified, generated and delivered in **simulated time**: they
   follow the run at any speed (×0.1, ×1, as fast as possible) and are reproducible from the seed. *Realtime* data
   (voice, video, sensors, anything streamed in) arrives in wall-clock time while the run goes.
2. **One view replaces the current inputs**: the Inputs page, and the Run page's Stimulate and Background controls.
3. **Inputs belong to the project, not the `.boo` file.** A project can have **several input files**; a run uses
   the one selected (so the same network can be run under different stimulus protocols).
4. **Camera: both encodings**: a luminance grid and frame differences (event-camera-like), chosen per source.
5. **No editing while running** (for now): a run is fully described by its input file and seed. Sources can't be
   turned on or off, or changed, during a run; only **Fire** on a `pulse` source (a manual trigger, recorded) acts
   during a run.
6. **Repeated trials**: an input file can describe trials (repetitions, conditions, per-trial randomization) (§2.1).
7. **Formulas get noise and data lookups**: seeded `rand()` and `gauss()`, and `data(…)` from files in the project
   (§3).
8. **Sets as targets**: the network's sets are sent to the UI (an API addition).
9. **Several realtime sources at once** (microphone and camera together, and any number of sensors).

## Where things stand

| Today | Where it runs | Time base | Saved | Reproducible |
|---|---|---|---|---|
| Stimulate (one pulse to matching neurons) | engine, on request | now | no | no (manual) |
| Background input (Poisson) | engine input actor, added at Build | simulated | `project.toml` `[background]` | yes (seed) |
| Designed signal, "Inject before running" | formula sampled in the browser, sent up front as a sensor batch | simulated (pre-scheduled) | no | yes |
| Designed signal, "Stream live" | formula sampled in the browser as the run goes, sensor stream | wall clock | no | no |
| Microphone | browser spectrum → sensor stream | wall clock | no | no (recorded only) |
| External sensor programs | the sensor protocol (`spikes-sensor`, Python) | wall clock | no | recorded |

The engine already has what realtime needs: sensors stream timestamped samples; it anchors their clock to the
simulation's, encodes samples into neuron input (`current`, `rate`, `threshold`, `spikes`), buffers or drops them
while paused, and records them (01 §5). What's missing is on the simulated side (the formula language lives only in
the browser, `signal.ts`), and a model that ties sources, targets and files together.

## 1. The model

An **input file** holds a list of **sources**. A source is *what* (its kind), *where* (its targets), *how* (its
encoding) and *when* (its schedule).

### Targets

- **Neurons**: a match, as now (`prefix` or `regex`), or a **set** of the network (`set = "nociceptive"`).
- **Channels**: a multi-channel source (a signal over `c`, a spectrum, a video grid) maps channel *i* to the *i*-th
  target neuron in network order (today's rule), or **by position**: channels laid out as a 1D or 2D grid are
  assigned to the nearest target neurons along an axis or plane (useful for a camera onto a sheet of neurons).
- Targets are resolved when the network is built; a source whose targets match nothing is an error shown in the
  view.

### Simulated sources (simulated time, engine-generated, deterministic)

| Kind | Settings | Replaces |
|---|---|---|
| `pulse` | times (one or a list), mV | Stimulate, scripted pulses |
| `poisson` | Hz per neuron, mV | Background input |
| `periodic` | period, mV | the engine's `Periodic` input |
| `signal` | a formula over `t`, `c`, `n` (today's language), sample interval, encoding | the designed signal |
| `file` | a recorded sensor stream or a data file (CSV/WAV), played in simulated time, encoding | "a recorded sensor stream can drive a new run" (01 §5) |

All take a **schedule**: `start` (simulated time, default 0), `duration` (default: for ever), `enabled`.

### Realtime sources (wall-clock time, streamed in)

| Kind | Captured by | Becomes channels |
|---|---|---|
| `microphone` | the UI (browser audio) | a log-spaced spectrum, one band per channel (today's) |
| `camera` | the UI (browser video) | per source (decision 4): a downsampled **luminance grid** (a cell's brightness, encoded like any signal), or **frame differences** (an event per cell whose brightness rose or fell by more than a level, on and off channels, like an event camera); grid 16×16 by default (256 channels, or 512 with on/off) |
| `sensor` | any program, by name, over the sensor protocol | the channels it sends |

Realtime sources use the existing sensor path. Per source: `on_pause = "drop" | "buffer"`, and what to do when the
simulation isn't at real time (`speed = "resample"`: scale its time by the speed; or `"refuse"`). Runs *as fast as
possible* can't take realtime input: the view says so and won't start them.

### Encodings

The engine's four, shared by `signal`, `file` and realtime sources: `current` (gain mV per unit), `rate` (Poisson at
value × max Hz, mV per event), `threshold` (an event per level of change, ±mV), `spikes` (events passed through).

## 2. Files

Input files live in the project folder; `project.toml` lists them and which one is selected:

```toml
# project.toml
name = "ring experiments"
network = "network.boo"
inputs = ["baseline.inputs.toml", "touch-tail.inputs.toml", "odour-step.inputs.toml"]
selected_inputs = "touch-tail.inputs.toml"
```

```toml
# touch-tail.inputs.toml
description = "Gentle touch to the tail, then a noxious stimulus"

[[source]]
name = "background"
kind = "poisson"
neurons = { regex = "^(AW|ASE)" }
hz = 2
mv = 12

[[source]]
name = "tail touch"
kind = "signal"
neurons = { set = "touch_posterior" }
formula = "pulse(t - 1000, 2000, 500) * 80"   # 500 ms of touch every 2 s, from 1 s
sample_ms = 1
encoding = { kind = "rate", max_hz = 100, mv = 12 }
start = "1 s"
duration = "10 s"

[[source]]
name = "nose touch"
kind = "pulse"
neurons = { prefix = "ASH" }
at = ["4 s", "4.5 s", "5 s"]
mv = 15

[[source]]
name = "voice"
kind = "microphone"
neurons = { prefix = "AW" }
encoding = { kind = "rate", max_hz = 200, mv = 12 }
on_pause = "drop"
enabled = false
```

- Times use the description language's units (`500 ms`, `1 s`).
- Any number of a source's settings can be random per trial: `uniform(a, b)` or `normal(mean, sd)`, as for weights
  in the description language (§2.1).
- **The browser app** (no folders) keeps input files in the project in memory, with download and upload, like the
  network file today.
- **Migration**: a project with `[background]` gets a `background.inputs.toml` with one `poisson` source, selected.
  `[background]` goes away (no v1 compatibility, so no dual support kept).

### 2.1 Trials

An input file can repeat a stimulus protocol as **trials**. Sources are either **run** sources (on for the whole
run, like background input: the default) or **trial** sources (`scope = "trial"`), whose `start` and `duration` are
relative to each trial's start.

```toml
[trials]
count = 20                  # trials (per condition when there are conditions)
length = "4 s"              # each trial
gap = "uniform(1 s, 2 s)"   # between trials (random per trial)
order = "shuffled"          # with conditions: "shuffled" (from the seed), "blocked" or "alternating"

[[trials.condition]]
name = "tail"
sources = ["tail touch"]    # the trial sources on in this condition

[[trials.condition]]
name = "head"
sources = ["head touch"]

[[source]]
name = "tail touch"
kind = "signal"
scope = "trial"
neurons = { set = "touch_posterior" }
formula = "80"
encoding = { kind = "rate", max_hz = 100, mv = 12 }
start = "uniform(0.5 s, 1 s)"   # random per trial
duration = "500 ms"
```

- Without conditions, every trial source is on in every trial.
- Random values (`gap`, a source's `start`, `mv`, `hz`, …) are drawn per trial from the run's seed, so the same
  file and seed give the same trials.
- The engine records each trial's start, number and condition (§3), so the Run page can show the current trial and
  mark trial starts on the charts, and analyses can align spikes to trials.

## 3. Engine

- **The formula language in Rust** (`spikes-session`, or a small `spikes-signal` crate): the same grammar and
  functions as `signal.ts`, so the view's preview and the engine agree. A test runs both over the same formulas and
  compares samples. (Later, the UI could use the Rust one through WebAssembly, leaving one implementation.) New
  functions (decision 7):
  - `rand()` (uniform in [0, 1)) and `gauss()` (standard normal): a fresh value per sample and channel, from a random
    stream derived from the seed, the source's name and the channel, so runs repeat exactly;
  - `data("name", t, c)`: the value of a data file in the project (CSV: a time column and a column per channel;
    WAV: samples per channel) at time `t` for channel `c`, linearly interpolated; `data("name", t)` for one-column
    data. Files are loaded at Build and checked then (missing file, too few channels).
- **A signal input actor** per `signal`/`file` source: samples in simulated time at `sample_ms`, encodes with the
  sensor encoders, and injects, scheduling itself ahead like the Poisson input. Deterministic: per-source random
  streams derived from the seed and the source's name.
- **`pulse`** becomes scheduled injections; `poisson` and `periodic` are the existing input actors.
- **API**: `setInputs(session, InputFile)` replaces `addInput` and the background handling in `buildNetwork`:
  validated (targets resolved, formulas compiled, data files loaded, errors with the source's name), applied at
  Build, and re-applied by Reset; refused while a run is open (decision 5). `fire(session, source)` triggers a
  `pulse` source now (recorded).
- **Trials**: the engine expands the trial schedule (§2.1) at Build into each source's on-periods, drawing the
  per-trial random values and the condition order from the seed, and records each trial's start, number and
  condition as events (for the views, and for analysis in the Parquet export).
- **Realtime sources** bind to the sensor path: the UI (or a program) connects a sensor named after the source;
  the source supplies its targets and encoding. Any number at once (decision 9): the UI's single "active input"
  becomes a set of streams, each with its own status.
- **Network info** includes the description's sets (name and members), for `set = …` targets in the UI.
- **Recording**: simulated sources are reproducible from the input file and seed (both saved with the recording's
  metadata); realtime ones are recorded as now.

## 4. The Inputs view (replaces the Inputs page and the Run page's input controls)

- **Files**: the project's input files (new, duplicate, rename, delete), and which one is selected for runs. The Run
  page shows the selected file's name, with a menu to switch.
- **Sources**: the file's sources as a list (name, kind, targets with their count, schedule, enabled); add, remove,
  reorder, duplicate; an editor per kind (targets with the prefix/regex/set field; formula with errors in place;
  encoding; schedule).
- **Preview** (before running):
  - a timeline of all sources (when each is on), and each `signal`'s channels over time (today's preview);
  - the targets highlighted in a 3D view of the network;
  - for rate encodings, the resulting event rate per channel.
- **Test**: run the network with these inputs for a few simulated seconds in a throwaway session (like the
  Network page's preview), and show the spikes of the targets and of chosen groups (e.g. `command_interneurons`):
  a quick check that the input does what's intended, without touching the real session.
- **Live, during a run** (read-only, decision 5): each source's state (scheduled, on, done; for realtime:
  connected, samples/s, late or dropped samples), the current trial and condition, and a **Fire** button on `pulse`
  sources (what Stimulate does today; recorded, so a replay matches). The editors are locked while a run is open.

## 5. Phases

| Phase | Delivers |
|---|---|
| **I1** Model and engine | input file format (Rust and TS types), the Rust formula language (with `rand`, `gauss`) and the cross-check test, the signal input actor, `pulse` and Fire, `setInputs`, Build and Reset applying it; sets in the network info; projects list and select input files; `[background]` migrated. The Run page uses the selected file (no editor yet: files edited by hand). |
| **I2** Trials | the trial schedule (count, length, gap, conditions, order, per-trial random values), expanded by the engine, trial events recorded and shown on the Run page (current trial and condition; trial starts on the charts). |
| **I3** The Inputs view | files, the source list and editors, the trial editor, the timeline and channel previews, target highlighting; replaces the Inputs page and the Run page's Stimulate and Background controls. |
| **I4** Realtime | microphone and camera (grid and frame differences) from the view, named sensor sources, several at once, pause and speed handling, live status. |
| **I5** Test runs and data | the throwaway test run; `data(…)` lookups and `file` sources (recorded streams, CSV/WAV) in simulated time; position mapping of channel grids; docs; examples (e.g. nemotode input files). |

Each phase: unit tests (engine and TS), e2e tests through the view, docs.

### I1 status: done (2026-11-27)

- **`spikes-signal`** (new crate): the formula language in Rust (the UI's grammar and functions, `round` as
  JavaScript rounds), `rand()` and `gauss()` (counter-based: a hash of the key, channel, sample and call site, so
  values don't depend on order, threads or shards), and the sample encoders, now shared with sensors (`sensors.rs`
  uses them; same behaviour and random draws). `tests/parity.rs` writes samples of 14 formulas, noise included, to
  `apps/spikes-ui/test/fixtures/signal-parity.json`; `signal.test.ts` checks `signal.ts` against them (equal, up to
  the last bit of `sin`, `log` and the like). `signal.ts` has the same noise in BigInt.
- **Engine**: `PulseInput` and `SignalInput` actors (the signal samples its formula in simulated time, `t` from the
  source's start, encodes, and sends at exact times; saved and restored with snapshots); `InputSpec::Pulses` and
  `Signal`; `Session::set_inputs` (named sources with start times, once per build at time 0, all checked before any
  starts, noise keyed by the build's seed and the source's name) and `Session::fire`; Reset applies the set again;
  cluster nodes get the sources like any input. `NetworkInfo` and the API's `Network` carry the description's sets.
  Protocol: `setInputs`, `fire`, `Input::pulses` and `Input::signal`, `InputSource`, `NeuronSet`.
- **UI**: `inputFile.ts` (the file format, read and written with inline tables; times in units; target resolution
  by prefix, regex or set; the engine's sources); `project.toml` lists input files and the selected one, and a
  `[background]` table becomes `background.inputs.toml`; project folders read and write input files (desktop), the
  browser downloads them. Build gives the selected file's sources to the run. The Run page has an **Input file**
  selector, and its Background controls edit the selected file's `background` source (until the Inputs view, I3);
  realtime sources in a file are listed as not run yet.
- **Tests**: Rust `tests/inputs.rs` (signals drive their channels at their rates; repeat exactly with a seed on 1
  or 4 shards; pulses and start times; once per build, checked first, repeated by Reset; fire; a saved run with a
  noisy signal continues exactly), `tests/cluster.rs` (an input set runs the same on two node agents); TS
  `inputFile.test.ts`, `project.test.ts` (with every sample project), the parity test; e2e `input-files.spec.ts`
  (the background input as a file's source, downloaded; `setInputs` and `fire` through the server); the desktop
  self-test writes and reads an input file.
- `test-data/projects/ring` uses input files (`background.inputs.toml`, and `wave.inputs.toml`: a travelling wave
  and taps).
- **Not yet**: trials (I2), the Inputs view (I3), realtime sources (I4), `data(…)` and file sources (I5). Fire has
  no button until the Inputs view; Stimulate stays on the Run page until then.

### I2 status: done (2026-11-27)

- **Where trials are expanded** (a change from §3): in the UI (`expandTrials` in `inputFile.ts`), not the engine.
  The draws use the engine's counter-based randoms (bit for bit the same in both), keyed by the run's seed, so the
  same file and seed always give the same trials; the engine needs no new input kinds, and gets the expansion as
  plain sources (`<name> #<trial>`) plus the list of trials (`setInputs`' `trials`). An API client can expand the
  same way, or send its own trials.
- **The file**: `[trials]` (`count` per condition, `length`, `gap` fixed or random, `start`, `order`: shuffled from
  the seed, blocked or alternating) and `[[trials.condition]]` (a name, and the trial sources on in it); sources
  get `scope = "trial"` (start and duration from the trial's start; cut at its end); a source's `start`,
  `duration`, `mv`, `hz` and `period`, and the trials' `gap`, can be `uniform(a, b)` or `normal(mean, sd)`, drawn
  per trial (run sources: once).
- **Engine**: `Session::set_inputs` takes the trials with the sources (checked in order; kept by a reset);
  `Session::trials` (as given, or as recorded for a replay); recorded runs get `trials.arrow` (trial, condition,
  start and end, ns), also in the Parquet export. Protocol: `setInputs`' `trials`, `getTrials`, `Trial`.
- **UI**: Build expands the selected file with the run's seed; the Run page shows the current trial ("3 of 8 ·
  column 1", "gap · next 4", "done") from `getTrials` (so also for replays and sessions opened from the picker).
  The browser can open an input file (Projects page), as it could download one.
- **Tests**: Rust: trials kept, checked, kept by a reset, recorded and read back by a replay. TS: reading and
  writing trials and random settings, the checks, the expansion (counts per condition, each order, the timeline
  and gaps, instances within their trials with drawn settings, the same for a seed and not for another). e2e:
  `trials.spec.ts` (the ring project's `trials.inputs.toml` opened, built, run: the Run page's trial, the engine's
  trials, done at the end).
- **Not done**: trial starts aren't marked on the charts (the charts have no markers yet; the Run page shows the
  current trial instead).

## 6. Questions answered (2026-11-27)

1. Camera encoding: **both** (decision 4).
2. Changing sources during a run: **no**, for now (decision 5).
3. Protocols: **repeated trials** (decision 6, §2.1).
4. Formulas: **noise and data lookups** (decision 7, §3).
5. Sets as targets: **yes** (decision 8).
6. Realtime sources: **several at once** (decision 9).
