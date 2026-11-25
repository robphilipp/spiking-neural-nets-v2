//! The generic syntax of a `.boo` file, independent of what the keys mean.
//!
//! ```text
//! file    ::= record
//! record  ::= "(" [ field { "," field } [","] ] ")"
//! field   ::= key "=" value
//! value   ::= record | list | call | random | number [unit] | ids
//! list    ::= "[" [ value { "," value } [","] ] "]"          e.g. a point: [40, 0, 10 µm]
//! call    ::= word "(" [ value { "," value } ] ")"            e.g. uniform(0, 2)
//! random  ::= "r" number ":" number                    e.g. r0:1 (the old form of uniform(0, 1))
//! ids     ::= word [ "{" item { "," item } "}" ]        e.g. in-1, input-1{1:7:1}, inhib-{1,3,5}
//! item    ::= integer ":" integer ":" integer | word    a range (start:end:step, inclusive) or a suffix
//! ```
//!
//! Comments (`// …` and `/* … */`) and whitespace are ignored. Every value keeps its line and column, for
//! error messages.

use std::fmt;

/// A position in the source text (1-based).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
pub struct Span {
    /// Line number.
    pub line: u32,
    /// Column number (in characters).
    pub column: u32,
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.column)
    }
}

/// A problem in a `.boo` file, with where it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// Where.
    pub span: Span,
    /// What.
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "line {}, column {}: {}",
            self.span.line, self.span.column, self.message
        )
    }
}

impl std::error::Error for ParseError {}

/// A parsed value.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Value {
    Record(Vec<Field>),
    List(Vec<Node>),
    Number {
        value: f64,
        unit: Option<String>,
    },
    Random {
        low: f64,
        high: f64,
    },
    /// A word, or the words an id expression expands to.
    Ids(Vec<String>),
    /// `name(arg, …)`, e.g. `uniform(0, 2)`.
    Call {
        name: String,
        args: Vec<Node>,
    },
}

/// A value with its position.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Node {
    pub(crate) span: Span,
    pub(crate) value: Value,
}

/// `key = value`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Field {
    pub(crate) key: String,
    pub(crate) span: Span,
    pub(crate) value: Node,
}

/// Parses a whole file: one top-level record.
pub(crate) fn parse(text: &str) -> Result<Node, ParseError> {
    let mut p = Parser {
        chars: text.chars().collect(),
        pos: 0,
        line: 1,
        column: 1,
    };
    p.skip_trivia()?;
    let node = p.record()?;
    p.skip_trivia()?;
    if p.pos < p.chars.len() {
        return Err(p.error("unexpected text after the closing parenthesis"));
    }
    Ok(node)
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
    line: u32,
    column: u32,
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '.')
}

impl Parser {
    fn span(&self) -> Span {
        Span {
            line: self.line,
            column: self.column,
        }
    }

    fn error(&self, message: impl Into<String>) -> ParseError {
        ParseError {
            span: self.span(),
            message: message.into(),
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += 1;
        if c == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(c)
    }

    /// Skips whitespace and comments.
    fn skip_trivia(&mut self) -> Result<(), ParseError> {
        loop {
            match (self.peek(), self.peek_at(1)) {
                (Some(c), _) if c.is_whitespace() => {
                    self.bump();
                }
                (Some('/'), Some('/')) => {
                    while self.peek().is_some_and(|c| c != '\n') {
                        self.bump();
                    }
                }
                (Some('/'), Some('*')) => {
                    let start = self.span();
                    self.bump();
                    self.bump();
                    loop {
                        match (self.peek(), self.peek_at(1)) {
                            (Some('*'), Some('/')) => {
                                self.bump();
                                self.bump();
                                break;
                            }
                            (Some(_), _) => {
                                self.bump();
                            }
                            (None, _) => {
                                return Err(ParseError {
                                    span: start,
                                    message: "comment is never closed (missing */)".into(),
                                });
                            }
                        }
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    fn expect(&mut self, c: char) -> Result<(), ParseError> {
        self.skip_trivia()?;
        if self.peek() == Some(c) {
            self.bump();
            Ok(())
        } else {
            Err(self.error(match self.peek() {
                Some(found) => format!("expected '{c}', found '{found}'"),
                None => format!("expected '{c}', found the end of the file"),
            }))
        }
    }

    /// Parses `open item { "," item } [","] close`, allowing a trailing comma.
    fn sequence<T>(
        &mut self,
        open: char,
        close: char,
        mut item: impl FnMut(&mut Self) -> Result<T, ParseError>,
    ) -> Result<Vec<T>, ParseError> {
        self.expect(open)?;
        let mut items = Vec::new();
        loop {
            self.skip_trivia()?;
            if self.peek() == Some(close) {
                self.bump();
                return Ok(items);
            }
            items.push(item(self)?);
            self.skip_trivia()?;
            match self.peek() {
                Some(',') => {
                    self.bump();
                }
                Some(c) if c == close => {
                    self.bump();
                    return Ok(items);
                }
                Some(found) => return Err(self.error(format!("expected ',' or '{close}', found '{found}'"))),
                None => return Err(self.error(format!("expected '{close}', found the end of the file"))),
            }
        }
    }

    fn record(&mut self) -> Result<Node, ParseError> {
        self.skip_trivia()?;
        let span = self.span();
        let fields = self.sequence('(', ')', |p| p.field())?;
        Ok(Node {
            span,
            value: Value::Record(fields),
        })
    }

    fn field(&mut self) -> Result<Field, ParseError> {
        self.skip_trivia()?;
        let span = self.span();
        let key = self.word();
        if key.is_empty() || !key.starts_with(|c: char| c.is_ascii_alphabetic()) {
            return Err(self.error("expected a key such as 'name' or 'neurons'"));
        }
        self.expect('=')?;
        let value = self.value()?;
        Ok(Field { key, span, value })
    }

    fn value(&mut self) -> Result<Node, ParseError> {
        self.skip_trivia()?;
        let span = self.span();
        let value = match self.peek() {
            Some('(') => return self.record(),
            Some('[') => Value::List(self.sequence('[', ']', |p| p.value())?),
            Some('r') if self.random_ahead() => {
                self.bump();
                let low = self.number()?;
                self.expect(':')?;
                let high = self.number()?;
                Value::Random { low, high }
            }
            Some(c) if (c.is_ascii_digit() || matches!(c, '-' | '+' | '.')) && self.number_ahead() => {
                let value = self.number()?;
                // A unit may follow, with or without a space: "1 mV", "0µm".
                let save = (self.pos, self.line, self.column);
                self.skip_trivia()?;
                let mut unit = self.unit();
                // A word followed by '=' is the next key (a comma is missing), not a unit.
                let after = self.chars[self.pos..].iter().find(|c| !c.is_whitespace());
                if unit.is_empty() || after == Some(&'=') {
                    unit.clear();
                    (self.pos, self.line, self.column) = save;
                }
                Value::Number {
                    value,
                    unit: (!unit.is_empty()).then_some(unit),
                }
            }
            // A call ('uniform(0, 2)'): a word directly followed by '('.
            Some(c) if c.is_ascii_alphabetic() && self.call_ahead() => {
                let name = self.word();
                let args = self.sequence('(', ')', |p| p.value())?;
                Value::Call { name, args }
            }
            // A word, a word with a set ('in-{1,2}'), or a bare set ('{a, b}').
            Some(c) if is_word_char(c) || c == '{' => Value::Ids(self.ids()?),
            Some(found) => return Err(self.error(format!("expected a value, found '{found}'"))),
            None => return Err(self.error("expected a value, found the end of the file")),
        };
        Ok(Node { span, value })
    }

    /// `true` if the text ahead is a word directly followed by `(`.
    fn call_ahead(&self) -> bool {
        let rest = &self.chars[self.pos..];
        let len = rest
            .iter()
            .take_while(|c| c.is_ascii_alphanumeric() || **c == '_')
            .count();
        len > 0 && rest.get(len) == Some(&'(')
    }

    /// `true` if the text ahead is `r<number>:<number>` rather than a word starting with 'r'.
    fn random_ahead(&self) -> bool {
        let rest: String = self.chars[self.pos + 1..].iter().take(40).collect();
        let Some(colon) = rest.find(':') else {
            return false;
        };
        let (a, b) = (rest[..colon].trim(), &rest[colon + 1..]);
        let b: String = b
            .trim_start()
            .chars()
            .take_while(|c| c.is_ascii_digit() || "+-.eE".contains(*c))
            .collect();
        a.parse::<f64>().is_ok() && b.parse::<f64>().is_ok()
    }

    /// `true` if a number (not, say, an IP address or an id starting with a digit) is ahead.
    fn number_ahead(&self) -> bool {
        let token: String = self.chars[self.pos..]
            .iter()
            .take_while(|c| is_word_char(**c) || matches!(c, '+'))
            .collect();
        // Exclude things like 192.168.1.174 (more than one '.').
        if token.matches('.').count() > 1 {
            return false;
        }
        number_prefix(&token) > 0
    }

    fn number(&mut self) -> Result<f64, ParseError> {
        self.skip_trivia()?;
        let rest: String = self.chars[self.pos..].iter().take(64).collect();
        let len = number_prefix(&rest);
        if len == 0 {
            return Err(self.error("expected a number"));
        }
        let text: String = rest.chars().take(len).collect();
        let value = text
            .parse::<f64>()
            .map_err(|_| self.error(format!("'{text}' is not a valid number")))?;
        for _ in 0..len {
            self.bump();
        }
        Ok(value)
    }

    fn unit(&mut self) -> String {
        let mut unit = String::new();
        while let Some(c) = self.peek() {
            if c.is_alphabetic() || matches!(c, '/' | '°') {
                unit.push(c);
                self.bump();
            } else {
                break;
            }
        }
        unit
    }

    fn word(&mut self) -> String {
        let mut word = String::new();
        while let Some(c) = self.peek() {
            if is_word_char(c) || c == ':' {
                word.push(c);
                self.bump();
            } else {
                break;
            }
        }
        word
    }

    /// A word, optionally followed by `{…}`: the words it expands to.
    fn ids(&mut self) -> Result<Vec<String>, ParseError> {
        let prefix = self.word();
        if self.peek() != Some('{') {
            return Ok(vec![prefix]);
        }
        let items = self.sequence('{', '}', |p| {
            p.skip_trivia()?;
            let span = p.span();
            let item = p.word();
            if item.is_empty() {
                return Err(p.error("expected a suffix or a range such as 1:7:1"));
            }
            expand_item(&item).map_err(|message| ParseError { span, message })
        })?;
        Ok(items
            .into_iter()
            .flatten()
            .map(|suffix| format!("{prefix}{suffix}"))
            .collect())
    }
}

/// Expands `start:end:step` (inclusive) to its numbers, or returns a plain suffix as is.
fn expand_item(item: &str) -> Result<Vec<String>, String> {
    let parts: Vec<&str> = item.split(':').collect();
    match parts.as_slice() {
        [single] => Ok(vec![(*single).to_string()]),
        [start, end, step] => {
            let parse = |s: &str| {
                s.parse::<i64>()
                    .map_err(|_| format!("range '{item}': '{s}' is not an integer"))
            };
            let (start, end, step) = (parse(start)?, parse(end)?, parse(step)?);
            if step <= 0 {
                return Err(format!("range '{item}': the step must be positive"));
            }
            Ok((start..=end).step_by(step as usize).map(|i| i.to_string()).collect())
        }
        _ => Err(format!("'{item}' is neither a suffix nor a range start:end:step")),
    }
}

/// Length of the longest prefix of `s` that is a number: `[+-]?(digits[.digits?] | .digits)([eE][+-]?digits)?`.
fn number_prefix(s: &str) -> usize {
    let b = s.as_bytes();
    let mut i = 0;
    if i < b.len() && matches!(b[i], b'+' | b'-') {
        i += 1;
    }
    let int_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let mut digits = i - int_start;
    if i < b.len() && b[i] == b'.' {
        let frac_start = i + 1;
        let mut j = frac_start;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if digits > 0 || j > frac_start {
            digits += j - frac_start;
            i = j;
        }
    }
    if digits == 0 {
        return 0;
    }
    // An exponent only counts if digits follow it, so "5 mV" or "2 m/s" keep their units.
    if i < b.len() && matches!(b[i], b'e' | b'E') {
        let mut j = i + 1;
        if j < b.len() && matches!(b[j], b'+' | b'-') {
            j += 1;
        }
        let exp_start = j;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j > exp_start {
            i = j;
        }
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(text: &str) -> Value {
        let node = parse(&format!("(x={text})")).unwrap();
        let Value::Record(mut fields) = node.value else {
            panic!()
        };
        fields.remove(0).value.value
    }

    #[test]
    fn numbers_and_units() {
        assert_eq!(value("1.5"), Value::Number { value: 1.5, unit: None });
        assert_eq!(
            value("-30 µm"),
            Value::Number {
                value: -30.0,
                unit: Some("µm".into())
            }
        );
        assert_eq!(
            value("0µm"),
            Value::Number {
                value: 0.0,
                unit: Some("µm".into())
            }
        );
        assert_eq!(
            value("6e-4"),
            Value::Number {
                value: 6e-4,
                unit: None
            }
        );
        assert_eq!(
            value("1e-5"),
            Value::Number {
                value: 1e-5,
                unit: None
            }
        );
        assert_eq!(
            value(".5 s"),
            Value::Number {
                value: 0.5,
                unit: Some("s".into())
            }
        );
        assert_eq!(
            value("1 m/s"),
            Value::Number {
                value: 1.0,
                unit: Some("m/s".into())
            }
        );
        assert_eq!(value("0."), Value::Number { value: 0.0, unit: None });
    }

    #[test]
    fn ids_and_expansions() {
        assert_eq!(value("in-1"), Value::Ids(vec!["in-1".into()]));
        assert_eq!(value("stdp_hard"), Value::Ids(vec!["stdp_hard".into()]));
        assert_eq!(value("192.168.1.174"), Value::Ids(vec!["192.168.1.174".into()]));
        assert_eq!(
            value("input-1{1:7:3}"),
            Value::Ids(vec!["input-11".into(), "input-14".into(), "input-17".into()])
        );
        assert_eq!(
            value("inhib-{2, 3, 7}"),
            Value::Ids(vec!["inhib-2".into(), "inhib-3".into(), "inhib-7".into()])
        );
        assert_eq!(value("r0.2:0.4"), Value::Random { low: 0.2, high: 0.4 });
        assert_eq!(value("ring-1"), Value::Ids(vec!["ring-1".into()]));
        assert_eq!(value("{a, b}"), Value::Ids(vec!["a".into(), "b".into()]));
    }

    #[test]
    fn calls_and_points() {
        let Value::Call { name, args } = value("uniform(0.5, 2)") else {
            panic!()
        };
        assert_eq!(name, "uniform");
        assert_eq!(args.len(), 2);
        assert_eq!(args[1].value, Value::Number { value: 2.0, unit: None });
        // A word then a space then '(' is not a call.
        assert!(parse("(x=uniform (1, 2))").is_err());
        let Value::List(point) = value("[40, 0, 10 µm]") else {
            panic!()
        };
        assert_eq!(point.len(), 3);
    }

    #[test]
    fn comments_trailing_commas_and_nesting() {
        let node =
            parse("// header\n( /* block\n comment */ A=[ (b=1, c=2 mV,), ], // trailing\n D=(e=f),\n)").unwrap();
        let Value::Record(fields) = node.value else { panic!() };
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].key, "A");
        assert_eq!(fields[1].span, Span { line: 4, column: 2 });
    }

    #[test]
    fn errors_point_at_the_problem() {
        let err = parse("(a=1,\n b=2 c=3)").unwrap_err();
        assert_eq!(err.span, Span { line: 2, column: 6 });
        assert!(err.message.contains("expected ','"), "{err}");
        assert!(
            parse("(a=input-{1:5:0})")
                .unwrap_err()
                .message
                .contains("step must be positive")
        );
        assert!(parse("(a=1 /* never closed").is_err());
    }
}
