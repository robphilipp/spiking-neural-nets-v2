import { describe, expect, it } from "vitest";
import { parseInputFile, type RealtimeSource } from "../src/inputFile";
import type { Network } from "@spikes/client";
import { channelTargets, type ChannelSource } from "../src/channelLayout";
import { bandLevels, cameraChannels, frameChanges, lumaGrid, previewChannels, speedText, speedVerdict } from "../src/realtimeChannels";

/** What a microphone or camera sends with `n` target neurons in a line (order layout): its channels, or the error. */
const channelPlan = (s: RealtimeSource, n: number): { channels: number } | { error: string } => {
  const network = { neurons: Array.from({ length: n }, (_, i) => ({ index: i, name: `n${i}`, position: [i, 0, 0] })) } as unknown as Network;
  const r = channelTargets(s as ChannelSource, network, [...Array(n).keys()]);
  return r.ok ? { channels: r.channels.length } : { error: r.error };
};

const source = (body: string): RealtimeSource =>
  parseInputFile(`[[source]]\nname = "s"\nneurons = { prefix = "" }\nencoding = { kind = "rate", max_hz = 1, mv = 1 }\n${body}`).sources[0] as RealtimeSource;

describe("microphone bands", () => {
  // 1024 bins up to 24 kHz: 23.4 Hz each.
  const sampleRate = 48_000;
  const spectrum = (loud: (hz: number) => number) => Float32Array.from({ length: 1024 }, (_, b) => loud(((b + 0.5) * sampleRate) / 2 / 1024));

  it("are one per channel, log-spaced from 100 Hz to 8 kHz", () => {
    // A tone at 1 kHz: −30 dB in the bins around it, silence (−∞ dB, as an analyser reports) elsewhere.
    const levels = bandLevels(spectrum((hz) => (Math.abs(hz - 1000) < 60 ? -30 : -Infinity)), 8, sampleRate);
    expect(levels).toHaveLength(8);
    // 8 bands over 100…8000 Hz: edges at 100·80^(c/8); 1 kHz falls in band 4 (≈ 760…1320 Hz) alone.
    const loudest = levels.indexOf(Math.max(...levels));
    expect(loudest).toBe(4);
    expect(levels.filter((v) => v > 0)).toHaveLength(1);
  });

  it("scale −90 dB to 0 and −30 dB to 1, and clamp", () => {
    expect([...bandLevels(spectrum(() => -90), 4, sampleRate)]).toEqual([0, 0, 0, 0]);
    expect([...bandLevels(spectrum(() => 0), 4, sampleRate)]).toEqual([1, 1, 1, 1]);
    const mid = bandLevels(spectrum(() => -60), 4, sampleRate);
    for (const v of mid) expect(v).toBeCloseTo(0.5, 5);
  });

  it("give every band at least one bin, even many narrow ones", () => {
    const levels = bandLevels(spectrum(() => -60), 200, sampleRate);
    expect(levels).toHaveLength(200);
    for (const v of levels) expect(v).toBeCloseTo(0.5, 5);
  });
});

describe("camera grids", () => {
  const solid = (w: number, h: number, rgb: [number, number, number]) => Uint8ClampedArray.from({ length: w * h * 4 }, (_, i) => (i % 4 === 3 ? 255 : rgb[i % 4]!));

  it("average pixels' brightness over cells, row by row", () => {
    // 4×2 pixels, a 2×1 grid: the left half white, the right half black.
    const px = new Uint8ClampedArray(4 * 2 * 4);
    for (let y = 0; y < 2; y++) for (let x = 0; x < 4; x++) px.set(x < 2 ? [255, 255, 255, 255] : [0, 0, 0, 255], 4 * (y * 4 + x));
    const g = lumaGrid(px, 4, 2, { cols: 2, rows: 1 });
    expect(g[0]).toBeCloseTo(1, 5);
    expect(g[1]).toBe(0);
    const rows = lumaGrid(solid(2, 4, [255, 255, 255]), 2, 4, { cols: 1, rows: 2 });
    expect([...rows]).toEqual([1, 1].map((v) => expect.closeTo(v, 5)));
  });

  it("weigh colours as brightness does (green most)", () => {
    const red = lumaGrid(solid(1, 1, [255, 0, 0]), 1, 1, { cols: 1, rows: 1 })[0]!;
    const green = lumaGrid(solid(1, 1, [0, 255, 0]), 1, 1, { cols: 1, rows: 1 })[0]!;
    const blue = lumaGrid(solid(1, 1, [0, 0, 255]), 1, 1, { cols: 1, rows: 1 })[0]!;
    expect(red).toBeCloseTo(0.2126, 4);
    expect(green).toBeCloseTo(0.7152, 4);
    expect(blue).toBeCloseTo(0.0722, 4);
  });

  it("make a cell of every grid position even when the image is smaller than the grid", () => {
    const g = lumaGrid(solid(2, 2, [255, 255, 255]), 2, 2, { cols: 4, rows: 4 });
    expect(g).toHaveLength(16);
    for (const v of g) expect(v).toBeCloseTo(1, 5);
  });

  it("make frame differences with on channels, then off channels, above the level", () => {
    const before = [0.5, 0.5, 0.5, 0.5];
    const after = [0.9, 0.1, 0.55, 0.5]; // rose by 0.4, fell by 0.4, rose by 0.05 (under the level), unchanged
    const d = frameChanges(before, after, 0.1);
    expect(d).toHaveLength(8);
    expect([...d].map((v) => Math.round(v * 100) / 100)).toEqual([0.4, 0, 0, 0, /* off: */ 0, 0.4, 0, 0]);
    expect([...frameChanges(before, before, 0.1)]).toEqual(new Array(8).fill(0));
  });

  it("count their channels", () => {
    expect(cameraChannels({ cols: 16, rows: 16 }, "luminance")).toBe(256);
    expect(cameraChannels({ cols: 16, rows: 16 }, "changes")).toBe(512);
  });
});

describe("channels and targets", () => {
  it("a microphone has a band per target neuron, up to its bands", () => {
    expect(channelPlan(source('kind = "microphone"'), 20)).toEqual({ channels: 20 });
    expect(channelPlan(source('kind = "microphone"\nbands = 8'), 20)).toEqual({ channels: 8 });
    expect(channelPlan(source('kind = "microphone"\nbands = 32'), 20)).toEqual({ channels: 20 });
  });

  it("a camera needs a target neuron per channel", () => {
    const cam = source('kind = "camera"');
    expect(channelPlan(cam, 100)).toEqual({ error: "camera 16x16 needs 256 target neurons, has 100" });
    expect(channelPlan(cam, 256)).toEqual({ channels: 256 });
    expect(channelPlan(cam, 300)).toEqual({ channels: 256 }); // the extra neurons get nothing
    const events = source('kind = "camera"\nmode = "changes"\ngrid = "4x4"');
    expect(channelPlan(events, 31)).toEqual({ error: "camera 4x4 changes (on and off) needs 32 target neurons, has 31" });
    expect(channelPlan(events, 32)).toEqual({ channels: 32 });
  });

  it("no source reaches nothing", () => {
    expect(channelPlan(source('kind = "camera"'), 0)).toEqual({ error: "camera has no target neurons" });
  });

  it("previews without a network use the settings", () => {
    expect(previewChannels(source('kind = "microphone"'))).toBe(16);
    expect(previewChannels(source('kind = "microphone"\nbands = 5'))).toBe(5);
    expect(previewChannels(source('kind = "camera"\nmode = "changes"\ngrid = "4x2"'))).toBe(16);
  });
});

describe("speed rules", () => {
  const resample = source('kind = "microphone"');
  const refuse = source('kind = "microphone"\nspeed = "refuse"');

  it("an as-fast-as-possible run takes no realtime input", () => {
    for (const s of [resample, refuse]) {
      const v = speedVerdict(s, null);
      expect(v.ok).toBe(false);
      expect(v.ok === false && v.message).toMatch(/as fast as possible/);
    }
  });

  it("resample follows any speed; refuse runs only at ×1", () => {
    for (const speed of [0.25, 1, 4]) expect(speedVerdict(resample, speed).ok).toBe(true);
    expect(speedVerdict(refuse, 1).ok).toBe(true);
    const slow = speedVerdict(refuse, 0.5);
    expect(slow.ok === false && slow.message).toBe("the run is at speed ×0.5 and this source runs only at ×1");
  });

  it("write speeds", () => {
    expect(speedText(0.5)).toBe("×0.5");
    expect(speedText(2)).toBe("×2");
    expect(speedText(1 / 3)).toBe("×0.333");
    expect(speedText(null)).toBe("max");
  });
});
