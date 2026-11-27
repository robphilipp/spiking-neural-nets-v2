import type { Network } from "@spikes/client";
import { describe, expect, it } from "vitest";
import { captureGroups, channelColor, channelOfNeurons, channelTargets, type ChannelSource, layoutProblem, naturalCompare } from "../src/channelLayout";
import { parseInputFile, serializeInputFile, type Source } from "../src/inputFile";

const net = (neurons: Array<[string, number, number, number]>): Network =>
  ({ neurons: neurons.map(([name, x, y, z], index) => ({ index, name, position: [x, y, z] })) }) as unknown as Network;

/** A 4×3 sheet of neurons named s<col>-<row> in the xy plane, listed in a scrambled order. */
const sheet = () => {
  const cells: Array<[string, number, number, number]> = [];
  for (let r = 0; r < 3; r++) for (let c = 0; c < 4; c++) cells.push([`s${c}-${r}`, c * 10, r * 10, 0]);
  return net([...cells].reverse());
};
const all = (n: Network) => n.neurons.map((x) => x.index);

const source = (body: string, kind = "signal"): ChannelSource =>
  parseInputFile(`[[source]]\nname = "s"\nkind = "${kind}"\nneurons = { prefix = "" }\nformula = "c"\nencoding = { kind = "rate", max_hz = 1, mv = 1 }\n${body}`).sources[0] as ChannelSource;
const names = (n: Network, channels: number[][]) => channels.map((c) => c.map((i) => n.neurons[i]!.name).sort());

describe("natural order", () => {
  it("compares digit runs as numbers and other text as case-sensitive", () => {
    const sorted = ["input-10", "input-2", "input-1", "Input-3", "a", "input-2b", "input-02"].sort(naturalCompare);
    expect(sorted).toEqual(["Input-3", "a", "input-1", "input-02", "input-2", "input-2b", "input-10"].sort(naturalCompare));
    expect(["input-10", "input-2", "input-1"].sort(naturalCompare)).toEqual(["input-1", "input-2", "input-10"]);
    expect(naturalCompare("B", "a")).toBeLessThan(0); // case-sensitive: upper case first
    expect(naturalCompare("x", "x")).toBe(0);
  });
});

describe("the default layout: order", () => {
  it("channel i drives the i-th target, and needs enough targets", () => {
    const n = sheet();
    const r = channelTargets(source('channels = 5'), n, all(n));
    expect(r).toMatchObject({ ok: true, empty: 0, ignored: 0 });
    expect(r.ok && r.channels).toEqual([[0], [1], [2], [3], [4]]);
    expect(channelTargets(source('channels = 20'), n, all(n))).toEqual({ ok: false, error: "signal needs 20 target neurons, has 12" });
    expect(channelTargets(source(""), n, [])).toEqual({ ok: false, error: "signal has no target neurons" });
    const whole = channelTargets(source(""), n, all(n));
    expect(whole.ok && whole.channels).toHaveLength(12); // one per target by default
  });
});

describe("grid", () => {
  it("cuts the targets' box into cells, row-major, y increasing with the row", () => {
    const n = sheet();
    const r = channelTargets(source('layout = "grid"\ngrid = "2x3"'), n, all(n));
    expect(r.ok).toBe(true);
    if (!r.ok) return;
    expect(names(n, r.channels)).toEqual([
      ["s0-0", "s1-0"],
      ["s2-0", "s3-0"],
      ["s0-1", "s1-1"],
      ["s2-1", "s3-1"],
      ["s0-2", "s1-2"],
      ["s2-2", "s3-2"],
    ]);
    expect(r.empty).toBe(0);
  });

  it("leaves cells with no neuron empty, and counts them", () => {
    const n = sheet();
    const r = channelTargets(source('layout = "grid"\ngrid = "8x1"'), n, all(n));
    expect(r.ok && r.empty).toBe(4); // 4 columns of neurons fill 4 of 8 cells
    expect(r.ok && r.channels).toHaveLength(8);
  });

  it("uses another plane, and defaults to one column", () => {
    const n = net([["a", 0, 5, 0], ["b", 0, 5, 10], ["c", 0, 5, 20]]);
    const r = channelTargets(source('layout = "grid"\nplane = "yz"\ngrid = "1x3"'), n, all(n));
    expect(r.ok && r.channels).toEqual([[0], [1], [2]]);
    const dflt = channelTargets(source('layout = "grid"\nchannels = 3'), n, all(n)); // 1x3 in xy: y is flat, all one row cell
    expect(dflt.ok && dflt.channels.flat().sort()).toEqual([0, 1, 2]);
    expect(channelTargets(source('layout = "grid"\ngrid = "2x2"\nchannels = 3'), n, all(n))).toEqual({ ok: false, error: "grid 2x2 has 4 cells, the signal has 3 channels" });
  });

  it("maps a camera's own grid; changes map on and off onto the same cells", () => {
    const n = sheet();
    const cam = source('mode = "luminance"\ngrid = "2x1"\nlayout = "grid"', "camera");
    const luminance = channelTargets(cam, n, all(n));
    expect(luminance.ok && luminance.channels.map((c) => c.length)).toEqual([6, 6]);
    const changes = channelTargets({ ...cam, mode: "changes" } as ChannelSource, n, all(n));
    expect(changes.ok && changes.channels).toHaveLength(4);
    expect(changes.ok && changes.channels[0]).toEqual(changes.ok && changes.channels[2]);
    expect(changes.ok && changes.channels[1]).toEqual(changes.ok && changes.channels[3]);
  });

  it("puts a single neuron or a flat extent in the first cell", () => {
    const n = net([["a", 3, 3, 3]]);
    const r = channelTargets(source('layout = "grid"\ngrid = "2x2"'), n, [0]);
    expect(r.ok && r.channels).toEqual([[0], [], [], []]);
  });
});

describe("axis", () => {
  it("cuts the extent into equal slabs, one per channel", () => {
    const n = sheet();
    const r = channelTargets(source('layout = "axis"\naxis = "x"\nchannels = 4'), n, all(n));
    expect(r.ok && names(n, r.channels).map((c) => c.length)).toEqual([3, 3, 3, 3]);
    expect(r.ok && names(n, r.channels)[0]).toEqual(["s0-0", "s0-1", "s0-2"]);
    const fine = channelTargets(source('layout = "axis"\naxis = "y"\nchannels = 6'), n, all(n));
    expect(fine.ok && fine.empty).toBe(3); // 3 rows of neurons in 6 slabs
  });

  it("gives a microphone band-count slabs", () => {
    const n = sheet();
    const mic = source('layout = "axis"\naxis = "x"\nbands = 2', "microphone");
    const r = channelTargets(mic, n, all(n));
    expect(r.ok && r.channels.map((c) => c.length)).toEqual([6, 6]);
  });
});

describe("by name", () => {
  it("without a pattern: the i-th target in natural name order, whatever the listed order", () => {
    const n = net([["input-10", 0, 0, 0], ["input-2", 1, 0, 0], ["input-1", 2, 0, 0]]);
    const r = channelTargets(source('layout = "name"'), n, all(n));
    expect(r.ok && r.channels).toEqual([[2], [1], [0]]);
  });

  it("one group: the channel index; several neurons share a channel; unmatched and out-of-range are ignored", () => {
    const n = net([["in-3", 0, 0, 0], ["in-0", 0, 0, 0], ["in-1", 0, 0, 0], ["in-1b", 0, 0, 0], ["other", 0, 0, 0], ["in-9", 0, 0, 0]]);
    // "in-1b" doesn't match ^in-(\d+)$; "other" neither; in-9 is beyond 4 channels.
    const r = channelTargets(source('layout = "name"\npattern = "^in-(\\\\d+)$"\nchannels = 4'), n, all(n));
    expect(r.ok && r.channels).toEqual([[1], [2], [], [0]]);
    expect(r).toMatchObject({ empty: 1, ignored: 3 });
    const dup = net([["a1", 0, 0, 0], ["b1", 0, 0, 0], ["a2", 0, 0, 0]]);
    const shared = channelTargets(source('layout = "name"\npattern = "(\\\\d)$"\nchannels = 2\nbase = 1'), dup, all(dup));
    expect(shared.ok && shared.channels).toEqual([[0, 1], [2]]);
  });

  it("two groups: column and row of the grid, from base", () => {
    const n = net([["input-11", 0, 0, 0], ["input-21", 0, 0, 0], ["input-12", 0, 0, 0], ["input-22", 0, 0, 0], ["input-33", 0, 0, 0]]);
    const r = channelTargets(source('layout = "name"\npattern = "^input-(\\\\d)(\\\\d)$"\nbase = 1\ngrid = "2x2"'), n, all(n));
    expect(r.ok && r.channels).toEqual([[0], [1], [2], [3]]);
    expect(r).toMatchObject({ empty: 0, ignored: 1 }); // input-33 is outside 2x2
    const zero = channelTargets(source('layout = "name"\npattern = "^input-(\\\\d)(\\\\d)$"\ngrid = "2x2"'), n, all(n));
    expect(zero.ok && zero.channels[0]).toEqual([]); // input-00 doesn't exist when counting from 0
    expect(zero.ok && zero.channels[3]).toEqual([0]); // input-11 is the first neuron
  });

  it("a camera's changes mode maps on and off onto the same cells", () => {
    const n = net([["p-0-0", 0, 0, 0], ["p-1-0", 0, 0, 0]]);
    const cam = source('mode = "changes"\ngrid = "2x1"\nlayout = "name"\npattern = "p-(\\\\d)-(\\\\d)"', "camera");
    const r = channelTargets(cam, n, all(n));
    expect(r.ok && r.channels).toEqual([[0], [1], [0], [1]]);
  });
});

describe("pattern checks", () => {
  const parse = (body: string, kind = "signal") => () => source(body, kind);
  it("are reported when a file is read", () => {
    expect(parse('layout = "name"\npattern = "(("')).toThrow(/isn't a valid regular expression/);
    expect(parse('layout = "name"\npattern = "abc"')).toThrow(/needs one capture group .* it has 0/);
    expect(parse('layout = "name"\npattern = "(a)(b)(c)"')).toThrow(/it has 3/);
    expect(parse('layout = "name"\npattern = "(a)(b)"')).toThrow(/two groups \(column, row\) needs a grid/);
    expect(parse('layout = "name"\npattern = "(a)(b)"\nbands = 4', "microphone")).toThrow(/microphone's channels are in a line/);
    expect(parse('layout = "grid"', "microphone")).toThrow(/microphone's bands are in a line/);
  });

  it("find a bad pattern set in the editor", () => {
    const s = { ...source(""), layout: { kind: "name", pattern: "((", base: 0 } } as Source;
    expect(layoutProblem(s)).toMatch(/valid regular expression/);
    const n = sheet();
    expect(channelTargets(s as ChannelSource, n, all(n))).toMatchObject({ ok: false });
  });

  it("count capture groups", () => {
    expect(captureGroups("^in-(\\d+)$")).toBe(1);
    expect(captureGroups("(?:a)(b)(c)")).toBe(2);
    expect(captureGroups("abc")).toBe(0);
  });
});

describe("layout settings in the file", () => {
  it("read, write and round trip", () => {
    const text = `[[source]]
name = "s"
kind = "signal"
neurons = { prefix = "" }
formula = "c"
sample = "1 ms"
encoding = { kind = "rate", max_hz = 1, mv = 1 }
channels = 6
layout = "grid"
plane = "xz"
grid = "3x2"
`;
    const f = parseInputFile(text);
    expect(f.sources[0]).toMatchObject({ channels: 6, layout: { kind: "grid", plane: "xz", grid: { cols: 3, rows: 2 } } });
    expect(parseInputFile(serializeInputFile(f))).toEqual(f);
    expect(serializeInputFile(f)).toContain('plane = "xz"');
    const named = parseInputFile(text.replace(/layout = "grid"[\s\S]*$/, 'layout = "name"\npattern = "(\\\\d)-(\\\\d)"\nbase = 1\ngrid = "3x2"\n'));
    expect(named.sources[0]).toMatchObject({ layout: { kind: "name", pattern: "(\\d)-(\\d)", base: 1, grid: { cols: 3, rows: 2 } } });
    expect(parseInputFile(serializeInputFile(named))).toEqual(named);
    const axis = parseInputFile(text.replace(/layout = "grid"[\s\S]*$/, 'layout = "axis"\naxis = "z"\n'));
    expect(parseInputFile(serializeInputFile(axis))).toEqual(axis);
  });

  it("don't write the default order, and reject settings a layout doesn't take", () => {
    const f = parseInputFile('[[source]]\nname = "s"\nkind = "signal"\nneurons = { prefix = "" }\nformula = "1"\nencoding = { kind = "rate", max_hz = 1, mv = 1 }\nlayout = "order"\n');
    expect(serializeInputFile(f)).not.toContain("layout");
    const base = '[[source]]\nname = "s"\nkind = "signal"\nneurons = { prefix = "" }\nformula = "1"\nencoding = { kind = "rate", max_hz = 1, mv = 1 }\n';
    expect(() => parseInputFile(base + 'plane = "xy"\n')).toThrow(/plane needs layout = grid/);
    expect(() => parseInputFile(base + 'layout = "axis"\n')).toThrow(/axis must be x, y or z/);
    expect(() => parseInputFile(base + 'layout = "grid"\nplane = "ab"\n')).toThrow(/plane must be xy, xz or yz/);
    expect(() => parseInputFile(base + 'layout = "sideways"\n')).toThrow(/layout must be order, grid, axis or name/);
    expect(() => parseInputFile(base + 'layout = "axis"\naxis = "x"\ngrid = "2x2"\n')).toThrow(/grid needs layout = grid or name/);
    expect(() => parseInputFile(base + 'layout = "grid"\ngrid = "0x2"\n')).toThrow(/grid must be columns×rows/);
  });

  it("a camera keeps its own grid beside a layout", () => {
    const f = parseInputFile('[[source]]\nname = "c"\nkind = "camera"\nneurons = { prefix = "" }\nencoding = { kind = "rate", max_hz = 1, mv = 1 }\ngrid = "4x3"\nlayout = "grid"\nplane = "yz"\n');
    expect(f.sources[0]).toMatchObject({ grid: { cols: 4, rows: 3 }, layout: { kind: "grid", plane: "yz" } });
    expect(parseInputFile(serializeInputFile(f))).toEqual(f);
  });
});

describe("colouring", () => {
  it("gives each neuron its first channel, and channels distinct hues", () => {
    expect(Array.from(channelOfNeurons([[2], [0, 2], []], 4))).toEqual([1, -1, 0, -1]);
    expect(channelColor(0, 5)).not.toBe(channelColor(4, 5));
    expect(channelColor(0, 1)).toBe("hsl(0, 80%, 50%)");
  });
});
