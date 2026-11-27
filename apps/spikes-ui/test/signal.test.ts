import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
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
});
