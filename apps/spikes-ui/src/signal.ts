// Signal formulas for the input designer: a small expression language over time `t` (ms), channel `c` (0…n−1) and
// the channel count `n`. Evaluated by our own parser, not as JavaScript: the desktop app's security policy forbids
// evaluating code, and a formula can't do anything but compute a number.
//
//   numbers, t, c, n, pi            + − * / % ^ (power), unary −, parentheses
//   < <= > >= == != (1 or 0)        a ? b : c
//   sin cos tan exp log sqrt abs floor ceil round   min(a, b) max(a, b) pow(a, b)
//   step(x)        1 for x ≥ 0, else 0
//   pulse(t, period, width)   1 for the first `width` of every `period`, else 0
//   clamp(x, lo, hi)
//   rand()         uniform in [0, 1): a fresh value per sample and channel (seeded: see `noiseKey`)
//   gauss()        standard normal, likewise
//
// The engine has the same language (crates/spikes-signal): a test compares the two on the same formulas, noise
// included (test/signal.test.ts, against samples the engine's test writes).
//
// Example: a stripe moving across a 7-wide line sensor every 100 ms:  floor(t / 100) % 7 == c % 7

/** Where a formula is evaluated: the sample's index (from the source's first), and the noise key. */
export interface At {
  sample: number;
  key: bigint;
}

const NO_NOISE: At = { sample: 0, key: 0n };

export type Signal = (t: number, c: number, n: number, at?: At) => number;

export class SignalError extends Error {
  constructor(
    message: string,
    readonly position: number,
  ) {
    super(message);
  }
}

type Node = (t: number, c: number, n: number, at: At) => number;

// ----- seeded noise: the engine's counter-based scheme (crates/spikes-signal/src/noise.rs), bit for bit -----

const MASK = (1n << 64n) - 1n;
const GOLDEN = 0x9e3779b97f4a7c15n;

/** splitmix64's finalizer. */
function mix(z: bigint): bigint {
  z = ((z ^ (z >> 30n)) * 0xbf58476d1ce4e5b9n) & MASK;
  z = ((z ^ (z >> 27n)) * 0x94d049bb133111ebn) & MASK;
  return z ^ (z >> 31n);
}

/** FNV-1a over a name's UTF-8 bytes. */
function fnv1a(name: string): bigint {
  let h = 0xcbf29ce484222325n;
  for (const b of new TextEncoder().encode(name)) h = ((h ^ BigInt(b)) * 0x100000001b3n) & MASK;
  return h;
}

/** A source's noise key: the run's seed and the source's name (as the engine computes it). */
export function noiseKey(seed: number | bigint, source: string): bigint {
  return mix(((BigInt(seed) + GOLDEN) & MASK) ^ fnv1a(source));
}

export function uniform(key: bigint, channel: number, sample: number, site: number, part: number): number {
  let z = mix(key ^ ((BigInt(channel) * GOLDEN) & MASK));
  z = mix(z ^ ((BigInt(sample) * 0xd6e8feb86659fd93n) & MASK));
  z = mix(z ^ ((((BigInt(site) << 32n) | BigInt(part)) * 0xa0761d6478bd642fn) & MASK));
  return Number(z >> 11n) * 2 ** -53;
}

export function gauss(key: bigint, channel: number, sample: number, site: number): number {
  const u1 = 1 - uniform(key, channel, sample, site, 0);
  const u2 = uniform(key, channel, sample, site, 1);
  return Math.sqrt(-2 * Math.log(u1)) * Math.cos(2 * Math.PI * u2);
}

const FUNCTIONS: Record<string, { arity: number; f: (...a: number[]) => number }> = {
  sin: { arity: 1, f: Math.sin },
  cos: { arity: 1, f: Math.cos },
  tan: { arity: 1, f: Math.tan },
  exp: { arity: 1, f: Math.exp },
  log: { arity: 1, f: Math.log },
  sqrt: { arity: 1, f: Math.sqrt },
  abs: { arity: 1, f: Math.abs },
  floor: { arity: 1, f: Math.floor },
  ceil: { arity: 1, f: Math.ceil },
  round: { arity: 1, f: Math.round },
  step: { arity: 1, f: (x) => (x >= 0 ? 1 : 0) },
  min: { arity: 2, f: Math.min },
  max: { arity: 2, f: Math.max },
  pow: { arity: 2, f: Math.pow },
  pulse: { arity: 3, f: (t, period, width) => (period > 0 && ((t % period) + period) % period < width ? 1 : 0) },
  clamp: { arity: 3, f: (x, lo, hi) => Math.min(Math.max(x, lo), hi) },
};

/** Compiles a formula. Throws a SignalError (with the position) for a malformed one. */
export function compile(source: string): Signal {
  let i = 0;
  /** Call sites of rand() and gauss(), numbered in order (so two calls are independent). */
  let sites = 0;
  const skip = () => {
    while (i < source.length && /\s/.test(source[i]!)) i++;
  };
  const fail = (message: string): never => {
    throw new SignalError(message, i);
  };
  const eat = (token: string) => {
    skip();
    if (source.startsWith(token, i)) {
      i += token.length;
      return true;
    }
    return false;
  };
  const expect = (token: string) => {
    if (!eat(token)) fail(`expected '${token}'`);
  };

  // conditional := comparison ('?' conditional ':' conditional)?
  const conditional = (): Node => {
    const test = comparison();
    if (!eat("?")) return test;
    const yes = conditional();
    expect(":");
    const no = conditional();
    return (t, c, n, at) => (test(t, c, n, at) !== 0 ? yes(t, c, n, at) : no(t, c, n, at));
  };
  const comparison = (): Node => {
    let left = additive();
    for (;;) {
      const op = ["<=", ">=", "==", "!=", "<", ">"].find((o) => eat(o));
      if (!op) return left;
      const l = left;
      const r = additive();
      const cmp: Record<string, (a: number, b: number) => boolean> = {
        "<=": (a, b) => a <= b,
        ">=": (a, b) => a >= b,
        "==": (a, b) => a === b,
        "!=": (a, b) => a !== b,
        "<": (a, b) => a < b,
        ">": (a, b) => a > b,
      };
      left = (t, c, n, at) => (cmp[op]!(l(t, c, n, at), r(t, c, n, at)) ? 1 : 0);
    }
  };
  const additive = (): Node => {
    let left = multiplicative();
    for (;;) {
      if (eat("+")) {
        const l = left;
        const r = multiplicative();
        left = (t, c, n, at) => l(t, c, n, at) + r(t, c, n, at);
      } else if (eat("-")) {
        const l = left;
        const r = multiplicative();
        left = (t, c, n, at) => l(t, c, n, at) - r(t, c, n, at);
      } else return left;
    }
  };
  const multiplicative = (): Node => {
    let left = unary();
    for (;;) {
      const op = ["*", "/", "%"].find((o) => eat(o));
      if (!op) return left;
      const l = left;
      const r = unary();
      left =
        op === "*"
          ? (t, c, n, at) => l(t, c, n, at) * r(t, c, n, at)
          : op === "/"
            ? (t, c, n, at) => l(t, c, n, at) / r(t, c, n, at)
            : (t, c, n, at) => {
                const b = r(t, c, n, at);
                return ((l(t, c, n, at) % b) + b) % b; // a modulus that is never negative for b > 0
              };
    }
  };
  // unary := '-' unary | power;  power := primary ('^' unary)?  — so -2^2 is -(2^2), and 2^-1 works.
  const unary = (): Node => {
    if (eat("-")) {
      const x = unary();
      return (t, c, n, at) => -x(t, c, n, at);
    }
    eat("+");
    return power();
  };
  const power = (): Node => {
    const base = primary();
    if (!eat("^")) return base;
    const exponent = unary(); // right-associative
    return (t, c, n, at) => base(t, c, n, at) ** exponent(t, c, n, at);
  };
  const primary = (): Node => {
    skip();
    if (eat("(")) {
      const x = conditional();
      expect(")");
      return x;
    }
    const number = /^(\d+(\.\d*)?|\.\d+)([eE][-+]?\d+)?/.exec(source.slice(i));
    if (number) {
      i += number[0].length;
      const v = Number(number[0]);
      return () => v;
    }
    const name = /^[A-Za-z_]\w*/.exec(source.slice(i));
    if (!name) return fail(i >= source.length ? "unexpected end of the formula" : `unexpected '${source[i]}'`);
    i += name[0].length;
    const word = name[0];
    if (word === "t") return (t) => t;
    if (word === "c") return (_, c) => c;
    if (word === "n") return (_, __, n) => n;
    if (word === "pi") return () => Math.PI;
    if (word === "rand" || word === "gauss") {
      expect("(");
      expect(")");
      const site = sites++;
      return word === "rand"
        ? (_, c, __, at) => uniform(at.key, c, at.sample, site, 0)
        : (_, c, __, at) => gauss(at.key, c, at.sample, site);
    }
    const fn = FUNCTIONS[word];
    if (!fn) {
      i -= word.length;
      return fail(`unknown name '${word}' (use t, c, n, pi, or a function like sin, pulse, step, rand)`);
    }
    expect("(");
    const args: Node[] = [conditional()];
    while (eat(",")) args.push(conditional());
    expect(")");
    if (args.length !== fn.arity) fail(`${word} takes ${fn.arity} argument${fn.arity > 1 ? "s" : ""}`);
    return (t, c, n, at) => fn.f(...args.map((a) => a(t, c, n, at)));
  };

  const node = conditional();
  skip();
  if (i < source.length) fail(`unexpected '${source[i]}'`);
  return (t, c, n, at = NO_NOISE) => {
    const v = node(t, c, n, at);
    return Number.isFinite(v) ? v : 0;
  };
}

/** Samples a signal: `values[s * channels + c]` for samples `first … first + count − 1`, `dtMs` apart. `key`: the
 *  noise key ({@link noiseKey}). */
export function sample(signal: Signal, channels: number, dtMs: number, first: number, count: number, key = 0n): Float32Array {
  const values = new Float32Array(count * channels);
  for (let s = 0; s < count; s++) {
    const t = (first + s) * dtMs;
    const at = { sample: first + s, key };
    for (let c = 0; c < channels; c++) values[s * channels + c] = signal(t, c, channels, at);
  }
  return values;
}
