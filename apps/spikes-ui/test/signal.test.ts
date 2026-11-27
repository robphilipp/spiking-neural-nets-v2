import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { fromBase64, parseData } from "../src/dataFile";
import { compile, noiseKey, sample, SignalError } from "../src/signal";

const f = (s: string) => compile(s);

describe("signal formulas", () => {
  it("compute arithmetic with the usual precedence", () => {
    expect(f("1 + 2 * 3")(0, 0, 1)).toBe(7);
    expect(f("(1 + 2) * 3")(0, 0, 1)).toBe(9);
    expect(f("2 ^ 3 ^ 2")(0, 0, 1)).toBe(512); // right-associative
    expect(f("-2 ^ 2")(0, 0, 1)).toBe(-4); // as in mathematics: -(2^2)
    expect(f("2 ^ -1")(0, 0, 1)).toBe(0.5);
    expect(f("t / 10 + c")(50, 2, 4)).toBe(7);
    expect(f("n")(0, 0, 49)).toBe(49);
    expect(f("-7 % 3")(0, 0, 1)).toBe(2); // never negative
  });

  it("compare and choose", () => {
    expect(f("t > 5")(6, 0, 1)).toBe(1);
    expect(f("t > 5")(5, 0, 1)).toBe(0);
    expect(f("c == 2 ? 10 : 20")(0, 2, 3)).toBe(10);
    expect(f("c == 2 ? 10 : 20")(0, 1, 3)).toBe(20);
  });

  it("have functions, including pulse and step", () => {
    expect(f("sin(pi / 2)")(0, 0, 1)).toBeCloseTo(1);
    expect(f("max(c, 3)")(0, 5, 6)).toBe(5);
    expect(f("pulse(t, 100, 10)")(105, 0, 1)).toBe(1);
    expect(f("pulse(t, 100, 10)")(115, 0, 1)).toBe(0);
    expect(f("step(t - 50)")(49, 0, 1)).toBe(0);
    expect(f("clamp(t, 0, 1)")(7, 0, 1)).toBe(1);
  });

  it("make a moving stripe", () => {
    const stripe = f("floor(t / 100) % 7 == c % 7");
    expect([0, 1, 2, 3].map((c) => stripe(150, c, 49))).toEqual([0, 1, 0, 0]);
  });

  it("give no NaN or Infinity", () => {
    expect(f("1 / 0")(0, 0, 1)).toBe(0);
    expect(f("log(-1)")(0, 0, 1)).toBe(0);
  });

  it("report mistakes with their position", () => {
    for (const bad of ["1 +", "sin(1, 2)", "foo(1)", "x", "(1", "1 2", "window.alert(1)"]) {
      expect(() => compile(bad), bad).toThrow(SignalError);
    }
    try {
      compile("1 + )");
    } catch (e) {
      expect((e as SignalError).position).toBe(4);
    }
  });

  it("sample sample-major", () => {
    expect([...sample(f("c + t"), 2, 1, 3, 2)]).toEqual([3, 4, 4, 5]);
  });
});

describe("the engine's formula language (crates/spikes-signal)", () => {
  // Samples written by the engine's test (`cargo test -p spikes-signal`): the same formulas here must give the same
  // values, noise included. sin, log and the like may differ in the last bit between JavaScript and Rust.
  const fixture = JSON.parse(readFileSync(new URL("./fixtures/signal-parity.json", import.meta.url), "utf8")) as {
    seed: number;
    source: string;
    key: string;
    channels: number;
    samples: number;
    dtMs: number;
    cases: Array<{ formula: string; values: number[] }>;
    dataFiles: Array<{ name: string; base64: string }>;
    dataCases: Array<{ formula: string; values: number[] }>;
  };

  it("derives the same noise key from the seed and the source's name", () => {
    expect(noiseKey(fixture.seed, fixture.source)).toBe(BigInt(fixture.key));
  });

  for (const { formula, values } of fixture.cases) {
    it(`agrees on ${formula}`, () => {
      const signal = compile(formula);
      const key = noiseKey(fixture.seed, fixture.source);
      for (let s = 0; s < fixture.samples; s++) {
        for (let c = 0; c < fixture.channels; c++) {
          const got = signal(s * fixture.dtMs, c, fixture.channels, { sample: s, key });
          const want = values[s * fixture.channels + c]!;
          expect(Math.abs(got - want), `sample ${s}, channel ${c}: ${got} vs ${want}`).toBeLessThanOrEqual(1e-12 * Math.max(1, Math.abs(want)));
        }
      }
    });
  }

  // The same CSV and WAV bytes the engine's test used, looked up by formulas.
  const data = new Map(fixture.dataFiles.map((d) => [d.name, parseData(d.name, fromBase64(d.base64))]));
  for (const { formula, values } of fixture.dataCases) {
    it(`agrees on the data lookup ${formula}`, () => {
      const signal = compile(formula, data);
      const key = noiseKey(fixture.seed, fixture.source);
      for (let s = 0; s < fixture.samples; s++) {
        for (let c = 0; c < fixture.channels; c++) {
          const got = signal(s * fixture.dtMs, c, fixture.channels, { sample: s, key });
          const want = values[s * fixture.channels + c]!;
          expect(Math.abs(got - want), `sample ${s}, channel ${c}: ${got} vs ${want}`).toBeLessThanOrEqual(1e-12 * Math.max(1, Math.abs(want)));
        }
      }
    });
  }
});

describe("data lookups", () => {
  const csv = (text: string) => parseData("d", new TextEncoder().encode(text));
  const sets = new Map([["d", csv("t,a,b\n0,0,10\n100,1,20\n")]]);

  it("interpolate, and are 0 outside the samples and for missing channels or files", () => {
    const at = (src: string, t: number, c = 0) => compile(src, sets)(t, c, 2);
    expect(at('data("d", t, c)', 50)).toBe(0.5);
    expect(at('data("d", t, c)', 50, 1)).toBe(15);
    expect(at('data("d", t)', 50, 1)).toBe(0.5);
    expect(at('data("d", t, c)', 100)).toBe(1);
    expect(at('data("d", t, c)', 150)).toBe(0);
    expect(at('data("d", t, c)', -1)).toBe(0);
    expect(at('data("d", t, c)', 50, 2)).toBe(0);
    expect(at('data("d", t, 1.5)', 50)).toBe(0);
    expect(at('data("nope", t)', 50)).toBe(0);
  });

  it("lists the data calls a formula makes", () => {
    const f = compile('data("a", t) + data("b", t, c) + data("c", t, 3) + data("d", t, c + 1)');
    expect(f.dataUses).toEqual([
      { name: "a", channel: { kind: "zero" } },
      { name: "b", channel: { kind: "c" } },
      { name: "c", channel: { kind: "fixed", index: 3 } },
      { name: "d", channel: { kind: "computed" } },
    ]);
  });

  it("reject malformed calls", () => {
    expect(() => compile("data(t, 1)")).toThrow(/data takes the file's name/);
    expect(() => compile('data("x, 1)')).toThrow(/closing quote/);
    expect(() => compile('data("x")')).toThrow(SignalError);
  });
});

describe("data files", () => {
  const enc = (s: string) => new TextEncoder().encode(s);
  const err = (b: Uint8Array) => {
    try {
      parseData("d", b);
    } catch (e) {
      return (e as Error).message;
    }
    return "";
  };

  it("read CSV with header, seconds, other delimiters and comments", () => {
    const a = parseData("d", enc("# note\ntime_s;x\n0;1\n0.5;3\n"));
    expect([a.channels.length, a.durationMs, lookupAt(a, 250)]).toEqual([1, 500, 2]);
    const b = parseData("d", enc("0\t1\t2\n10\t3\t4"));
    expect(b.channels.length).toBe(2);
    const c = parseData("d", enc("\uFEFF1,5\n2,7\n"));
    expect(c.startMs).toBe(1);
  });

  it("say what's wrong, naming the file", () => {
    expect(err(enc("t,a\n0,1\n0,2\n"))).toBe("data file 'd': line 3: times must be ascending");
    expect(err(enc("t,a\n0,1\n1,x\n"))).toContain("line 3, column 2: 'x' is not a number");
    expect(err(enc("t,a\n0,1\n1,2,3\n"))).toContain("line 3: has 3 columns, the first row has 2");
    expect(err(enc("0\n1\n"))).toContain("needs a time column");
    expect(err(enc(""))).toContain("no rows");
    expect(err(new Uint8Array([0xff, 0xfe, 0]))).toContain("not text");
    expect(err(enc("RIFF\0\0\0\0WAVE"))).toContain("no fmt chunk");
  });

  it("read a 16-bit stereo WAV", () => {
    const frames = [[0, 32767], [16384, 0], [-32768, 0]];
    const buf = new ArrayBuffer(44 + frames.length * 4);
    const v = new DataView(buf);
    const str = (o: number, t: string) => [...t].forEach((ch, k) => v.setUint8(o + k, ch.charCodeAt(0)));
    str(0, "RIFF");
    v.setUint32(4, 36 + frames.length * 4, true);
    str(8, "WAVEfmt ");
    v.setUint32(16, 16, true);
    v.setUint16(20, 1, true);
    v.setUint16(22, 2, true);
    v.setUint32(24, 1000, true);
    v.setUint32(28, 4000, true);
    v.setUint16(32, 4, true);
    v.setUint16(34, 16, true);
    str(36, "data");
    v.setUint32(40, frames.length * 4, true);
    frames.forEach((f, i) => f.forEach((x, c) => v.setInt16(44 + i * 4 + c * 2, x, true)));
    const d = parseData("w", new Uint8Array(buf));
    expect([d.channels.length, d.length, d.rateHz, d.durationMs]).toEqual([2, 3, 1000, 2]);
    expect(lookupAt(d, 0.5)).toBe(0.25);
    expect(lookupAt(d, 2)).toBe(-1);
    expect(lookupAt(d, 2.1)).toBe(0);
  });
});

function lookupAt(d: ReturnType<typeof parseData>, t: number) {
  return compile('data("d", t)', new Map([["d", d]]))(t, 0, 1);
}
