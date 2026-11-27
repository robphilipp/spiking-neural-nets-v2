// Editing input files (app plan 05, the Inputs view): pure helpers over the project's files and over one file's
// sources and trials, so the view stays thin and the rules (unique names, conditions that follow renames) are tested.

import {
  DEFAULT_GRID,
  DEFAULT_LEVEL,
  expandTrials,
  formatRandom,
  formatTime,
  type InputFile,
  inputFileName,
  InputFileError,
  isRealtime,
  meanOf,
  parseRandom,
  parseTime,
  type Random,
  type RandomField,
  type Source,
  type SourceKind,
  type TrialSchedule,
} from "./inputFile";

// ----- the project's files -----

/** The project's input files: their names (as `project.inputs`), the files, and the one runs use. */
export interface FileSet {
  names: string[];
  files: Record<string, InputFile>;
  selected: string | null;
}

/** "touch-tail.inputs.toml" → "touch-tail". */
export const inputLabel = (name: string) => name.replace(/\.inputs\.toml$/, "");

/** `base`, or `base 2`, `base 3`, … for the first not in `taken`. */
export function uniqueName(base: string, taken: Iterable<string>): string {
  const used = new Set(taken);
  if (!used.has(base)) return base;
  for (let n = 2; ; n++) if (!used.has(`${base} ${n}`)) return `${base} ${n}`;
}

/** A file name for the label `label` that no file in `set` has: `label`, else `label-2`, `label-3`, … (a label is made
 *  file-name safe, as `inputFileName` does). */
const freeFileName = (label: string, set: FileSet): string => {
  const base = inputLabel(inputFileName(label.trim() || "inputs"));
  const taken = new Set(set.names.map(inputLabel));
  let name = base;
  for (let n = 2; taken.has(name); n++) name = `${base}-${n}`;
  return inputFileName(name);
};

/** A new empty file, selected. */
export function addFile(set: FileSet, label = "inputs", file: InputFile = { description: "", sources: [] }): { set: FileSet; name: string } {
  const name = freeFileName(label, set);
  return { name, set: { names: [...set.names, name], files: { ...set.files, [name]: file }, selected: name } };
}

/** A copy of `name`, next to it, selected. */
export function duplicateFile(set: FileSet, name: string): { set: FileSet; name: string } {
  const copy = freeFileName(`${inputLabel(name)}-copy`, set);
  const at = set.names.indexOf(name);
  const names = [...set.names];
  names.splice(at + 1, 0, copy);
  return { name: copy, set: { names, files: { ...set.files, [copy]: structuredClone(set.files[name]!) }, selected: copy } };
}

/** `name` under the label `label`. Throws an InputFileError if the label is empty or another file has it. */
export function renameFile(set: FileSet, name: string, label: string): { set: FileSet; name: string } {
  const next = inputFileName(label.trim());
  if (!label.trim()) throw new InputFileError("a file needs a name");
  if (next === name) return { set, name };
  if (set.names.includes(next)) throw new InputFileError(`there's already a file called ${inputLabel(next)}`);
  const files = { ...set.files, [next]: set.files[name]! };
  delete files[name];
  return {
    name: next,
    set: { names: set.names.map((n) => (n === name ? next : n)), files, selected: set.selected === name ? next : set.selected },
  };
}

/** Without `name`; if it was the selected one, the file before it (or the first left) is. */
export function deleteFile(set: FileSet, name: string): FileSet {
  const at = set.names.indexOf(name);
  const names = set.names.filter((n) => n !== name);
  const files = { ...set.files };
  delete files[name];
  return { names, files, selected: set.selected === name ? (names[Math.max(0, at - 1)] ?? null) : set.selected };
}

// ----- a file's sources -----

const REALTIME_ENCODING = { type: "rate", maxHz: 200, mv: 12 } as const;
const REALTIME_DEFAULTS = { onPause: "drop", speed: "resample" } as const;

/** A new source of `kind` with sensible settings, named for the kind (made unique among `taken`). */
export function newSource(kind: SourceKind, taken: string[]): Source {
  const base = { name: uniqueName(kind, taken), targets: { prefix: "" }, start: 0, duration: null, enabled: true };
  switch (kind) {
    case "pulse":
      return { ...base, kind, at: [1000], mv: 12 };
    case "poisson":
      return { ...base, kind, hz: 5, mv: 12 };
    case "periodic":
      return { ...base, kind, period: 100, mv: 12 };
    case "signal":
      return { ...base, kind, formula: "pulse(t, 500, 100)", sample: 1, encoding: { type: "rate", maxHz: 100, mv: 12 } };
    case "microphone":
      return { ...base, kind, encoding: { ...REALTIME_ENCODING }, ...REALTIME_DEFAULTS };
    case "camera":
      return { ...base, kind, mode: "luminance", grid: { ...DEFAULT_GRID }, level: DEFAULT_LEVEL, encoding: { type: "rate", maxHz: 100, mv: 12 }, ...REALTIME_DEFAULTS };
    case "sensor":
      return { ...base, kind, encoding: { ...REALTIME_ENCODING }, ...REALTIME_DEFAULTS };
  }
}

/** Trial conditions name only trial sources that exist: drop the rest. */
function tidy(file: InputFile): InputFile {
  if (!file.trials) return file;
  const trialSources = new Set(file.sources.filter((s) => s.scope === "trial").map((s) => s.name));
  const conditions = file.trials.conditions.map((c) => ({ ...c, sources: c.sources.filter((n) => trialSources.has(n)) }));
  return { ...file, trials: { ...file.trials, conditions } };
}

/** Adds `source` at the end. */
export const addSource = (file: InputFile, source: Source): InputFile => ({ ...file, sources: [...file.sources, source] });

/** Source `index` changed. A new name carries into the trial conditions that use it. Throws an InputFileError if the
 *  name is empty or another source has it. */
export function changeSource(file: InputFile, index: number, change: Partial<Source>): InputFile {
  const old = file.sources[index]!;
  if (change.name !== undefined && change.name !== old.name) {
    if (!change.name.trim()) throw new InputFileError("a source needs a name");
    if (file.sources.some((s, i) => i !== index && s.name === change.name)) throw new InputFileError(`there's already a source called ${change.name}`);
  }
  const sources = file.sources.map((s, i) => (i === index ? ({ ...s, ...change } as Source) : s));
  let next: InputFile = { ...file, sources };
  if (change.name !== undefined && change.name !== old.name && next.trials) {
    const conditions = next.trials.conditions.map((c) => ({ ...c, sources: c.sources.map((n) => (n === old.name ? change.name! : n)) }));
    next = { ...next, trials: { ...next.trials, conditions } };
  }
  // A "run" scope is the absence of one.
  if (change.scope === undefined && "scope" in change) {
    const only = { ...next.sources[index]! };
    delete only.scope;
    next = { ...next, sources: next.sources.map((s, i) => (i === index ? only : s)) };
  }
  return tidy(next);
}

export const removeSource = (file: InputFile, index: number): InputFile => tidy({ ...file, sources: file.sources.filter((_, i) => i !== index) });

/** A copy of source `index` right after it. */
export function duplicateSource(file: InputFile, index: number): InputFile {
  const s = file.sources[index]!;
  const copy = structuredClone({ ...s, name: uniqueName(`${s.name} copy`, file.sources.map((x) => x.name)) }) as Source;
  const sources = [...file.sources];
  sources.splice(index + 1, 0, copy);
  return { ...file, sources };
}

/** Source `index` moved up (-1) or down (+1) the list. */
export function moveSource(file: InputFile, index: number, by: -1 | 1): InputFile {
  const to = index + by;
  if (to < 0 || to >= file.sources.length) return file;
  const sources = [...file.sources];
  [sources[index], sources[to]] = [sources[to]!, sources[index]!];
  return { ...file, sources };
}

// ----- trials -----

export const defaultTrials = (): TrialSchedule => ({ count: 5, length: 2000, gap: 500, start: 0, order: "shuffled", conditions: [] });

/** Trials on or off. Off, trial sources become run sources (a file without [trials] can't have trial sources). */
export function withTrials(file: InputFile, on: boolean): InputFile {
  if (on) return file.trials ? file : { ...file, trials: defaultTrials() };
  const sources = file.sources.map((s) => {
    const run = { ...s };
    delete run.scope;
    return run as Source;
  });
  const next = { ...file, sources };
  delete next.trials;
  return next;
}

/** The trial schedule changed. */
export const changeTrials = (file: InputFile, change: Partial<TrialSchedule>): InputFile =>
  file.trials ? tidy({ ...file, trials: { ...file.trials, ...change } }) : file;

/** How many trials the schedule makes in all, and when the last one ends if gaps are at their means. */
export function trialTotals(t: TrialSchedule): { trials: number; endMs: number } {
  const trials = t.count * Math.max(1, t.conditions.length);
  const gap = typeof t.gap === "number" ? t.gap : meanOf(t.gap);
  return { trials, endMs: t.start + trials * t.length + (trials - 1) * gap };
}

// ----- times and random values, as typed -----

/** A number or time as typed, possibly random: "500 ms", "1 s", "uniform(500 ms, 1 s)", "normal(10, 2)". */
export function parseSetting(text: string, time: boolean, what: string): { value: number; random?: Random } {
  const random = parseRandom(text, time, what);
  if (random) return { value: meanOf(random), random };
  if (time) return { value: parseTime(text, what) };
  const n = Number(text);
  if (text.trim() === "" || !Number.isFinite(n)) throw new InputFileError(`${what} must be a number (got ${JSON.stringify(text)})`);
  return { value: n };
}

/** A setting as shown: its random form if it has one. */
export const settingText = (value: number, random: Random | undefined, time: boolean): string =>
  random ? formatRandom(random, time) : time ? formatTime(value) : String(value);

/** A list of times as typed: "500 ms, 1 s, 2.5 s" (ascending or not; the engine sorts them). */
export function parseTimes(text: string, what: string): number[] {
  return text
    .split(",")
    .map((t) => t.trim())
    .filter(Boolean)
    .map((t) => parseTime(t, what));
}

export const timesText = (times: number[]): string => times.map(formatTime).join(", ");

// ----- status during a run -----

export type SourceState = "off" | "live" | "manual" | "no trials" | "scheduled" | "on" | "done";

/** The concrete instances of source `s` in an expanded file (a trial source: one per trial it's on in). */
function instancesOf(s: Source, expanded: InputFile): Source[] {
  if (s.scope !== "trial") return expanded.sources.filter((x) => x.name === s.name);
  const prefix = `${s.name} #`;
  return expanded.sources.filter((x) => x.name.startsWith(prefix) && /^\d+$/.test(x.name.slice(prefix.length)));
}

/** From and to (ms of the run) of an instance (a pulse source: from its first to its last pulse, both included). */
function spanOf(s: Source): { from: number; to: number; closed: boolean } {
  if (s.kind === "pulse") return { from: s.start + Math.min(...s.at), to: s.start + Math.max(...s.at), closed: true };
  return { from: s.start, to: s.duration === null ? Infinity : s.start + s.duration, closed: false };
}

/** Where source `s` is at simulated time `simMs`: scheduled, on or done (several instances: on if any is). `expanded`
 *  is the file as the run gets it ({@link expandTrials}). */
export function sourceState(s: Source, expanded: InputFile, simMs: number): SourceState {
  if (!s.enabled) return "off";
  if (isRealtime(s)) return "live"; // streams while the run goes; its live status comes from the realtime manager
  if (s.kind === "pulse" && s.at.length === 0 && s.scope !== "trial") return "manual";
  const spans = instancesOf(s, expanded).map(spanOf);
  if (!spans.length) return "no trials";
  const states = spans.map(({ from, to, closed }) => (simMs < from ? "scheduled" : simMs < to || (closed && simMs === to) ? "on" : "done"));
  return states.includes("on") ? "on" : states.includes("scheduled") ? "scheduled" : "done";
}

// ----- the timeline -----

export interface TimelineRow {
  name: string;
  kind: SourceKind;
  enabled: boolean;
  /** When it's on (ms of the run; an open end is cut at the horizon). */
  spans: Array<{ from: number; to: number; open: boolean }>;
  /** Pulses (ms). */
  marks: number[];
  /** A realtime source: on while the run goes, with no schedule to draw. */
  live: boolean;
}

export interface Timeline {
  /** The time the chart shows to, ms. */
  horizonMs: number;
  rows: TimelineRow[];
  trials: Array<{ number: number; condition: string; startMs: number; endMs: number }>;
  /** Tick positions along the time axis, ms. */
  ticks: number[];
}

/** Round tick positions covering 0…`horizonMs` (about `target` of them, at 1, 2 or 5 × a power of ten ms). */
export function niceTicks(horizonMs: number, target = 6): number[] {
  const raw = horizonMs / target;
  const pow = 10 ** Math.floor(Math.log10(raw));
  const step = [1, 2, 5, 10].map((m) => m * pow).find((s) => s >= raw) ?? pow * 10;
  const ticks: number[] = [];
  for (let t = 0; t <= horizonMs + 1e-9; t += step) ticks.push(Math.round(t * 1000) / 1000);
  return ticks;
}

/**
 * When each source of the file is on across the run, as the run will have it: trials expanded with `seed` (each
 * trial source's instances on its row, the trials listed), random settings drawn. The chart runs to `runMs` (the
 * run's duration) when given, else to the end of what's scheduled (at least 1 s; 10 s when everything is for ever).
 */
export function timeline(file: InputFile, runMs: number | null, seed: number): Timeline {
  const { file: expanded, trials } = expandTrials(file, seed);
  const rows = file.sources.map((s): TimelineRow => {
    const spans: TimelineRow["spans"] = [];
    const marks: number[] = [];
    for (const x of isRealtime(s) ? [] : instancesOf(s, expanded)) {
      if (x.kind === "pulse") marks.push(...x.at.map((t) => x.start + t));
      else spans.push({ from: x.start, to: x.duration === null ? Infinity : x.start + x.duration, open: x.duration === null });
    }
    return { name: s.name, kind: s.kind, enabled: s.enabled, spans, marks: marks.sort((a, b) => a - b), live: isRealtime(s) };
  });
  const finite = [
    ...rows.flatMap((r) => [...r.marks, ...r.spans.flatMap((p) => [p.from, p.to].filter(Number.isFinite))]),
    ...trials.map((t) => t.endMs),
  ];
  const content = finite.length ? Math.max(...finite) : 0;
  const horizonMs = runMs !== null && runMs > 0 ? runMs : content > 0 ? Math.max(1000, content * 1.05) : 10_000;
  const clipped = rows.map((r) => ({
    ...r,
    spans: r.spans.filter((p) => p.from < horizonMs).map((p) => ({ ...p, to: Math.min(p.to, horizonMs), open: p.open })),
    marks: r.marks.filter((t) => t <= horizonMs),
  }));
  return { horizonMs, rows: clipped, trials: trials.filter((t) => t.startMs < horizonMs), ticks: niceTicks(horizonMs) };
}

/** The pulse sources of a file that can be fired by hand during a run: enabled run sources (a trial source is given
 *  to the engine as one source per trial, under other names). */
export const firable = (file: InputFile | null): Source[] => (file?.sources ?? []).filter((s) => s.kind === "pulse" && s.enabled && s.scope !== "trial");

/** When a source is on, in words: "from 1 s for 10 s", "from 0 ms for ever", "2 pulses at 1 s, 2 s". Random settings show as typed. */
export function scheduleText(s: Source): string {
  const time = (field: RandomField, v: number) => settingText(v, s.random?.[field], true);
  if (s.kind === "pulse") {
    const at = s.at.length === 0 ? "no times (fire by hand)" : s.at.length <= 3 ? `at ${timesText(s.at)}` : `${s.at.length} pulses from ${formatTime(Math.min(...s.at))}`;
    return s.start ? `${at}, from ${time("start", s.start)}` : at;
  }
  const to = s.duration === null && !s.random?.duration ? "for ever" : `for ${time("duration", s.duration ?? 0)}`;
  return `from ${time("start", s.start)} ${to}`;
}

/** Where `ms` falls among the run's trials: "3 of 40 · tail", "gap · next 4", "done". */
export function trialAt(trials: Array<{ number: number; condition: string; startMs: number; endMs: number }>, ms: number): string {
  const i = trials.findIndex((t) => ms < t.endMs);
  if (i < 0) return "done";
  const t = trials[i]!;
  if (ms < t.startMs) return i === 0 ? `starts at ${(t.startMs / 1000).toFixed(1)} s` : `gap · next ${t.number}`;
  return `${t.number} of ${trials.length}${t.condition ? ` · ${t.condition}` : ""}`;
}
