import type { Network } from "@spikes/client";
import { describe, expect, it } from "vitest";
import {
  backgroundOf,
  engineSources,
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
