import type { Network } from "@spikes/client";
import { describe, expect, it } from "vitest";
import {
  backgroundOf,
  engineSources,
  expandTrials,
  formatTime,
  InputFileError,
  type InputFile,
  parseInputFile,
  parseTime,
  serializeInputFile,
  withBackground,
} from "../src/inputFile";

const EXAMPLE = `
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
formula = "pulse(t, 2000, 500) * 0.8"
sample = "1 ms"
encoding = { kind = "rate", max_hz = 100, mv = 12 }
start = "1 s"
duration = "10 s"

[[source]]
name = "nose touch"
kind = "pulse"
neurons = { prefix = "ASH" }
at = ["4 s", "4.5 s", 5000]
mv = 15

[[source]]
name = "voice"
kind = "microphone"
neurons = { prefix = "AW" }
encoding = { kind = "rate", max_hz = 200, mv = 12 }
enabled = false
`;

const network = {
  neurons: ["AWAL", "AWAR", "ASEL", "ASHL", "ASHR", "PLML", "PLMR", "PVM"].map((name, index) => ({ index, name })),
  sets: [{ name: "touch_posterior", members: [7, 5, 6] }],
} as unknown as Network;

describe("input files", () => {
  it("read the plan's example, times in units", () => {
    const f = parseInputFile(EXAMPLE);
    expect(f.description).toMatch(/^Gentle touch/);
    expect(f.sources.map((s) => [s.name, s.kind, s.enabled])).toEqual([
      ["background", "poisson", true],
      ["tail touch", "signal", true],
      ["nose touch", "pulse", true],
      ["voice", "microphone", false],
    ]);
    const touch = f.sources[1]!;
    expect([touch.start, touch.duration]).toEqual([1000, 10_000]);
    expect(f.sources[2]).toMatchObject({ at: [4000, 4500, 5000], mv: 15 });
  });

  it("round-trip", () => {
    const f = parseInputFile(EXAMPLE);
    expect(parseInputFile(serializeInputFile(f))).toEqual(f);
  });

  it("say what's wrong, naming the source", () => {
    const bad = (body: string) => () => parseInputFile(`[[source]]\nname = "s"\n${body}`, "x.inputs.toml");
    expect(bad('kind = "poisson"\nneurons = { prefix = "a" }\nhz = 0\nmv = 1')).toThrow(/x.inputs.toml: source 's': hz/);
    expect(bad('kind = "signal"\nneurons = { prefix = "a" }\nformula = "sin(t"\nencoding = { kind = "rate", max_hz = 1, mv = 1 }')).toThrow(
      /formula: expected '\)'/,
    );
    expect(bad('kind = "pulse"\nneurons = { prefix = "a", regex = "b" }\nmv = 1')).toThrow(/neurons must be/);
    expect(bad('kind = "bell"\nneurons = { prefix = "a" }')).toThrow(/kind must be/);
    expect(bad('kind = "pulse"\nneurons = { prefix = "a" }\nmv = 1\nstart = "soon"')).toThrow(InputFileError);
    expect(() => parseInputFile('[[source]]\nname = "a"\nkind = "pulse"\nneurons = { prefix = "x" }\nmv = 1\n'.repeat(2))).toThrow(/two sources are named 'a'/);
  });

  it("convert times", () => {
    expect(parseTime("500 ms", "t")).toBe(500);
    expect(parseTime("1.5 s", "t")).toBe(1500);
    expect(parseTime("2 min", "t")).toBe(120_000);
    expect(parseTime(250, "t")).toBe(250);
    expect(formatTime(1500)).toBe("1.5 s");
    expect(formatTime(250)).toBe("250 ms");
  });

  it("become the engine's sources: enabled, simulated, targets resolved", () => {
    const sources = engineSources(parseInputFile(EXAMPLE), network);
    expect(sources.map((s) => s.name)).toEqual(["background", "tail touch", "nose touch"]); // the microphone: off (and realtime)
    expect(sources[0]).toEqual({ name: "background", startMs: 0, input: { type: "poisson", targets: [0, 1, 2], rateHz: 6, mv: 12, durationMs: null } });
    expect(sources[1]!.input).toMatchObject({ type: "signal", channels: [[5], [6], [7]], sampleMs: 1, durationMs: 10_000 });
    expect(sources[1]!.startMs).toBe(1000);
    expect(sources[2]!.input).toEqual({ type: "pulses", targets: [3, 4], atMs: [4000, 4500, 5000], mv: 15 });
  });

  it("refuse to run a source that reaches nothing", () => {
    const f = parseInputFile('[[source]]\nname = "lost"\nkind = "pulse"\nneurons = { prefix = "ZZZ" }\nmv = 1');
    expect(() => engineSources(f, network)).toThrow(/input 'lost': no neurons have names starting with 'ZZZ'/);
    const g = parseInputFile('[[source]]\nname = "set"\nkind = "pulse"\nneurons = { set = "missing" }\nmv = 1');
    expect(() => engineSources(g, network)).toThrow(/input 'set': the network has no set 'missing'/);
  });

  it("hold the Run page's background input as a source", () => {
    const empty: InputFile = { description: "", sources: [] };
    expect(backgroundOf(empty)).toBeNull();
    const fallback = { name: "background", kind: "poisson" as const, targets: { prefix: "in-" }, hz: 5, mv: 12, start: 0, duration: null, enabled: false };
    const on = withBackground(empty, { enabled: true }, fallback);
    expect(backgroundOf(on)).toMatchObject({ enabled: true, hz: 5, targets: { prefix: "in-" } });
    const faster = withBackground(on, { hz: 20 }, fallback);
    expect(faster.sources).toHaveLength(1);
    expect(backgroundOf(faster)?.hz).toBe(20);
  });
});

const TRIALS = `
[trials]
count = 3
length = "4 s"
gap = "uniform(1 s, 2 s)"
start = "500 ms"
order = "shuffled"

[[trials.condition]]
name = "tail"
sources = ["tail touch"]

[[trials.condition]]
name = "head"
sources = ["head touch"]

[[source]]
name = "background"
kind = "poisson"
neurons = { prefix = "AW" }
hz = 2
mv = 12

[[source]]
name = "tail touch"
kind = "pulse"
scope = "trial"
neurons = { set = "touch_posterior" }
at = ["0 ms", "100 ms"]
mv = "uniform(10, 14)"
start = "uniform(500 ms, 1 s)"
duration = "500 ms"

[[source]]
name = "head touch"
kind = "signal"
scope = "trial"
neurons = { prefix = "ASH" }
formula = "1"
encoding = { kind = "rate", max_hz = 50, mv = 12 }
start = "3.5 s"
`;

describe("trials", () => {
  it("read and write, with conditions and random settings", () => {
    const f = parseInputFile(TRIALS);
    expect(f.trials).toEqual({
      count: 3,
      length: 4000,
      gap: { uniform: [1000, 2000] },
      start: 500,
      order: "shuffled",
      conditions: [
        { name: "tail", sources: ["tail touch"] },
        { name: "head", sources: ["head touch"] },
      ],
    });
    const tail = f.sources[1]!;
    expect(tail.scope).toBe("trial");
    expect(tail.random).toEqual({ mv: { uniform: [10, 14] }, start: { uniform: [500, 1000] } });
    expect(tail.start).toBe(750); // the mean, for display
    expect(parseInputFile(serializeInputFile(f))).toEqual(f);
  });

  it("are checked", () => {
    expect(() => parseInputFile('[[source]]\nname = "x"\nkind = "pulse"\nscope = "trial"\nneurons = { prefix = "a" }\nmv = 1')).toThrow(
      /trial source, but there's no \[trials\]/,
    );
    expect(() => parseInputFile(TRIALS.replace('sources = ["head touch"]', 'sources = ["background"]'))).toThrow(
      /names 'background', which isn't a trial source/,
    );
    expect(() => parseInputFile(TRIALS.replace('order = "shuffled"', 'order = "random"'))).toThrow(/order must be/);
    expect(() => parseInputFile(TRIALS.replace('mv = "uniform(10, 14)"', 'mv = "uniform(14, 10)"'))).toThrow(/low end is above/);
  });

  it("expand into sources and trials, the same for the same seed", () => {
    const f = parseInputFile(TRIALS);
    const a = expandTrials(f, 7);
    expect(a).toEqual(expandTrials(f, 7));
    expect(a.trials.map((t) => t.number)).toEqual([1, 2, 3, 4, 5, 6]);
    // Each condition count times.
    expect(a.trials.filter((t) => t.condition === "tail")).toHaveLength(3);
    expect(a.trials.filter((t) => t.condition === "head")).toHaveLength(3);
    // The timeline: from 500 ms, 4 s each, gaps of 1–2 s.
    expect(a.trials[0]!.startMs).toBe(500);
    for (let k = 0; k < a.trials.length; k++) {
      const t = a.trials[k]!;
      expect(t.endMs - t.startMs).toBe(4000);
      if (k > 0) {
        const gap = t.startMs - a.trials[k - 1]!.endMs;
        expect(gap).toBeGreaterThanOrEqual(1000);
        expect(gap).toBeLessThanOrEqual(2000);
      }
    }
    // One instance per trial of its condition, within its trial, with drawn settings; run sources once.
    const names = a.file.sources.map((s) => s.name);
    expect(names[0]).toBe("background");
    for (const t of a.trials) {
      const own = a.file.sources.filter((s) => s.name.endsWith(` #${t.number}`));
      expect(own.map((s) => s.name)).toEqual([`${t.condition} touch #${t.number}`]);
      const s = own[0]!;
      expect(s.start).toBeGreaterThanOrEqual(t.startMs);
      expect(s.start + (s.duration ?? 0)).toBeLessThanOrEqual(t.endMs);
      expect(s.scope).toBeUndefined();
      expect(s.random).toBeUndefined();
      if (s.kind === "pulse") {
        expect(s.mv).toBeGreaterThanOrEqual(10);
        expect(s.mv).toBeLessThanOrEqual(14);
        expect(s.duration).toBe(500);
      } else {
        expect(s.start - t.startMs).toBe(3500);
        expect(s.duration).toBe(500); // to the trial's end
      }
    }
    // Another seed: another order, other draws.
    expect(expandTrials(f, 8)).not.toEqual(a);
  });

  it("follow the order asked for", () => {
    const f = parseInputFile(TRIALS);
    const order = (o: "blocked" | "alternating") => expandTrials({ ...f, trials: { ...f.trials!, order: o } }, 1).trials.map((t) => t.condition);
    expect(order("blocked")).toEqual(["tail", "tail", "tail", "head", "head", "head"]);
    expect(order("alternating")).toEqual(["tail", "head", "tail", "head", "tail", "head"]);
  });

  it("without conditions, turn every trial source on in every trial", () => {
    const f = parseInputFile(TRIALS);
    const g = expandTrials({ ...f, trials: { ...f.trials!, conditions: [], gap: 250 } }, 1);
    expect(g.trials).toHaveLength(3);
    expect(g.trials.map((t) => t.condition)).toEqual(["", "", ""]);
    expect(g.trials[1]!.startMs).toBe(500 + 4000 + 250);
    expect(g.file.sources.filter((s) => s.name !== "background")).toHaveLength(6);
  });

  it("become the engine's sources", () => {
    const { file, trials } = expandTrials(parseInputFile(TRIALS), 3);
    const sources = engineSources(file, network);
    expect(sources).toHaveLength(1 + trials.length);
    expect(sources.every((s) => s.startMs >= 0)).toBe(true);
  });
});
