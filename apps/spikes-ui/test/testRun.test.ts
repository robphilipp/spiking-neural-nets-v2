import type { Network, TestRunResult } from "@spikes/client";
import { describe, expect, it } from "vitest";
import { parseInputFile } from "../src/inputFile";
import {
  binWidth,
  conditionNames,
  customName,
  defaultDurationMs,
  emptySelection,
  MAX_DURATION_MS,
  rates,
  resolveGroups,
  summarize,
  testInputs,
  testKey,
  trialResponses,
} from "../src/testRun";

const network = {
  neurons: ["AWA", "AWB", "ASH", "ASK", "AVA", "AVB"].map((name, index) => ({ name, index })),
  sets: [{ name: "command", members: [5, 4] }],
} as unknown as Network;

const file = parseInputFile(`
[[source]]
name = "background"
kind = "poisson"
neurons = { prefix = "AW" }
hz = 5
mv = 10

[[source]]
name = "tap"
kind = "pulse"
neurons = { regex = "^AS" }
at = ["10 ms"]
mv = 15

[[source]]
name = "off"
kind = "pulse"
neurons = { prefix = "AV" }
mv = 15
enabled = false

[[source]]
name = "voice"
kind = "microphone"
neurons = { prefix = "AV" }
encoding = { kind = "rate", max_hz = 100, mv = 10 }
`);

describe("the duration", () => {
  it("is 5 s without trials and the end of the last trial with them, to the cap", () => {
    expect(defaultDurationMs([])).toBe(5000);
    const t = (startMs: number, endMs: number) => ({ number: 1, condition: "", startMs, endMs });
    expect(defaultDurationMs([t(200, 1200), t(1500, 2500.4)])).toBe(2501);
    expect(defaultDurationMs([t(0, 500_000)])).toBe(MAX_DURATION_MS);
  });
});

describe("resolving groups", () => {
  it("makes one group per enabled simulated source, named after it", () => {
    const { groups, problems } = resolveGroups(emptySelection(), file, network);
    expect(problems).toEqual([]);
    expect(groups).toEqual([
      { name: "background", neurons: [0, 1] },
      { name: "tap", neurons: [2, 3] },
    ]);
  });

  it("adds sets (sorted), all neurons and custom matches, with unique names", () => {
    const { groups, problems } = resolveGroups(
      { sources: false, sets: ["command", "missing"], all: true, custom: [{ mode: "prefix", pattern: "AV" }, { mode: "regex", pattern: "^A[SV]" }, { mode: "prefix", pattern: "zz" }, { mode: "prefix", pattern: "" }] },
      file,
      network,
    );
    expect(groups.map((g) => g.name)).toEqual(["command", "all neurons", "starts with AV", "/^A[SV]/"]);
    expect(groups[0]!.neurons).toEqual([4, 5]);
    expect(groups[1]!.neurons).toHaveLength(6);
    expect(groups[3]!.neurons).toEqual([2, 3, 4, 5]);
    expect(problems).toEqual(["the network has no set 'missing'", "no neurons have names starting with 'zz'"]);
  });

  it("renames a group whose name is taken, and reports a bad regular expression", () => {
    const sel = { sources: true, sets: [], all: false, custom: [{ mode: "regex" as const, pattern: "(" }] };
    const named = resolveGroups({ ...sel, custom: [] }, { ...file, sources: [...file.sources, { ...file.sources[1]!, name: "tap" }] }, network);
    expect(named.groups.map((g) => g.name)).toEqual(["background", "tap", "tap (2)"]);
    expect(resolveGroups(sel, file, network).problems[0]).toMatch(/regular expression/);
    expect(customName({ mode: "prefix", pattern: "AW" })).toBe("starts with AW");
  });
});

describe("the inputs of a test", () => {
  it("are the simulated sources as a run gets them, naming the realtime ones left out", () => {
    const inputs = testInputs(file, 1, network);
    expect(inputs.sources.map((s) => s.name)).toEqual(["background", "tap"]);
    expect(inputs.trials).toEqual([]);
    expect(inputs.skipped).toEqual(["voice"]);
  });

  it("expand the trials with the seed, as Build does", () => {
    const trials = parseInputFile(`
[trials]
count = 2
length = "1 s"
gap = "uniform(100 ms, 200 ms)"

[[trials.condition]]
name = "a"
sources = ["tap"]

[[trials.condition]]
name = "b"
sources = ["tap"]

[[source]]
name = "tap"
kind = "pulse"
scope = "trial"
neurons = { prefix = "AS" }
at = ["0 ms"]
mv = 15
`);
    const a = testInputs(trials, 7, network);
    expect(a.trials).toHaveLength(4);
    expect(a.sources).toHaveLength(4);
    expect(testInputs(trials, 7, network)).toEqual(a);
    expect(testInputs(trials, 8, network).trials.map((t) => t.condition)).not.toEqual([]);
  });
});

// A run of 1 s in bins of 100 ms, as the engine counts it: group "g" has 2 neurons, "h" has 4.
const result = (over: Partial<TestRunResult> = {}): TestRunResult => ({
  simMs: 1000,
  elapsedMs: 12,
  complete: true,
  binMs: 100,
  spikes: 60,
  counts: [6, 6, 6, 6, 6, 6, 6, 6, 6, 6],
  groups: [
    { name: "g", neurons: 2, spikes: 40, counts: [0, 8, 0, 8, 0, 8, 0, 8, 0, 0] },
    { name: "h", neurons: 4, spikes: 0, counts: new Array<number>(10).fill(0) },
  ],
  ...over,
});

describe("rates", () => {
  it("are spikes per neuron per second in each bin", () => {
    const r = result();
    expect(rates(r, r.groups[0]!.counts, 2)).toEqual([0, 40, 0, 40, 0, 40, 0, 40, 0, 0]);
    expect(rates(r, [1], 0)).toEqual([0]);
  });

  it("account for a last bin cut short", () => {
    const r = result({ simMs: 950, binMs: 100 });
    expect(binWidth(r, 9)).toBe(50);
    expect(rates(r, [0, 0, 0, 0, 0, 0, 0, 0, 0, 5], 1)[9]).toBe(100);
  });

  it("are summarised per group, with the network last", () => {
    const rows = summarize(result(), 6);
    expect(rows.map((x) => [x.name, x.neurons, x.spikes])).toEqual([["g", 2, 40], ["h", 4, 0], ["whole network", 6, 60]]);
    expect(rows[0]!.meanHz).toBe(20);
    expect(rows[2]!.meanHz).toBe(10);
  });
});

describe("trial responses", () => {
  const trial = (number: number, condition: string, startMs: number, endMs: number) => ({ number, condition, startMs, endMs });

  it("average the trials of each condition from their starts", () => {
    // Condition a: trials at 100 and 300 ms (g spikes in bin 1 and 3: the 2nd bin of each); b: trial at 500.
    const trials = [trial(1, "a", 100, 300), trial(2, "b", 500, 700), trial(3, "a", 300, 500)];
    const out = trialResponses(result(), trials);
    expect(out.map((c) => [c.condition, c.trials, c.bins])).toEqual([["a", 2, 2], ["b", 1, 2]]);
    const a = out[0]!.groups.find((g) => g.name === "g")!;
    // Trial 1 covers bins 1,2 (40, 0), trial 3 bins 3,4 (40, 0): the mean is 40, 0.
    expect(a.rates).toEqual([40, 0]);
    expect(out[1]!.groups[0]!.rates).toEqual([40, 0]);
    expect(out[0]!.groups[1]!.rates).toEqual([0, 0]);
  });

  it("average all trials without conditions, and leave out those the run didn't finish", () => {
    const r = result({ simMs: 650, complete: false });
    const out = trialResponses(r, [trial(1, "", 100, 300), trial(2, "", 500, 700)]);
    expect(out).toHaveLength(1);
    expect(out[0]!.condition).toBe("");
    expect(out[0]!.trials).toBe(1);
    expect(trialResponses(r, [trial(1, "", 600, 900)])).toEqual([]);
    expect(trialResponses(r, [])).toEqual([]);
  });

  it("list conditions in order of first appearance", () => {
    expect(conditionNames([trial(1, "b", 0, 1), trial(2, "a", 1, 2), trial(3, "b", 2, 3)])).toEqual(["b", "a"]);
  });
});

describe("the key of a result", () => {
  it("changes with the file, the seed, the session and its description", () => {
    const k = testKey("a", file, 1, 3, "net");
    expect(testKey("a", file, 1, 3, "net")).toBe(k);
    expect(testKey("a", { ...file, description: "x" }, 1, 3, "net")).not.toBe(k);
    expect(testKey("a", file, 2, 3, "net")).not.toBe(k);
    expect(testKey("a", file, 1, 4, "net")).not.toBe(k);
    expect(testKey("a", file, 1, 3, "other net")).not.toBe(k);
  });
});
