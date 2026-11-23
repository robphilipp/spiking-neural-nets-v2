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
//
// Example: a stripe moving across a 7-wide line sensor every 100 ms:  floor(t / 100) % 7 == c % 7

export type Signal = (t: number, c: number, n: number) => number;

export class SignalError extends Error {
  constructor(
    message: string,
    readonly position: number,
  ) {
    super(message);
  }
}

type Node = (t: number, c: number, n: number) => number;

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
    return (t, c, n) => (test(t, c, n) !== 0 ? yes(t, c, n) : no(t, c, n));
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
      left = (t, c, n) => (cmp[op]!(l(t, c, n), r(t, c, n)) ? 1 : 0);
    }
  };
  const additive = (): Node => {
    let left = multiplicative();
    for (;;) {
      if (eat("+")) {
        const l = left;
        const r = multiplicative();
        left = (t, c, n) => l(t, c, n) + r(t, c, n);
      } else if (eat("-")) {
        const l = left;
        const r = multiplicative();
        left = (t, c, n) => l(t, c, n) - r(t, c, n);
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
          ? (t, c, n) => l(t, c, n) * r(t, c, n)
          : op === "/"
            ? (t, c, n) => l(t, c, n) / r(t, c, n)
            : (t, c, n) => {
                const b = r(t, c, n);
                return ((l(t, c, n) % b) + b) % b; // a modulus that is never negative for b > 0
              };
    }
  };
  // unary := '-' unary | power;  power := primary ('^' unary)?  — so -2^2 is -(2^2), and 2^-1 works.
  const unary = (): Node => {
    if (eat("-")) {
      const x = unary();
      return (t, c, n) => -x(t, c, n);
    }
    eat("+");
    return power();
  };
  const power = (): Node => {
    const base = primary();
    if (!eat("^")) return base;
    const exponent = unary(); // right-associative
    return (t, c, n) => base(t, c, n) ** exponent(t, c, n);
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
    const fn = FUNCTIONS[word];
    if (!fn) return fail(`unknown name '${word}' (use t, c, n, pi, or a function like sin, pulse, step)`);
    expect("(");
    const args: Node[] = [conditional()];
    while (eat(",")) args.push(conditional());
    expect(")");
    if (args.length !== fn.arity) fail(`${word} takes ${fn.arity} argument${fn.arity > 1 ? "s" : ""}`);
    return (t, c, n) => fn.f(...args.map((a) => a(t, c, n)));
  };

  const node = conditional();
  skip();
  if (i < source.length) fail(`unexpected '${source[i]}'`);
  return (t, c, n) => {
    const v = node(t, c, n);
    return Number.isFinite(v) ? v : 0;
  };
}

/** Samples a signal: `values[s * channels + c]` for samples `first … first + count − 1`, `dtMs` apart. */
export function sample(signal: Signal, channels: number, dtMs: number, first: number, count: number): Float32Array {
  const values = new Float32Array(count * channels);
  for (let s = 0; s < count; s++) {
    const t = (first + s) * dtMs;
    for (let c = 0; c < channels; c++) values[s * channels + c] = signal(t, c, channels);
  }
  return values;
}
