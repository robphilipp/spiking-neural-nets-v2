import { describe, expect, it } from "vitest";
import {
  addFile,
  addSource,
  changeSource,
  changeTrials,
  deleteFile,
  duplicateFile,
  duplicateSource,
  type FileSet,
  firable,
  moveSource,
  newSource,
  niceTicks,
  parseSetting,
  parseTimes,
  removeSource,
  renameFile,
  scheduleText,
  settingText,
  sourceState,
  timeline,
  trialAt,
  trialTotals,
  withTrials,
} from "../src/inputEdit";
import { expandTrials, type InputFile, InputFileError, parseInputFile, serializeInputFile, type Source } from "../src/inputFile";

const empty = (): InputFile => ({ description: "", sources: [] });
const set = (names: string[], selected: string | null = names[0] ?? null): FileSet => ({
  names,
  files: Object.fromEntries(names.map((n) => [n, empty()])),
  selected,
});

describe("the project's files", () => {
  it("adds a file with a free name, selected", () => {
    const a = addFile(set(["inputs.inputs.toml"]));
    expect(a.name).toBe("inputs-2.inputs.toml");
    expect(a.set.names).toEqual(["inputs.inputs.toml", "inputs-2.inputs.toml"]);
    expect(a.set.selected).toBe(a.name);
    expect(addFile(a.set).name).toBe("inputs-3.inputs.toml");
    expect(addFile(set([]), "touch tail").name).toBe("touch-tail.inputs.toml");
  });

  it("duplicates a file (a deep copy, next to it, selected)", () => {
    const s = set(["a.inputs.toml", "b.inputs.toml"]);
    s.files["a.inputs.toml"] = addSource(empty(), newSource("poisson", []));
    const d = duplicateFile(s, "a.inputs.toml");
    expect(d.name).toBe("a-copy.inputs.toml");
    expect(d.set.names).toEqual(["a.inputs.toml", "a-copy.inputs.toml", "b.inputs.toml"]);
    expect(d.set.selected).toBe(d.name);
    d.set.files[d.name]!.sources[0]!.name = "changed";
    expect(s.files["a.inputs.toml"]!.sources[0]!.name).toBe("poisson");
    expect(duplicateFile(d.set, "a.inputs.toml").name).toBe("a-copy-2.inputs.toml");
  });

  it("renames a file, keeping its place and the selection", () => {
    const s = set(["a.inputs.toml", "b.inputs.toml"], "b.inputs.toml");
    const r = renameFile(s, "b.inputs.toml", "touch");
    expect(r.name).toBe("touch.inputs.toml");
    expect(r.set.names).toEqual(["a.inputs.toml", "touch.inputs.toml"]);
    expect(r.set.selected).toBe("touch.inputs.toml");
    expect(Object.keys(r.set.files).sort()).toEqual(["a.inputs.toml", "touch.inputs.toml"]);
    expect(renameFile(s, "b.inputs.toml", "b").set).toBe(s); // unchanged
    expect(() => renameFile(s, "b.inputs.toml", "a")).toThrow(InputFileError);
    expect(() => renameFile(s, "b.inputs.toml", "  ")).toThrow(InputFileError);
  });

  it("deletes a file; the selection moves to the one before, or none", () => {
    const s = set(["a.inputs.toml", "b.inputs.toml", "c.inputs.toml"], "b.inputs.toml");
    const d = deleteFile(s, "b.inputs.toml");
    expect(d.names).toEqual(["a.inputs.toml", "c.inputs.toml"]);
    expect(d.selected).toBe("a.inputs.toml");
    expect(Object.keys(d.files)).toEqual(["a.inputs.toml", "c.inputs.toml"]);
    expect(deleteFile(s, "a.inputs.toml").selected).toBe("b.inputs.toml"); // another file: unchanged
    expect(deleteFile(set(["a.inputs.toml"]), "a.inputs.toml")).toMatchObject({ names: [], selected: null });
  });
});

describe("a file's sources", () => {
  it("makes new sources of every kind that write and read back", () => {
    let file = empty();
    for (const kind of ["pulse", "poisson", "periodic", "signal", "microphone", "camera", "sensor"] as const) {
      file = addSource(file, newSource(kind, file.sources.map((s) => s.name)));
    }
    expect(file.sources.map((s) => s.name)).toEqual(["pulse", "poisson", "periodic", "signal", "microphone", "camera", "sensor"]);
    expect(parseInputFile(serializeInputFile(file)).sources).toEqual(file.sources);
    expect(newSource("pulse", ["pulse", "pulse 2"]).name).toBe("pulse 3");
  });

  it("changes a source; names stay unique and conditions follow a rename", () => {
    let file = withTrials(empty(), true);
    file = addSource(file, { ...newSource("poisson", []), name: "a", scope: "trial" });
    file = addSource(file, { ...newSource("poisson", ["a"]), name: "b" });
    file = changeTrials(file, { conditions: [{ name: "c", sources: ["a"] }] });
    expect(() => changeSource(file, 1, { name: "a" })).toThrow(/already a source called a/);
    expect(() => changeSource(file, 1, { name: " " })).toThrow(InputFileError);
    const renamed = changeSource(file, 0, { name: "z" });
    expect(renamed.trials!.conditions[0]!.sources).toEqual(["z"]);
    // Leaving the trials drops the source from conditions.
    const run = changeSource(renamed, 0, { scope: undefined });
    expect("scope" in run.sources[0]!).toBe(false);
    expect(run.trials!.conditions[0]!.sources).toEqual([]);
  });

  it("removes, duplicates and moves sources", () => {
    let file = empty();
    for (const k of ["pulse", "poisson", "periodic"] as const) file = addSource(file, newSource(k, file.sources.map((s) => s.name)));
    const names = (f: InputFile) => f.sources.map((s) => s.name);
    expect(names(removeSource(file, 1))).toEqual(["pulse", "periodic"]);
    const d = duplicateSource(file, 0);
    expect(names(d)).toEqual(["pulse", "pulse copy", "poisson", "periodic"]);
    expect(names(duplicateSource(d, 0))).toEqual(["pulse", "pulse copy 2", "pulse copy", "poisson", "periodic"]);
    expect(names(moveSource(file, 2, -1))).toEqual(["pulse", "periodic", "poisson"]);
    expect(moveSource(file, 0, -1)).toBe(file);
    expect(moveSource(file, 2, 1)).toBe(file);
  });

  it("turns trials on and off; off, trial sources become run sources", () => {
    let file = addSource(empty(), { ...newSource("pulse", []), scope: "trial" });
    expect(() => parseInputFile(serializeInputFile(file))).toThrow(/no \[trials\]/); // why it must
    file = withTrials(file, true);
    expect(file.trials).toMatchObject({ count: 5, length: 2000, order: "shuffled" });
    expect(withTrials(file, true)).toBe(file);
    const off = withTrials(file, false);
    expect(off.trials).toBeUndefined();
    expect(off.sources[0]!.scope).toBeUndefined();
    expect(parseInputFile(serializeInputFile(off)).sources).toHaveLength(1);
    expect(trialTotals({ ...file.trials!, count: 3, gap: 500, conditions: [{ name: "a", sources: [] }, { name: "b", sources: [] }] })).toEqual({
      trials: 6,
      endMs: 6 * 2000 + 5 * 500,
    });
  });

  it("lists the pulse sources that can be fired", () => {
    let file = empty();
    file = addSource(file, newSource("pulse", []));
    file = addSource(file, { ...newSource("pulse", ["pulse"]), enabled: false });
    file = addSource(file, { ...newSource("pulse", ["pulse", "pulse 2"]), scope: "trial" });
    file = addSource(file, newSource("poisson", []));
    expect(firable(file).map((s) => s.name)).toEqual(["pulse"]);
    expect(firable(null)).toEqual([]);
  });
});

describe("settings as typed", () => {
  it("reads times, numbers and random values", () => {
    expect(parseSetting("500 ms", true, "start")).toEqual({ value: 500 });
    expect(parseSetting("1.5 s", true, "start")).toEqual({ value: 1500 });
    expect(parseSetting("20", false, "hz")).toEqual({ value: 20 });
    expect(parseSetting("uniform(500 ms, 1 s)", true, "start")).toEqual({ value: 750, random: { uniform: [500, 1000] } });
    expect(parseSetting("normal(10, 2)", false, "mv")).toEqual({ value: 10, random: { normal: [10, 2] } });
    expect(() => parseSetting("soon", true, "start")).toThrow(/must be a time/);
    expect(() => parseSetting("", false, "hz")).toThrow(/must be a number/);
    expect(() => parseSetting("uniform(2 s, 1 s)", true, "start")).toThrow(/low end/);
  });

  it("shows them as they'd be written", () => {
    expect(settingText(1500, undefined, true)).toBe("1.5 s");
    expect(settingText(750, { uniform: [500, 1000] }, true)).toBe("uniform(500 ms, 1 s)");
    expect(settingText(20, undefined, false)).toBe("20");
    expect(parseTimes("500 ms, 1 s,2.5 s", "t")).toEqual([500, 1000, 2500]);
    expect(() => parseTimes("1 s, later", "t")).toThrow(InputFileError);
    expect(parseTimes("", "t")).toEqual([]);
  });

  it("describes a source's schedule", () => {
    const p = newSource("pulse", []) as Source & { kind: "pulse" };
    expect(scheduleText(p)).toBe("at 1 s");
    expect(scheduleText({ ...p, at: [] })).toBe("no times (fire by hand)");
    expect(scheduleText({ ...p, at: [1, 2, 3, 4], start: 500 })).toBe("4 pulses from 1 ms, from 500 ms");
    const q = newSource("poisson", []);
    expect(scheduleText(q)).toBe("from 0 ms for ever");
    expect(scheduleText({ ...q, start: 1000, duration: 2500 })).toBe("from 1 s for 2.5 s");
    expect(scheduleText({ ...q, start: 750, random: { start: { uniform: [500, 1000] } } })).toBe("from uniform(500 ms, 1 s) for ever");
  });
});

const PROTOCOL = `
[trials]
count = 2
length = "2 s"
gap = "1 s"

[[source]]
name = "background"
kind = "poisson"
neurons = { prefix = "in-" }
hz = 5
mv = 12

[[source]]
name = "tap"
kind = "pulse"
neurons = { prefix = "in-" }
at = ["500 ms", "1 s"]
mv = 12
start = "100 ms"

[[source]]
name = "touch"
kind = "signal"
scope = "trial"
neurons = { prefix = "in-" }
formula = "1"
start = "500 ms"
duration = "1 s"
encoding = { kind = "rate", max_hz = 100, mv = 12 }

[[source]]
name = "later"
kind = "poisson"
neurons = { prefix = "in-" }
hz = 5
mv = 12
start = "5 s"
duration = "1 s"

[[source]]
name = "voice"
kind = "microphone"
neurons = { prefix = "in-" }
encoding = { kind = "rate", max_hz = 100, mv = 12 }
`;

describe("a source's state during a run", () => {
  const file = parseInputFile(PROTOCOL);
  const { file: expanded } = expandTrials(file, 1);
  const state = (name: string, ms: number) => sourceState(file.sources.find((s) => s.name === name)!, expanded, ms);

  it("is scheduled, on, then done", () => {
    expect(state("later", 0)).toBe("scheduled");
    expect(state("later", 5000)).toBe("on");
    expect(state("later", 5999)).toBe("on");
    expect(state("later", 6000)).toBe("done");
    expect(state("background", 0)).toBe("on"); // for ever
    expect(state("background", 1e9)).toBe("on");
  });

  it("follows a pulse source from its first to its last pulse", () => {
    expect(state("tap", 0)).toBe("scheduled");
    expect(state("tap", 700)).toBe("on");
    expect(state("tap", 1100)).toBe("on");
    expect(state("tap", 1101)).toBe("done");
  });

  it("is on in a trial source's instances, scheduled between them, done after the last", () => {
    // Trials: 0–2 s and 3–5 s; the source is on 0.5–1.5 s into each.
    expect(state("touch", 0)).toBe("scheduled");
    expect(state("touch", 1000)).toBe("on");
    expect(state("touch", 2000)).toBe("scheduled"); // the next trial's
    expect(state("touch", 3700)).toBe("on");
    expect(state("touch", 5000)).toBe("done");
  });

  it("tells disabled, realtime and manual sources apart", () => {
    expect(state("voice", 0)).toBe("live");
    expect(sourceState({ ...file.sources[0]!, enabled: false }, expanded, 0)).toBe("off");
    const manual = newSource("pulse", []) as Source & { kind: "pulse" };
    expect(sourceState({ ...manual, at: [] }, expanded, 0)).toBe("manual");
  });
});

describe("the timeline", () => {
  const file = parseInputFile(PROTOCOL);
  const row = (t: ReturnType<typeof timeline>, name: string) => t.rows.find((r) => r.name === name)!;

  it("lays out each source's on-periods and pulses, with the trials", () => {
    const t = timeline(file, null, 1);
    expect(t.trials.map((x) => [x.startMs, x.endMs])).toEqual([
      [0, 2000],
      [3000, 5000],
    ]);
    expect(row(t, "tap").marks).toEqual([600, 1100]); // from its start
    expect(row(t, "tap").spans).toEqual([]);
    expect(row(t, "touch").spans).toEqual([
      { from: 500, to: 1500, open: false },
      { from: 3500, to: 4500, open: false },
    ]);
    expect(row(t, "later").spans).toEqual([{ from: 5000, to: 6000, open: false }]);
    // To the end of what's scheduled (6 s), a little over; a source for ever runs to the end.
    expect(t.horizonMs).toBeCloseTo(6300);
    expect(row(t, "background").spans).toEqual([{ from: 0, to: t.horizonMs, open: true }]);
    expect(t.ticks[0]).toBe(0);
    expect(t.ticks.at(-1)!).toBeLessThanOrEqual(t.horizonMs);
  });

  it("runs to the run's duration when it has one, cutting what's beyond", () => {
    const t = timeline(file, 4000, 1);
    expect(t.horizonMs).toBe(4000);
    expect(row(t, "later").spans).toEqual([]);
    expect(row(t, "touch").spans.at(-1)).toEqual({ from: 3500, to: 4000, open: false });
    expect(row(t, "tap").marks).toEqual([600, 1100]);
  });

  it("is 10 s of nothing for an empty file or sources that never end", () => {
    expect(timeline(empty(), null, 1).horizonMs).toBe(10_000);
    expect(timeline(addSource(empty(), newSource("poisson", [])), null, 1).horizonMs).toBe(10_000);
  });

  it("is the same for a seed, with random starts drawn per trial", () => {
    const random = parseInputFile(PROTOCOL.replace('start = "500 ms"', 'start = "uniform(100 ms, 900 ms)"'));
    const a = timeline(random, null, 7);
    expect(timeline(random, null, 7)).toEqual(a);
    const starts = row(a, "touch").spans.map((s) => s.from % 3000); // within each trial
    expect(starts[0]).not.toBe(starts[1]);
    expect(timeline(random, null, 8)).not.toEqual(a);
  });

  it("makes round ticks", () => {
    expect(niceTicks(10_000)).toEqual([0, 2000, 4000, 6000, 8000, 10_000]);
    expect(niceTicks(1000)).toEqual([0, 200, 400, 600, 800, 1000]);
    expect(niceTicks(60_000, 6)).toEqual([0, 10_000, 20_000, 30_000, 40_000, 50_000, 60_000]);
    expect(niceTicks(6300)).toEqual([0, 2000, 4000, 6000]);
  });
});

describe("where a run is among its trials", () => {
  const trials = [
    { number: 1, condition: "a", startMs: 200, endMs: 1200 },
    { number: 2, condition: "b", startMs: 2000, endMs: 3000 },
  ];
  it("says the trial, the gap or done", () => {
    expect(trialAt(trials, 0)).toBe("starts at 0.2 s");
    expect(trialAt(trials, 500)).toBe("1 of 2 · a");
    expect(trialAt(trials, 1500)).toBe("gap · next 2");
    expect(trialAt(trials, 3000)).toBe("done");
  });
});
