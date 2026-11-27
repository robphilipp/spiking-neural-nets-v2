# 05 Inputs: input files, simulated and realtime sources, the Inputs view

Status: **reviewed**, 2026-11-27 (decisions below; the open questions answered). **I1 to I4 done** (2026-11-27). Replaces today's three ways of giving a network input (the Run page's
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

### I3 status: done (2026-11-27)

- **The Inputs page** (`pages/InputsPage.tsx`, parts in `components/inputs/`) replaces the old page; from top to bottom:
  - **Input files**: a selector (the project's files, and "none"; the same `selected_inputs` the Run page uses, so
    the file edited is the file runs get), a file name (renamed on Enter or leaving the field; a taken or empty name
    says why), New, Duplicate, Delete (asks first) and the description. The pure rules (free names, copies,
    renames that keep the selection, what is selected after a delete) are in `inputEdit.ts`.
  - **Sources**: a list (kind, name, targets with the number of neurons they match in the built network, schedule,
    "trial" when it runs in trials) with Add buttons for every kind, and the editor of the selected source: name,
    enabled, scope (when the file has trials), targets (prefix or regex through `NeuronMatchField`; or one of the
    built network's sets), start, duration, and by kind the pulse times and mV, Hz per neuron and mV, period and mV,
    the signal's formula (errors in place, with the character), sample interval and encoding; realtime kinds show
    their encoding and a note that they don't run yet. Move up and down, duplicate and remove are in the editor's
    header. A source's start, duration, mV, Hz and period take the file's notation, so "500 ms", "1 s" and also
    `uniform(500 ms, 1 s)` or `normal(10, 2)` (kept as the random settings of §2.1).
  - **Trials**: on or off, count, length, gap (fixed or random), first trial, order (with 2 or more conditions) and
    the conditions (a name and which trial sources are on in it). Off, trial sources become run sources, because a
    file without `[trials]` can't have trial sources.
  - **Preview**: a **timeline** (a row per source: bars for when it is on, ticks for pulses, the trials numbered above
    and marked across the rows, the run's current time as a red line; to the run's duration if the project has one,
    else to the end of what is scheduled; the trials are drawn with the run's seed, as `expandTrials` gives them to
    the run); the selected signal's **channels** over time (the old page's chart; noise keyed as the engine keys it);
    the selected source's **targets in 3D** (`Network3D` with the targets coloured), or "build the network to see the
    selected source's targets".
  - **Locked while a run is open** (running or paused): every editor is read-only (a `<fieldset disabled>`), with a
    note. Then each source row shows its **state** (scheduled, on, done; "off", "manual" for a pulse source without
    times, "not run yet" for realtime), the current **trial** and condition, and a **Fire** button on run-scope pulse
    sources. Fire is enabled when the network is built with the file as it is now (the session remembers which file,
    as JSON, `builtInputs`, so the Run page's rebuild hint and Fire agree). A trial-scope pulse source has no Fire:
    the engine knows it only as one source per trial.
  - **Realtime (until I4)**: the old page's microphone and "Stream live" (a designed signal streamed by the wall clock
    through the sensor protocol) kept as a section at the bottom, from the old code (`RealtimePanel.tsx`,
    `inputStreams.ts`). The old "Inject before running" is gone: a signal source in the file does that, in simulated
    time and reproducibly.
- **The Run page** keeps the **Input file** selector with its summary, the rebuild hint and the Trial stat; the
  Stimulate and Background rows are replaced by a **Fire** button per run-scope pulse source of the file and an
  **Edit inputs** button to the Inputs page. (The Background switch is a Poisson source named `background` on the
  Inputs page now. `backgroundOf` and `withBackground` in `inputFile.ts` are no longer used by the app.)
- **Decisions** where the plan was silent: the file being edited is the selected one (no separate "current file");
  New makes `inputs-2` (and `-3`, …), Duplicate `<name>-copy`; a new source gets all neurons as targets (an empty
  prefix) so it works as soon as it's added; adding a source selects it; a pulse source with no times is a manual-only
  one (Fire); a signal's channel preview shows the first second of the source (its own `t`, from its start) and,
  with no built network, assumes 4 channels; the timeline is made from the expanded file, so what it shows is what
  Build gives the run.
- **Tests**: vitest `inputEdit.test.ts` (file new, duplicate, rename, delete; sources: new, change with unique names
  and condition renames, remove, duplicate, move; trials on and off; what can be fired; settings as typed;
  schedule text; a source's state at a time, with trials; the timeline's layout, clipping, seeds and ticks;
  "where the run is among its trials"); e2e `inputs-view.spec.ts` (files; a Poisson, a pulse and a signal source, the
  timeline and channel preview, trials, build, run, locked editors, the sources' states, targets in 3D; Fire from the
  Run page); the e2e suite that used the Run page's Background and Stimulate controls (19 files) now uses
  `e2e/helpers.ts` (`addBackground`, `addPulse`) and the Fire buttons; `wta.spec.ts` and `inputs.spec.ts` use signal
  sources instead of "Inject".
- **Not done**: the test run (I5); position mapping of channels (I5); a random setting can't be edited for a
  source's `at` times; `file` sources; the rate preview (event rate per channel, §4); realtime sources in the file
  (I4). No marking of trials on the Run page's charts (as in I2).

### I4 status: done (2026-11-27)

- **File format** (`inputFile.ts`): realtime sources take `encoding` (current, rate, threshold; a sensor also
  spikes), `on_pause = "drop" | "buffer"` (default **drop** for all three: they are live), `speed = "resample" |
  "refuse"` (default resample: the engine's anchor scales the source's time by the run's speed; refuse: it doesn't
  start unless the run is at x1). Microphone: `bands` (optional cap; otherwise one band per target neuron).
  Camera: `mode = "luminance" | "changes"`, `grid = "16x16"` (columns x rows), `level` (changes, default 0.1).
  Changes send 2 x cells channels, all "on" cells (row-major) then all "off" cells, each carrying the frame
  difference when above `level`. Sensor: one channel per target neuron. Defaults aren't written. Realtime sources
  have no schedule (a `start` or `duration` is an error) and can't be trial sources.
- **Manager** (`realtimeManager.ts`, with `realtime.ts` for the store and the host hook mounted in `App`; replaces
  `inputStreams.ts`): several sources at once, each a sensor named after the source with a status (starting,
  streaming, stopped, refused, error; message; samples/s, samples, batches, late, dropped), counters polled from
  `listSensors` every second. It starts the enabled sources when the run is running, holds them across pause and
  resume (`on_pause` goes to the engine as `bufferWhilePaused`), stops them when the run ends or the session
  changes, and follows speed changes (a speed change settles 300 ms first): as fast as possible refuses all,
  `refuse` sources refuse at other speeds than x1 and start again at x1. A source that can't start (too few targets,
  device refused) shows its error and the others go on. Devices: `capture.ts` (getUserMedia with echo
  cancellation, noise suppression and gain control off; audio batches every 10 ms; video per frame, ~30 fps, from a
  canvas at 4x the grid, averaged by `lumaGrid`); the pure channel code is in `realtimeChannels.ts`.
- **Engine and API**: `SensorInfo` has `batches`, `lateBatches` and `dropped`; new request `claimSensor` (session,
  name): a connection takes over a sensor another connection declared (the UI declares the source's sensor, a
  program claims it; see `docs/inputs-sensors.md`); `claim_sensor` in `spikes-sensor` and `claimSensor` in the TS
  client. A `Sensor` is now a shared handle (the entry in the session's list goes with the last one).
- **UI**: realtime editors (settings, encoding, channels vs targets, a live **preview** of the capture: bands as
  bars, the camera's grid, on and off cells green and red); source rows show the channel count or the target-count
  error, and during a run the live status; the timeline shows realtime sources as "live" rows; the Run page says
  how many realtime sources stream, warns when a run as fast as possible has realtime sources, and lists their
  statuses. The old Realtime section (microphone and "Stream live") is gone: a signal streamed live in wall-clock
  time is dropped, simulated signal sources replace it.
- **Desktop**: `Info.plist` (NSMicrophoneUsageDescription, NSCameraUsageDescription) and `Entitlements.plist`
  (audio-input, camera). Untested on macOS.
- **Tests**: vitest `realtimeFile`, `realtimeChannels` (bands, grids, differences, channel counts, speed rules),
  `realtimeManager` (fake client and devices); Rust `sensors.rs` (late and dropped counts, buffer vs drop, claim),
  `connection.rs` (claim through the protocol, listed counters); e2e `realtime.spec.ts` (microphone, camera and a
  claimed sensor at once; as fast as possible; speed refuse; pause drop vs buffer; too few targets; previews), with
  Chromium's fake devices.
- **Not done**: position mapping of channel grids (I5); start/duration for realtime sources; recording of the
  realtime sources' settings with the run beyond what sensors record already.

## 6. Questions answered (2026-11-27)

1. Camera encoding: **both** (decision 4).
2. Changing sources during a run: **no**, for now (decision 5).
3. Protocols: **repeated trials** (decision 6, §2.1).
4. Formulas: **noise and data lookups** (decision 7, §3).
5. Sets as targets: **yes** (decision 8).
6. Realtime sources: **several at once** (decision 9).

### I5a status: test runs (2026-11-27)

- **Engine** (`spikes-session/src/test_run.rs`, `test_run(&Session, TestPlan)`; protocol `testRun`, TS `client.testRun`):
  a request `testRun { session, sources, trials, durationMs, binMs, groups: [{name, neurons}], budgetMs?, seed? }`
  replies `testRun { result: { simMs, elapsedMs, complete, binMs, spikes, counts, groups: [{name, neurons, spikes,
  counts}] } }`. It builds the session's description text in a **throwaway `Engine`** created inside the call (so the
  real session, its state, time, inputs, sensors and the engine's session list are never touched, and nothing of it is
  listed), with the session's last build settings (the defaults if it was never built; weights not recorded; `seed`
  overrides the seed), applies `sources` and `trials` as `set_inputs` does, runs as fast as possible and counts the
  network's spikes from a lossless event subscription into bins of `binMs`, for the whole network and per group (a
  neuron may be in several groups; indices out of range and repeats don't count). Nothing but counts is kept, however
  big the network. It runs on this engine alone, also for a session on a cluster. Refused for a replay. A spike counts
  in the bin of its time (rise time included); one decided just before the end that rises after it isn't counted.
- **Budget**: the run goes in slices of simulated time sized to take about 0.1 s of wall-clock time each, and the budget
  (default 30 s) is checked between them (and a stop is sent if a slice overruns it). Slices, not one long run,
  because a run can only be stopped between synchronization windows, and with one shard a window can be the whole
  run; a test checks that slicing doesn't change what is counted. A run cut by the budget replies `complete: false`
  with `simMs` where it got, and counts only up to there.
- **UI**: a **Test** panel at the bottom of the Inputs page (`components/inputs/TestPanel.tsx`, `RateChart.tsx`; pure
  parts in `testRun.ts`, settings and last result in `testStore.ts`, so they survive moving between pages).
  - Controls: duration (default: the end of the trial schedule, else 5 s; at most 60 s), bin (50 ms), **Run test**,
    and the groups as chips: *each source's targets* (one group per enabled simulated source, named after it; on by
    default), *all neurons*, one chip per set of the network, and groups added by name (`NeuronMatchField`).
  - The test gets the selected file as it is now (unsaved edits included), expanded with the run's seed exactly as Build
    does it (`expandTrials`, `engineSources`) and the seed itself, so the same file and seed give the same result.
    Realtime sources are left out, with a note.
  - Results: a table (group, neurons, spikes, mean rate in Hz per neuron; the whole network last), a rate-per-neuron
    chart over time with the trials shaded and numbered in their conditions' colours (an overlay on a chart with fixed
    margins and no zoom, so it stays aligned), and, when the file has trials, the **trial-averaged response** per
    condition (or all trials): the group rates from each trial's start over the trial's length, averaged, one chart per
    condition. A trial the run didn't finish (budget) is left out, and the budget stop is stated. Results are marked
    **stale** when the file, seed, session or applied description changes.
  - Disabled, with a note, while a run is open (the machine is busy), for a replay, with no applied session, or with no
    input file.
- **Decisions where the plan was silent**: the throwaway is built with the session's own settings plus the run's seed
  (the UI always sends it, so an unbuilt session tests with the seed the next Build would use); a partial last bin has
  its real width in the rates; a trial's start is taken in the bin it falls in (the response is aligned to bins, not
  to the exact start); at most 200,000 bins.
- **Deviation**: the plan's brief said the groups that need a built network (sets, custom groups) are offered only
  when it is built. Target resolution needs a network anyway, so when the session isn't built the panel resolves
  everything in the description's network as written (`previewNetwork`, the same neuron numbering the throwaway
  gets): sets and custom groups are offered then too, and a test needs only an applied description.
- **Tests**: Rust `tests/test_run.rs` (a seed repeats and another differs, on a built and an unbuilt session; the real
  session is untouched, still runs, and runs exactly as an untested one; groups and bins add up, with overlaps and
  bad indices, pulses land in their bins; the budget stops a long run; a replay and bad plans are refused; trials are
  passed through and checked; a test run counts what a plain run does); vitest `testRun.test.ts` (default duration,
  resolving groups, the inputs, rates, summaries, trial responses per condition, the stale key); e2e `test-run.spec.ts`
  (the ring with a Poisson and a pulse source, no network built: counts per source group and the whole network,
  repeatable, session list unchanged, stale after an edit, a custom group, disabled while running, session time
  untouched when built; a trials file: bands and a chart per condition).
- **Not done**: the test can't be cancelled from the UI while it runs (the connection answers when it's done, up to
  the budget); the budget is a fixed 30 s (the request has `budgetMs`, the panel has no field); a test on a
  cluster session runs on this engine alone; no raster of the groups' spikes; the exact (sub-bin) trial alignment.
