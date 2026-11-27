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

### I5b status: data lookups, file sources, position mapping (2026-11-27)

- **`data("name", t, c)`** in formulas, in Rust (`spikes-signal`: `data.rs`, `Formula::eval_with(…, &DataSets)`;
  `eval` keeps working with no data) and in `signal.ts` (`compile(source, data)`), the same results. `t` is the
  formula's `t` (ms from the source's start); values are linearly interpolated between samples; **0 before the first
  sample and after the last, for a channel the file doesn't have or one that isn't a whole number, and for an unknown
  name**; `data("name", t)` is channel 0. Values are kept as 32-bit floats. Data files, in both languages (hand-written
  parsers, no new dependencies): **CSV** (comma, semicolon or tab, from the first line; optional header row: a first row
  with a non-number; `#` lines and blank lines skipped; first column time, in ms unless the header's first cell is
  `t_s` or `time_s`; the other columns are channels; strictly ascending times, equal row widths, numbers only) and
  **WAV** (RIFF/WAVE, PCM 8 unsigned, 16, 24, 32 bit and 32/64 bit float, extensible format too, any channels, time is
  the sample index over the header's rate, integers scaled to [−1, 1)). Errors name the file, the line and the column.
- **Engine**: `InputSpec::Signal` carries the data files its formula uses (`data: Vec<DataFile>`, raw bytes), so
  Reset and saved runs keep them with no other bookkeeping (a saved run records the specs, as before; its format
  changed, which the "no compatibility" rule allows). `Session::set_inputs` parses them and checks, with the source's
  name in the error: a missing file name, a file that doesn't read, and too few channels (`data("x", t, c)` needs one per
  channel of the source, `data("x", t, 3)` four). The signal actor hands its `DataSets` to `eval_with`. **API**:
  `setInputs` has `data: [{ name, base64 }]` (default empty; TS `DataFile`; `client.setInputs(session, sources, trials,
  data)`); only the files a signal uses are kept per signal. **Message size**: the server's WebSocket (axum/tungstenite
  defaults) takes messages up to 64 MiB, so with base64's 4/3 about 48 MB of data per run; the UI refuses more than 40
  MB of data files in one run (`MAX_DATA_BYTES`). The Tauri bridge passes the same JSON in-process; no limit found.
- **Input file**: `[[data]]` (`name`, `file`: a path in the project folder, no `..`, no absolute paths, names unique and
  without quotes), `kind = "file"` (`data`, `gain`, `sample` default 1 ms, `channels` limit, `encoding`, start,
  duration, enabled, scope like other simulated sources). `engineSources` turns a file source into a signal with the
  formula `data("name", t, c)` (times `* gain`), one channel per data channel (limited by `channels`). A source (file or
  signal) naming data that isn't in `[[data]]` is a read error. A WAV is **point-sampled** at the sample interval
  (1 ms by default, not the file's rate: no filtering, so a sound above 500 Hz aliases; lower the interval to follow it).
- **Data in projects**: `useProject.dataFiles` (path → bytes) next to `inputFiles`. Desktop: read with the fs plugin
  (`fs:allow-read-file` added) when the folder opens; a file that is missing stays out and the source says so. "Open a
  data file…" (desktop) picks a file inside the project folder (outside is refused: they are references); in the
  browser it keeps the uploaded bytes in memory under the file's name. The Projects page downloads them with the other
  files. Saving never writes data files, and they don't count as unsaved changes (only the `[[data]]` list does, as part
  of the input file). Build loads and checks them, gives the engine the ones sources use.
- **Position mapping** (`channelLayout.ts`, pure, with the engine unchanged: it still gets `channels: number[][]`): for
  microphone, camera, signal and file sources, `layout = "order"` (default) | `"name"` | `"grid"` | `"axis"`:
  - `grid` (`plane` xy|xz|yz, default xy; `grid = "CxR"`, a camera's own, a signal's or file's setting, default `1xN`):
    the targets' bounding box in the plane is cut into C×R cells; channel (col, row), row-major, y or z increasing with the
    row, drives every target in its cell (a single-point extent goes to the first column or row). A signal's or file's
    grid must have as many cells as it has channels.
  - `axis` (`x|y|z`): the extent along the axis is cut into N equal slabs, slab k is channel k (N: a microphone's bands,
    a signal's or file's channels; a camera's cells).
  - `name`: without `pattern`, channel i is the i-th target in natural name order (digit runs as numbers, otherwise
    case-sensitive text), independent of the description's order; with `pattern` (a **JavaScript** regular expression,
    evaluated in the UI) one capture group is the channel, two are column and row of the grid; numbers count from `base`
    (default 0; 1 for `input-11`); several neurons may share a channel; a non-matching or out-of-range neuron is ignored.
  - A signal's channel count is `channels`, else the grid's cells, else its target count. A camera's `changes` mode: on
    and off channels both map onto the same cells' neurons (under `order` they stay different neurons, as in I4).
  - Warnings in place (the editor and the source list): channels with no neuron, neurons ignored by a name pattern;
    errors (an invalid regular expression, no or more than two groups, two groups without a grid, a grid whose cells don't
    match the channels, too few targets under `order`/`name`) stop the source: the read refuses a bad pattern, the editor
    shows it, Build and the realtime manager refuse it. Under `grid`, `axis` and `name` there is no "needs N target
    neurons" error except for `name` without a pattern (it takes one neuron per channel). The microphone has no `grid`.
  - The realtime manager and the engine sources use it; the old `channelPlan` is replaced by `channelTargets`.
  - **Preview**: the Inputs page's 3D targets view colours the selected source's targets by channel (hues from red to
    violet, the first channel a neuron drives) with a legend "channel 0 … n−1", for every layout but `order`.
- **UI**: data files panel (Open, Remove; name, path, channels, duration, rate or errors per file), a file source editor
  (data picker, gain, sample interval, channel limit, encoding, what the file holds, errors in place for a missing or
  unreadable file, the channels preview chart), `LayoutFields` under signal, file, microphone and camera, a signal's
  `channels` setting, the formula help mentions `data()`.
- **Decisions** where the plan was silent: a data name is the file's name without its extension; one `[[data]]` list per
  input file; `sample` is 1 ms for every file source; values are f32; a signal with `data("x", t, c)` and `layout`
  `order` gets one channel per target (all must exist in the file, or it is refused); the data bytes are not part of the
  "rebuild needed" check (only the `[[data]]` list is).
- **Examples**: `test-data/projects/ring/pulse.csv` and `pulse.inputs.toml` (a file source: three channels on in turn).
- **Tests**: Rust `data.rs` (CSV, WAV, errors, interpolation, bounds, base64), formula `data()`, `parity.rs` (CSV and WAV
  built in the test, written with the formulas to the fixture the TS test loads), `tests/data_inputs.rs` (a signal
  follows the data in simulated time; errors name the source; repeats exactly across seeds and shards; Reset keeps the
  data; a saved run with data continues exactly), `connection.rs` (`setInputs` with base64 data). TS: `signal.test.ts`
  (parity with data, lookups, parsers), `channelLayout.test.ts` (natural order, grid cells, empty cells, slabs, name layout
  with one and two groups, base, shared channels, ignored and out-of-range counts, invalid patterns, file round trips),
  `inputData.test.ts` (`[[data]]` and file sources read and written, path rules, `engineSources` for files, loading,
  what is sent). e2e `data-inputs.spec.ts`: a CSV opened in the browser, a file source, build and run: the targets spike
  in the data's pattern; unreadable data; layouts name, grid (and a finer grid's warning), axis and an invalid pattern on
  the line-sensor sheet, with the 3D channel colouring (screenshots `test-results/layout-name.png` and `layout-grid.png`).
- **Not done**: **`recording` sources** (replaying a recorded run's sensor inputs through `setInputs`): the engine's
  `replay_sensor_inputs` injects events once into a ready session, and it would need the recordings folder in the session,
  an input kind that survives Reset and saved runs, and a start offset; not small, so left out. A WAV is not resampled or
  filtered. Data files are not watched for changes on disk. A source's data can't be a recording of the run's own output.

