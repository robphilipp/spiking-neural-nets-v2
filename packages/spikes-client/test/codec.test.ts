import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { decodeFrame, encodeEvents, encodeSamples, Lod, spikeCount, timeMs } from "../src";

const golden = (name: string) => {
  const bytes = readFileSync(join(__dirname, "golden", name));
  // A fresh, aligned ArrayBuffer, as a WebSocket delivers.
  return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer;
};

describe("frames", () => {
  it("decode the Rust encoder's reference frame", () => {
    const f = decodeFrame(golden("frame.bin"));
    expect(f.header).toMatchObject({
      version: 1, session: 7, seq: 42, viewer: 3, t0Ms: 1500, t1Ms: 1516, wallUnixMs: 1_795_000_000_000,
      speed: 0.25, lagMs: 1.5, lod: Lod.Exact, droppedEvents: 2, droppedFrames: 1, timeUnitNs: 1,
    });
    expect([...f.spikes!.neuron]).toEqual([1, 2, 3]);
    expect([...f.spikes!.offset]).toEqual([10, 20, 30]);
    expect([...f.spikeCounts!.count]).toEqual([1, 2, 3]);
    expect([...f.spikeBins!.cell]).toEqual([7]);
    expect([...f.membrane!.mv]).toEqual([-1.5, 2.25]);
    expect([...f.weights!.weight]).toEqual([0.5, 2.0]);
    expect([...f.transitions!.tonic]).toEqual([1, 0, 1]);
    expect(timeMs(f.header, 1_000_000)).toBe(1501);
    expect(spikeCount(f)).toBe(3 + 6 + 70);
  });

  it("decode from an unaligned view by copying", () => {
    const bytes = new Uint8Array(golden("frame.bin"));
    const padded = new Uint8Array(bytes.length + 3);
    padded.set(bytes, 3);
    expect(decodeFrame(padded.subarray(3)).header.seq).toBe(42);
  });

  it("reject garbage", () => {
    expect(() => decodeFrame(new Uint8Array(10))).toThrow();
    expect(() => decodeFrame(golden("frame.bin").slice(0, 90))).toThrow();
  });
});

describe("sensor batches", () => {
  it("encode exactly as the Rust encoder does", () => {
    const samples = encodeSamples(3, 7, 1n << 40n, 1_000_000, 2, [0.5, -1.0, 2.0, 3.5]);
    expect(new Uint8Array(samples)).toEqual(new Uint8Array(golden("samples.bin")));
    const events = encodeEvents(1, 0, 5n, [0, 3, 1], [10, 20, 30]);
    expect(new Uint8Array(events)).toEqual(new Uint8Array(golden("events.bin")));
  });

  it("refuse partial samples", () => {
    expect(() => encodeSamples(0, 0, 0n, 1, 3, [1, 2])).toThrow(RangeError);
  });
});

describe("generated types", () => {
  it("are all re-exported", () => {
    const dir = join(__dirname, "..", "src", "generated");
    const index = readFileSync(join(dir, "index.ts"), "utf8");
    for (const file of readdirSync(dir).filter((f) => f.endsWith(".ts") && f !== "index.ts")) {
      expect(index, `index.ts misses ${file}`).toContain(`"./${file.replace(/\.ts$/, "")}"`);
    }
  });
});
