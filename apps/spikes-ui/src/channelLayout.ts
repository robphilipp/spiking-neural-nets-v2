// Position mapping of channels (app plan 05, phase I5): which target neurons each channel of a source drives.
//
// A source with channels (microphone bands, a camera's cells, a signal's or file's channels) has a `layout`:
//
//  - order (default): channel i drives the i-th target neuron in network order.
//  - name: by the neurons' names. Without a `pattern`, channel i drives the i-th target in NATURAL name order (digit
//    runs compare as numbers: input-2 before input-10; otherwise case-sensitive text), whatever order the description
//    lists them in. With a `pattern` (a JavaScript regular expression), each target whose name matches is assigned by
//    its capture groups: one group is the channel index; two are the column and row of the channel grid (row-major:
//    row × columns + column). Numbers count from `base` (default 0; 1 for names like input-11 = column 1, row 1).
//    Several neurons may share a channel; a neuron that doesn't match, or lands outside the channels, is ignored.
//  - grid: the targets' bounding box in a `plane` (xy, xz or yz) is split into columns × rows cells (a camera's own
//    grid; a signal's or file's `grid`, default 1xN: one column); channel (col, row), row-major, drives every target
//    in its cell. y (or z) increases with the row.
//  - axis: the targets' extent along `axis` is split into N equal slabs, slab k driving channel k.
//
// A camera's `changes` mode has on and off channels: both sets map onto the same cells' neurons. A channel can drive
// no neuron (an empty cell or slab, a name nothing has): reported, not an error.

import type { Network } from "@spikes/client";
import type { Grid, Source } from "./inputFile";
import { cameraChannels, cellsOf } from "./realtimeChannels";

export type Plane = "xy" | "xz" | "yz";
export type Axis = "x" | "y" | "z";

export type Layout =
  | { kind: "grid"; plane: Plane; grid?: Grid }
  | { kind: "axis"; axis: Axis }
  | { kind: "name"; pattern?: string; base: number; grid?: Grid };

export const PLANES: Plane[] = ["xy", "xz", "yz"];
export const AXES: Axis[] = ["x", "y", "z"];
export type LayoutKind = "order" | Layout["kind"];

/** A source that has channels. */
export type ChannelSource = Extract<Source, { kind: "microphone" | "camera" | "signal" | "file" }>;
export const hasChannels = (s: Source): s is ChannelSource => s.kind === "microphone" || s.kind === "camera" || s.kind === "signal" || s.kind === "file";

export const layoutKind = (s: Source): LayoutKind => ("layout" in s && s.layout ? s.layout.kind : "order");

/** Compares names in natural order: digit runs as numbers, the rest as case-sensitive text. */
export function naturalCompare(a: string, b: string): number {
  const parts = (s: string) => s.match(/\d+|\D+/g) ?? [];
  const [pa, pb] = [parts(a), parts(b)];
  for (let i = 0; i < Math.min(pa.length, pb.length); i++) {
    const [x, y] = [pa[i]!, pb[i]!];
    const numeric = /^\d/.test(x) && /^\d/.test(y);
    if (numeric) {
      const d = Number(x) - Number(y);
      if (d !== 0) return d;
    } else if (x !== y) return x < y ? -1 : 1;
  }
  return pa.length - pb.length || (a < b ? -1 : a > b ? 1 : 0);
}

/** How many capture groups a regular expression has; throws a SyntaxError for an invalid one. */
export function captureGroups(pattern: string): number {
  return new RegExp(`${pattern}|`).exec("")!.length - 1;
}

export type ChannelResult =
  | {
      ok: true;
      /** Each channel's neurons (network indices). */
      channels: number[][];
      /** Channels that drive no neuron. */
      empty: number;
      /** Target neurons no channel drives because of their names (the name layout only). */
      ignored: number;
    }
  | { ok: false; error: string };

const fail = (error: string): ChannelResult => ({ ok: false, error });

/** A source's grid (a camera's own; a signal's or file's setting) — undefined if it has none. */
function gridOf(s: ChannelSource): Grid | undefined {
  if (s.kind === "camera") return s.grid;
  if (s.kind === "microphone") return undefined;
  return s.layout && "grid" in s.layout ? s.layout.grid : undefined;
}

/** What a source says about its layout that can be wrong, without a network: an invalid pattern, a layout a kind
 *  doesn't have, a pattern's groups the source can't use. null: fine. */
export function layoutProblem(s: Source): string | null {
  if (!hasChannels(s) || !("layout" in s) || !s.layout) return null;
  const l = s.layout;
  if (s.kind === "microphone" && l.kind === "grid") return "a microphone's bands are in a line: use layout name or axis, not grid";
  if (l.kind === "name" && l.pattern !== undefined) {
    let groups: number;
    try {
      groups = captureGroups(l.pattern);
    } catch (e) {
      return `pattern isn't a valid regular expression: ${e instanceof Error ? e.message : String(e)}`;
    }
    if (groups < 1 || groups > 2) return `pattern needs one capture group (the channel) or two (column, row), it has ${groups}`;
    if (groups === 2 && s.kind === "microphone") return "a microphone's channels are in a line: its pattern takes one capture group";
    if (groups === 2 && s.kind !== "camera" && !gridOf(s)) return "a pattern with two groups (column, row) needs a grid setting";
  }
  return null;
}

/** The channels a source has, given its target neurons and (a file) the data's channels. null: can't say yet. */
export function channelCount(s: ChannelSource, targets: number, dataChannels?: number): number | null {
  const grid = gridOf(s);
  switch (s.kind) {
    case "camera":
      return cameraChannels(s.grid, s.mode);
    case "microphone":
      return s.layout ? (s.bands ?? targets) : Math.min(targets, s.bands ?? targets);
    case "signal":
      return s.channels ?? (grid ? cellsOf(grid) : targets);
    case "file":
      if (dataChannels === undefined) return null;
      return Math.min(s.channels ?? Infinity, dataChannels);
  }
}

/** The camera's message when it has too few targets under `order` (kept as it always was). */
function needs(s: ChannelSource, n: number, targets: number): string {
  if (s.kind === "camera") {
    const what = `camera ${s.grid.cols}x${s.grid.rows}${s.mode === "changes" ? " changes (on and off)" : ""}`;
    return `${what} needs ${n} target neurons, has ${targets}`;
  }
  return `${s.kind} needs ${n} target neurons, has ${targets}`;
}

/**
 * The neurons each channel of `source` drives, among `targets` (the source's target neurons, network indices, in
 * network order) of `network`. `dataChannels`: how many channels the source's data file has (file sources).
 */
export function channelTargets(source: ChannelSource, network: Network, targets: number[], dataChannels?: number): ChannelResult {
  const problem = layoutProblem(source);
  if (problem) return fail(problem);
  if (targets.length === 0) return fail(`${source.kind} has no target neurons`);
  let n = channelCount(source, targets.length, dataChannels);
  if (n === null) return fail("the data file isn't loaded");
  const layout = "layout" in source ? source.layout : undefined;
  // The cells of a grid, when the layout uses one (a camera's changes: on and off cells share them).
  const copies = source.kind === "camera" && source.mode === "changes" ? 2 : 1;
  const finish = (cells: number[][], ignored = 0): ChannelResult => {
    const channels = copies === 2 ? [...cells, ...cells] : cells;
    return { ok: true, channels, empty: channels.filter((c) => !c.length).length, ignored };
  };
  const grid = gridOf(source);
  if (!layout) {
    if (targets.length < n) return fail(needs(source, n, targets.length));
    return { ok: true, channels: targets.slice(0, n).map((i) => [i]), empty: 0, ignored: 0 };
  }
  if (layout.kind === "grid") {
    if (grid && source.kind !== "camera" && cellsOf(grid) !== n) {
      return fail(`grid ${grid.cols}x${grid.rows} has ${cellsOf(grid)} cells, the ${source.kind} has ${n} channels`);
    }
    return finish(gridCells(network, targets, layout.plane, grid ?? { cols: 1, rows: n }));
  }
  if (layout.kind === "axis") {
    const slabs = source.kind === "camera" ? cellsOf(source.grid) : n;
    return finish(slabCells(network, targets, layout.axis, slabs));
  }
  // By name.
  if (layout.pattern === undefined) {
    const sorted = [...targets].sort((a, b) => naturalCompare(network.neurons[a]!.name, network.neurons[b]!.name));
    const cells = source.kind === "camera" ? cellsOf(source.grid) : n;
    if (sorted.length < cells) return fail(needs(source, cells, sorted.length));
    return finish(sorted.slice(0, cells).map((i) => [i]));
  }
  const re = new RegExp(layout.pattern);
  const groups = captureGroups(layout.pattern);
  const g = groups === 2 ? gridOf(source)! : null;
  if (source.kind === "camera") n = cellsOf(source.grid);
  const cells: number[][] = Array.from({ length: g ? cellsOf(g) : n }, () => []);
  let ignored = 0;
  for (const i of targets) {
    const m = re.exec(network.neurons[i]!.name);
    const nums = m ? [m[1], m[2]].slice(0, groups).map((x) => (x !== undefined && /^\d+$/.test(x) ? Number(x) - layout.base : NaN)) : [];
    let channel = -1;
    if (nums.length === groups && nums.every(Number.isInteger)) {
      if (g) {
        const [col, row] = nums as [number, number];
        if (col >= 0 && col < g.cols && row >= 0 && row < g.rows) channel = row * g.cols + col;
      } else if (nums[0]! >= 0 && nums[0]! < cells.length) channel = nums[0]!;
    }
    if (channel < 0) ignored++;
    else cells[channel]!.push(i);
  }
  return finish(cells, ignored);
}

function extent(xs: number[]): [number, number] {
  let lo = Infinity;
  let hi = -Infinity;
  for (const x of xs) {
    if (x < lo) lo = x;
    if (x > hi) hi = x;
  }
  return [lo, hi];
}

/** Targets split into `grid`'s cells in `plane`, row-major. */
export function gridCells(network: Network, targets: number[], plane: Plane, grid: Grid): number[][] {
  const [a, b] = plane === "xy" ? [0, 1] : plane === "xz" ? [0, 2] : [1, 2];
  const cells: number[][] = Array.from({ length: cellsOf(grid) }, () => []);
  const us = targets.map((i) => network.neurons[i]!.position[a]!);
  const vs = targets.map((i) => network.neurons[i]!.position[b]!);
  const bin = (x: number, lo: number, hi: number, k: number) => (hi > lo ? Math.min(k - 1, Math.floor(((x - lo) / (hi - lo)) * k)) : 0);
  const [u0, u1] = extent(us);
  const [v0, v1] = extent(vs);
  targets.forEach((neuron, k) => {
    cells[bin(vs[k]!, v0, v1, grid.rows) * grid.cols + bin(us[k]!, u0, u1, grid.cols)]!.push(neuron);
  });
  return cells;
}

/** Targets split into `count` equal slabs along `axis`. */
export function slabCells(network: Network, targets: number[], axis: Axis, count: number): number[][] {
  const a = AXES.indexOf(axis);
  const cells: number[][] = Array.from({ length: count }, () => []);
  const xs = targets.map((i) => network.neurons[i]!.position[a]!);
  const [lo, hi] = extent(xs);
  targets.forEach((neuron, k) => {
    cells[hi > lo ? Math.min(count - 1, Math.floor(((xs[k]! - lo) / (hi - lo)) * count)) : 0]!.push(neuron);
  });
  return cells;
}

/** The first channel each neuron drives (−1: none), for colouring. */
export function channelOfNeurons(channels: number[][], neurons: number): Int32Array {
  const out = new Int32Array(neurons).fill(-1);
  channels.forEach((list, c) => list.forEach((i) => out[i] === -1 && (out[i] = c)));
  return out;
}

/** A colour for channel `c` of `n`: hues from red through violet (not wrapping back to red). */
export function channelColor(c: number, n: number): string {
  return `hsl(${Math.round((300 * c) / Math.max(1, n - 1))}, 80%, 50%)`;
}
