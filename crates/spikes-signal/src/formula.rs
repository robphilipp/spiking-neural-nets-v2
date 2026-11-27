//! The signal formula language: a small expression language over time `t` (ms), channel `c` (0…n−1) and the
//! channel count `n`. The same language as the UI's designer (`apps/spikes-ui/src/signal.ts`): a test compares the
//! two on the same formulas (`tests/parity.rs` writes the samples the UI's test checks).
//!
//! ```text
//! numbers, t, c, n, pi            + − * / % ^ (power), unary −, parentheses
//! < <= > >= == != (1 or 0)        a ? b : c
//! sin cos tan exp log sqrt abs floor ceil round   min(a, b) max(a, b) pow(a, b)
//! step(x)        1 for x ≥ 0, else 0
//! pulse(t, period, width)   1 for the first `width` of every `period`, else 0
//! clamp(x, lo, hi)
//! data("name", t, c)   the data file `name` (CSV or WAV, see `data`) at time `t` ms, channel `c`, linearly
//!                      interpolated; 0 outside the file's samples and for a channel it doesn't have
//! data("name", t)      channel 0
//! rand()         uniform in [0, 1): a fresh value per sample and channel (seeded: see `noise`)
//! gauss()        standard normal, likewise
//! ```
//!
//! `%` is a modulus that is never negative for a positive divisor; `round` rounds halves up (as JavaScript does);
//! a result that isn't finite is 0.

use serde::{Deserialize, Serialize};

use crate::data::DataSets;
use crate::noise;

/// A malformed formula: what's wrong, and where (a character offset).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormulaError {
    pub message: String,
    pub position: usize,
}

impl std::fmt::Display for FormulaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (at character {})", self.message, self.position + 1)
    }
}

impl std::error::Error for FormulaError {}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Func {
    Sin,
    Cos,
    Tan,
    Exp,
    Log,
    Sqrt,
    Abs,
    Floor,
    Ceil,
    Round,
    Step,
    Min,
    Max,
    Pow,
    Pulse,
    Clamp,
}

impl Func {
    fn named(name: &str) -> Option<(Self, usize)> {
        Some(match name {
            "sin" => (Self::Sin, 1),
            "cos" => (Self::Cos, 1),
            "tan" => (Self::Tan, 1),
            "exp" => (Self::Exp, 1),
            "log" => (Self::Log, 1),
            "sqrt" => (Self::Sqrt, 1),
            "abs" => (Self::Abs, 1),
            "floor" => (Self::Floor, 1),
            "ceil" => (Self::Ceil, 1),
            "round" => (Self::Round, 1),
            "step" => (Self::Step, 1),
            "min" => (Self::Min, 2),
            "max" => (Self::Max, 2),
            "pow" => (Self::Pow, 2),
            "pulse" => (Self::Pulse, 3),
            "clamp" => (Self::Clamp, 3),
            _ => return None,
        })
    }

    fn apply(self, a: &[f64]) -> f64 {
        match self {
            Self::Sin => a[0].sin(),
            Self::Cos => a[0].cos(),
            Self::Tan => a[0].tan(),
            Self::Exp => a[0].exp(),
            Self::Log => a[0].ln(),
            Self::Sqrt => a[0].sqrt(),
            Self::Abs => a[0].abs(),
            Self::Floor => a[0].floor(),
            Self::Ceil => a[0].ceil(),
            Self::Round => js_round(a[0]),
            Self::Step => f64::from(u8::from(a[0] >= 0.0)),
            Self::Min => js_min(a[0], a[1]),
            Self::Max => js_max(a[0], a[1]),
            Self::Pow => a[0].powf(a[1]),
            Self::Pulse => {
                let (t, period, width) = (a[0], a[1], a[2]);
                f64::from(u8::from(period > 0.0 && ((t % period) + period) % period < width))
            }
            Self::Clamp => js_min(js_max(a[0], a[1]), a[2]),
        }
    }
}

/// `Math.round`: halves round up (−2.5 → −2), unlike `f64::round`.
fn js_round(x: f64) -> f64 {
    if !x.is_finite() {
        return x;
    }
    let r = x.floor();
    if x - r >= 0.5 { r + 1.0 } else { r }
}

/// `Math.min` and `Math.max`: NaN if either is NaN (unlike `f64::min`).
fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() { f64::NAN } else { a.min(b) }
}

fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() { f64::NAN } else { a.max(b) }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Cmp {
    Le,
    Ge,
    Eq,
    Ne,
    Lt,
    Gt,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
enum Node {
    Number(f64),
    T,
    C,
    N,
    Neg(Box<Node>),
    Add(Box<Node>, Box<Node>),
    Sub(Box<Node>, Box<Node>),
    Mul(Box<Node>, Box<Node>),
    Div(Box<Node>, Box<Node>),
    Mod(Box<Node>, Box<Node>),
    Pow(Box<Node>, Box<Node>),
    Compare(Cmp, Box<Node>, Box<Node>),
    If(Box<Node>, Box<Node>, Box<Node>),
    Call(Func, Vec<Node>),
    /// `rand()`, with its call site (so two `rand()`s in one formula are independent).
    Rand(u32),
    /// `gauss()`, with its call site.
    Gauss(u32),
    /// `data("name", t, c)`: the data file's name, the time, and the channel (channel 0 if omitted).
    Data(String, Box<Node>, Option<Box<Node>>),
}

/// Which channel of a data file a `data(…)` call reads, as far as the formula says.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataChannel {
    /// No channel argument: channel 0.
    Zero,
    /// The formula's own channel `c`.
    C,
    /// A whole number written in the formula.
    Fixed(u32),
    /// Anything else (computed).
    Computed,
}

/// A `data(…)` call in a formula: for checking, when the inputs are given to a run, that the files exist and have
/// the channels.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataUse {
    pub name: String,
    pub channel: DataChannel,
}

/// Where a formula is evaluated: which sample, and the noise stream's key (see [`noise::key`]).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct At {
    /// The sample's index (from the source's first sample).
    pub sample: u64,
    /// The noise key: the run's seed and the source's name.
    pub key: u64,
}

/// A compiled formula.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Formula {
    source: String,
    node: Node,
    uses: Vec<DataUse>,
}

impl Formula {
    /// Compiles `source`.
    ///
    /// # Errors
    /// A [`FormulaError`] for a malformed formula.
    pub fn compile(source: &str) -> Result<Self, FormulaError> {
        let mut p = Parser {
            src: source.as_bytes(),
            text: source,
            i: 0,
            sites: 0,
            uses: Vec::new(),
        };
        let node = p.conditional()?;
        p.skip();
        if p.i < p.src.len() {
            return Err(p.fail(&format!("unexpected '{}'", p.current_char())));
        }
        Ok(Self {
            source: source.to_string(),
            node,
            uses: p.uses,
        })
    }

    /// The formula as written.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// The value at time `t` (ms), for channel `c` of `n`, at `at`, with no data files (`data(…)` is 0). Not finite:
    /// 0.
    pub fn eval(&self, t: f64, c: u32, n: u32, at: At) -> f64 {
        self.eval_with(t, c, n, at, &DataSets::default())
    }

    /// Like [`Self::eval`], with the data files `data(…)` looks up.
    pub fn eval_with(&self, t: f64, c: u32, n: u32, at: At, data: &DataSets) -> f64 {
        let v = eval(&self.node, t, c, n, at, data);
        if v.is_finite() { v } else { 0.0 }
    }

    /// The `data(…)` calls the formula makes.
    pub fn data_uses(&self) -> &[DataUse] {
        &self.uses
    }
}

fn eval(node: &Node, t: f64, c: u32, n: u32, at: At, data: &DataSets) -> f64 {
    let e = |x: &Node| eval(x, t, c, n, at, data);
    match node {
        Node::Number(v) => *v,
        Node::T => t,
        Node::C => f64::from(c),
        Node::N => f64::from(n),
        Node::Neg(x) => -e(x),
        Node::Add(a, b) => e(a) + e(b),
        Node::Sub(a, b) => e(a) - e(b),
        Node::Mul(a, b) => e(a) * e(b),
        Node::Div(a, b) => e(a) / e(b),
        Node::Mod(a, b) => {
            let b = e(b);
            ((e(a) % b) + b) % b
        }
        Node::Pow(a, b) => e(a).powf(e(b)),
        Node::Compare(op, a, b) => {
            let (a, b) = (e(a), e(b));
            let r = match op {
                Cmp::Le => a <= b,
                Cmp::Ge => a >= b,
                Cmp::Eq => a == b,
                Cmp::Ne => a != b,
                Cmp::Lt => a < b,
                Cmp::Gt => a > b,
            };
            f64::from(u8::from(r))
        }
        Node::If(test, yes, no) => {
            if e(test) != 0.0 {
                e(yes)
            } else {
                e(no)
            }
        }
        Node::Call(f, args) => {
            let values: Vec<f64> = args.iter().map(e).collect();
            f.apply(&values)
        }
        Node::Rand(site) => noise::uniform(at.key, c, at.sample, *site, 0),
        Node::Gauss(site) => noise::gauss(at.key, c, at.sample, *site),
        Node::Data(name, time, channel) => {
            let time = e(time);
            // A channel that isn't a whole number in range is no channel: 0.
            let channel = channel.as_ref().map_or(0.0, |x| e(x));
            match data.get(name) {
                Some(set) if channel >= 0.0 && channel.fract() == 0.0 && channel < 4.0e9 => {
                    set.at(time, channel as usize)
                }
                _ => 0.0,
            }
        }
    }
}

struct Parser<'a> {
    src: &'a [u8],
    text: &'a str,
    i: usize,
    sites: u32,
    uses: Vec<DataUse>,
}

impl Parser<'_> {
    fn fail(&self, message: &str) -> FormulaError {
        FormulaError {
            message: message.to_string(),
            position: self.text[..self.i].chars().count(),
        }
    }

    fn current_char(&self) -> char {
        self.text[self.i..].chars().next().unwrap_or(' ')
    }

    fn skip(&mut self) {
        while self.i < self.src.len() && self.src[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }

    fn eat(&mut self, token: &str) -> bool {
        self.skip();
        if self.src[self.i..].starts_with(token.as_bytes()) {
            self.i += token.len();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, token: &str) -> Result<(), FormulaError> {
        if self.eat(token) {
            Ok(())
        } else {
            Err(self.fail(&format!("expected '{token}'")))
        }
    }

    // conditional := comparison ('?' conditional ':' conditional)?
    fn conditional(&mut self) -> Result<Node, FormulaError> {
        let test = self.comparison()?;
        if !self.eat("?") {
            return Ok(test);
        }
        let yes = self.conditional()?;
        self.expect(":")?;
        let no = self.conditional()?;
        Ok(Node::If(Box::new(test), Box::new(yes), Box::new(no)))
    }

    fn comparison(&mut self) -> Result<Node, FormulaError> {
        let mut left = self.additive()?;
        loop {
            let op = [
                ("<=", Cmp::Le),
                (">=", Cmp::Ge),
                ("==", Cmp::Eq),
                ("!=", Cmp::Ne),
                ("<", Cmp::Lt),
                (">", Cmp::Gt),
            ]
            .into_iter()
            .find(|(token, _)| self.eat(token));
            let Some((_, op)) = op else { return Ok(left) };
            let right = self.additive()?;
            left = Node::Compare(op, Box::new(left), Box::new(right));
        }
    }

    fn additive(&mut self) -> Result<Node, FormulaError> {
        let mut left = self.multiplicative()?;
        loop {
            if self.eat("+") {
                left = Node::Add(Box::new(left), Box::new(self.multiplicative()?));
            } else if self.eat("-") {
                left = Node::Sub(Box::new(left), Box::new(self.multiplicative()?));
            } else {
                return Ok(left);
            }
        }
    }

    fn multiplicative(&mut self) -> Result<Node, FormulaError> {
        let mut left = self.unary()?;
        loop {
            if self.eat("*") {
                left = Node::Mul(Box::new(left), Box::new(self.unary()?));
            } else if self.eat("/") {
                left = Node::Div(Box::new(left), Box::new(self.unary()?));
            } else if self.eat("%") {
                left = Node::Mod(Box::new(left), Box::new(self.unary()?));
            } else {
                return Ok(left);
            }
        }
    }

    // unary := '-' unary | power;  power := primary ('^' unary)?  (so -2^2 is -(2^2), and 2^-1 works)
    fn unary(&mut self) -> Result<Node, FormulaError> {
        if self.eat("-") {
            return Ok(Node::Neg(Box::new(self.unary()?)));
        }
        self.eat("+");
        let base = self.primary()?;
        if !self.eat("^") {
            return Ok(base);
        }
        let exponent = self.unary()?; // right-associative
        Ok(Node::Pow(Box::new(base), Box::new(exponent)))
    }

    fn primary(&mut self) -> Result<Node, FormulaError> {
        self.skip();
        if self.eat("(") {
            let x = self.conditional()?;
            self.expect(")")?;
            return Ok(x);
        }
        if let Some(len) = number_len(&self.src[self.i..]) {
            let text = &self.text[self.i..self.i + len];
            self.i += len;
            let v: f64 = text.parse().map_err(|_| self.fail("malformed number"))?;
            return Ok(Node::Number(v));
        }
        let start = self.i;
        if self.i < self.src.len() && (self.src[self.i].is_ascii_alphabetic() || self.src[self.i] == b'_') {
            self.i += 1;
            while self.i < self.src.len() && (self.src[self.i].is_ascii_alphanumeric() || self.src[self.i] == b'_') {
                self.i += 1;
            }
        } else if self.i >= self.src.len() {
            return Err(self.fail("unexpected end of the formula"));
        } else {
            return Err(self.fail(&format!("unexpected '{}'", self.current_char())));
        }
        let word = &self.text[start..self.i];
        match word {
            "t" => return Ok(Node::T),
            "c" => return Ok(Node::C),
            "n" => return Ok(Node::N),
            "pi" => return Ok(Node::Number(std::f64::consts::PI)),
            "rand" | "gauss" => {
                self.expect("(")?;
                self.expect(")")?;
                let site = self.sites;
                self.sites += 1;
                return Ok(if word == "rand" {
                    Node::Rand(site)
                } else {
                    Node::Gauss(site)
                });
            }
            _ => {}
        }
        if word == "data" {
            return self.data_call();
        }
        let Some((func, arity)) = Func::named(word) else {
            self.i = start;
            return Err(self.fail(&format!(
                "unknown name '{word}' (use t, c, n, pi, or a function like sin, pulse, step, rand, data)"
            )));
        };
        self.expect("(")?;
        let mut args = vec![self.conditional()?];
        while self.eat(",") {
            args.push(self.conditional()?);
        }
        self.expect(")")?;
        if args.len() != arity {
            return Err(self.fail(&format!(
                "{word} takes {arity} argument{}",
                if arity > 1 { "s" } else { "" }
            )));
        }
        Ok(Node::Call(func, args))
    }
}

impl Parser<'_> {
    // data := 'data' '(' string ',' conditional (',' conditional)? ')'   (the name is in double quotes, no escapes)
    fn data_call(&mut self) -> Result<Node, FormulaError> {
        self.expect("(")?;
        self.skip();
        if !self.eat("\"") {
            return Err(
                self.fail("data takes the file's name in quotes, then t, and optionally c: data(\"name\", t, c)")
            );
        }
        let start = self.i;
        let Some(len) = self.text[start..].find('"') else {
            return Err(self.fail("the data name needs a closing quote"));
        };
        let name = self.text[start..start + len].to_string();
        self.i = start + len + 1;
        self.expect(",")?;
        let time = self.conditional()?;
        let (channel, kind) = if self.eat(",") {
            let from = self.i;
            let node = self.conditional()?;
            let text = self.text[from..self.i].trim();
            let kind = if text == "c" {
                DataChannel::C
            } else if !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) {
                text.parse().map_or(DataChannel::Computed, DataChannel::Fixed)
            } else {
                DataChannel::Computed
            };
            (Some(Box::new(node)), kind)
        } else {
            (None, DataChannel::Zero)
        };
        self.expect(")")?;
        self.uses.push(DataUse {
            name: name.clone(),
            channel: kind,
        });
        Ok(Node::Data(name, Box::new(time), channel))
    }
}

/// The length of a number at the start of `s`: `\d+(\.\d*)?|\.\d+`, then an optional exponent.
fn number_len(s: &[u8]) -> Option<usize> {
    let digits = |from: usize| s[from..].iter().take_while(|b| b.is_ascii_digit()).count();
    let mut i = digits(0);
    if i > 0 {
        if s.get(i) == Some(&b'.') {
            i += 1 + digits(i + 1);
        }
    } else if s.first() == Some(&b'.') && digits(1) > 0 {
        i = 1 + digits(1);
    } else {
        return None;
    }
    if matches!(s.get(i), Some(b'e' | b'E')) {
        let mut j = i + 1;
        if matches!(s.get(j), Some(b'+' | b'-')) {
            j += 1;
        }
        let d = digits(j);
        if d > 0 {
            i = j + d;
        }
    }
    Some(i)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(src: &str, t: f64, c: u32, n: u32) -> f64 {
        Formula::compile(src).unwrap().eval(t, c, n, At::default())
    }

    #[test]
    fn arithmetic_and_precedence() {
        assert_eq!(v("1 + 2 * 3", 0.0, 0, 1), 7.0);
        assert_eq!(v("-2^2", 0.0, 0, 1), -4.0);
        assert_eq!(v("2^-1", 0.0, 0, 1), 0.5);
        assert_eq!(v("2^3^2", 0.0, 0, 1), 512.0);
        assert_eq!(v("-7 % 3", 0.0, 0, 1), 2.0);
        assert_eq!(v("t > 5 ? c : n", 10.0, 3, 8), 3.0);
        assert_eq!(v("t > 5 ? c : n", 1.0, 3, 8), 8.0);
        assert_eq!(v("floor(t / 100) % 7 == c % 7", 250.0, 2, 7), 1.0);
        assert_eq!(v("pulse(t, 100, 30)", 120.0, 0, 1), 1.0);
        assert_eq!(v("pulse(t, 100, 30)", 140.0, 0, 1), 0.0);
        assert_eq!(v("1 / 0", 0.0, 0, 1), 0.0, "not finite: 0");
        assert_eq!(v("round(-2.5)", 0.0, 0, 1), -2.0, "halves round up, as in JavaScript");
        assert_eq!(v("round(2.5)", 0.0, 0, 1), 3.0);
        assert_eq!(v("1.5e2 + .5", 0.0, 0, 1), 150.5);
    }

    #[test]
    fn errors_say_what_and_where() {
        let e = Formula::compile("sin(t").unwrap_err();
        assert_eq!(e.message, "expected ')'");
        let e = Formula::compile("foo(t)").unwrap_err();
        assert!(e.message.starts_with("unknown name 'foo'"));
        assert_eq!(e.position, 0);
        let e = Formula::compile("min(1)").unwrap_err();
        assert_eq!(e.message, "min takes 2 arguments");
        let e = Formula::compile("1 +").unwrap_err();
        assert_eq!(e.message, "unexpected end of the formula");
        let e = Formula::compile("1 $ 2").unwrap_err();
        assert_eq!((e.message.as_str(), e.position), ("unexpected '$'", 2));
    }

    #[test]
    fn noise_is_seeded_per_sample_channel_and_call() {
        let f = Formula::compile("rand()").unwrap();
        let at = |sample, key| At { sample, key };
        let a = f.eval(0.0, 0, 2, at(5, 1));
        assert_eq!(
            a,
            f.eval(99.0, 0, 2, at(5, 1)),
            "the same sample, channel and key: the same value"
        );
        assert_ne!(a, f.eval(0.0, 1, 2, at(5, 1)), "another channel");
        assert_ne!(a, f.eval(0.0, 0, 2, at(6, 1)), "another sample");
        assert_ne!(a, f.eval(0.0, 0, 2, at(5, 2)), "another key");
        assert!((0.0..1.0).contains(&a));
        let two = Formula::compile("rand() - rand()").unwrap();
        assert_ne!(two.eval(0.0, 0, 1, at(0, 1)), 0.0, "two calls are independent");
    }

    #[test]
    fn data_lookups_and_the_calls_a_formula_makes() {
        use crate::data::{DataFile, DataSets};
        let sets = DataSets::load(&[DataFile {
            name: "d".into(),
            bytes: b"t,a,b\n0,0,10\n100,1,20\n".to_vec(),
        }])
        .unwrap();
        let at = |src: &str, t: f64, c: u32| Formula::compile(src).unwrap().eval_with(t, c, 2, At::default(), &sets);
        assert_eq!(at("data(\"d\", t, c)", 50.0, 0), 0.5);
        assert_eq!(at("data(\"d\", t, c)", 50.0, 1), 15.0);
        assert_eq!(at("data(\"d\", t)", 50.0, 1), 0.5, "channel 0 without c");
        assert_eq!(at("data(\"d\", t, c)", 150.0, 0), 0.0, "after the last sample");
        assert_eq!(at("data(\"d\", t, c)", 50.0, 2), 0.0, "no such channel");
        assert_eq!(at("data(\"d\", t, 1.5)", 50.0, 0), 0.0, "not a whole channel");
        assert_eq!(at("data(\"nope\", t)", 50.0, 0), 0.0, "no such file");
        assert_eq!(at("2 * data(\"d\", t * 2 - 10, c + 1)", 30.0, 0), 30.0);
        let f =
            Formula::compile("data(\"a\", t) + data(\"b\", t, c) + data(\"c\", t, 3) + data(\"d\", t, c + 1)").unwrap();
        let uses: Vec<(&str, DataChannel)> = f
            .data_uses()
            .iter()
            .map(|u| (u.name.as_str(), u.channel.clone()))
            .collect();
        assert_eq!(
            uses,
            [
                ("a", DataChannel::Zero),
                ("b", DataChannel::C),
                ("c", DataChannel::Fixed(3)),
                ("d", DataChannel::Computed)
            ]
        );
        assert!(
            Formula::compile("data(t, 1)")
                .unwrap_err()
                .message
                .starts_with("data takes the file's name")
        );
        assert!(
            Formula::compile("data(\"x, 1)")
                .unwrap_err()
                .message
                .contains("closing quote")
        );
        assert!(Formula::compile("data(\"x\")").is_err());
    }
}
