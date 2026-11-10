//! The text of data-values: the writer that `convert --from` uses to give back values that it has read.
//!
//! A value is written on one line if that is at most `WIDTH` characters wide, not counting the indentation, the name
//! of the member that it is in, a comma or the `;`. Otherwise it is spread over lines, with one member, element or
//! argument to a line, and each of those is laid out by the same rule. As for the JSON that `convert` writes, there is
//! no maximum column, which nesting by indentation would make meaningless.

use crate::ast::{Argument, Member, TaggedBody, Value, ValueKind};

/// The widest that a value may be and still be written on one line.
const WIDTH: usize = 80;

/// The text of one value, followed by `;` and a new line. When `compact` it is all on one line.
pub fn render(value: &Value, compact: bool) -> String {
    let mut out = String::new();
    if compact {
        let mut flat = Flat::new(usize::MAX);
        flat.value(value);
        out.push_str(&flat.out);
    } else {
        spread(value, 0, &mut out);
    }
    out.push_str(";\n");
    out
}

/// The text of a value on one line, built until it passes a limit, when it is given up.
struct Flat {
    out: String,
    width: usize,
    limit: usize,
    /// Set once the limit has been passed, after which nothing more is written.
    over: bool,
}

impl Flat {
    fn new(limit: usize) -> Flat {
        Flat {
            out: String::new(),
            width: 0,
            limit,
            over: false,
        }
    }

    fn push(&mut self, text: &str) {
        if self.over {
            return;
        }
        self.width = self.width.saturating_add(text.chars().count());
        if self.width > self.limit {
            self.over = true;
        } else {
            self.out.push_str(text);
        }
    }

    /// Writes a list of parts between an opening and a closing, with `", "` between them.
    fn list<T>(&mut self, open: &str, close: &str, items: &[T], each: impl Fn(&mut Flat, &T)) {
        self.push(open);
        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                self.push(", ");
            }
            each(self, item);
            if self.over {
                return;
            }
        }
        self.push(close);
    }

    fn value(&mut self, value: &Value) {
        if self.over {
            return;
        }
        match &value.kind {
            ValueKind::Nat(text) | ValueKind::Int(text) | ValueKind::Float(text) => self.push(text),
            ValueKind::Str(text) => self.push(&quoted(text)),
            ValueKind::Array(items) => self.list("[", "]", items, |f, v| f.value(v)),
            ValueKind::Record(members) => self.record("", members),
            ValueKind::Tagged { name, body } => match body {
                None => self.push(name),
                Some(TaggedBody::Record(members)) => self.record(name, members),
                Some(TaggedBody::Array(items)) => {
                    self.push(name);
                    self.list("[", "]", items, |f, v| f.value(v));
                }
                Some(TaggedBody::Tuple(arguments)) => {
                    self.push(name);
                    self.list("(", ")", arguments, |f, a| f.argument(a));
                }
            },
        }
    }

    fn record(&mut self, name: &str, members: &[Member]) {
        if !name.is_empty() {
            self.push(name);
            self.push(" ");
        }
        if members.is_empty() {
            self.push("{}");
            return;
        }
        self.list("{ ", " }", members, |f, m| f.member(m));
    }

    fn member(&mut self, member: &Member) {
        self.push(&member.name);
        self.push(": ");
        self.value(&member.value);
    }

    fn argument(&mut self, argument: &Argument) {
        if let Some(name) = &argument.name {
            self.push(name);
            self.push(": ");
        }
        self.value(&argument.value);
    }
}

/// The value on one line, if that is at most `WIDTH` characters wide.
fn short(value: &Value) -> Option<String> {
    let mut flat = Flat::new(WIDTH);
    flat.value(value);
    if flat.over {
        None
    } else {
        Some(flat.out)
    }
}

fn indent(out: &mut String, level: usize) {
    for _ in 0..level {
        out.push_str("    ");
    }
}

/// Writes a value laid out by the rule, with its container's lines indented to `level`. The first line is not
/// indented, since the caller has written what comes before it.
fn spread(value: &Value, level: usize, out: &mut String) {
    if let Some(text) = short(value) {
        out.push_str(&text);
        return;
    }
    match &value.kind {
        ValueKind::Array(items) => parts(out, level, "[", "]", items, spread),
        ValueKind::Record(members) => parts(out, level, "{", "}", members, member),
        ValueKind::Tagged { name, body } => match body {
            // A name is written whole, so a bare constructor or a const is never spread, whatever its width.
            None => out.push_str(name),
            Some(TaggedBody::Record(members)) => {
                out.push_str(name);
                out.push(' ');
                parts(out, level, "{", "}", members, member);
            }
            Some(TaggedBody::Array(items)) => {
                out.push_str(name);
                parts(out, level, "[", "]", items, spread);
            }
            Some(TaggedBody::Tuple(arguments)) => {
                out.push_str(name);
                parts(out, level, "(", ")", arguments, argument);
            }
        },
        // The literals are never spread, whatever their width.
        ValueKind::Nat(text) | ValueKind::Int(text) | ValueKind::Float(text) => out.push_str(text),
        ValueKind::Str(text) => out.push_str(&quoted(text)),
    }
}

fn member(m: &Member, level: usize, out: &mut String) {
    out.push_str(&m.name);
    out.push_str(": ");
    spread(&m.value, level, out);
}

fn argument(a: &Argument, level: usize, out: &mut String) {
    if let Some(name) = &a.name {
        out.push_str(name);
        out.push_str(": ");
    }
    spread(&a.value, level, out);
}

/// Writes the opening, the parts one to a line and the closing. The parts are not empty, since an empty one is always
/// short enough to be written on one line.
fn parts<T>(
    out: &mut String,
    level: usize,
    open: &str,
    close: &str,
    items: &[T],
    each: impl Fn(&T, usize, &mut String),
) {
    out.push_str(open);
    out.push('\n');
    for (i, item) in items.iter().enumerate() {
        indent(out, level + 1);
        each(item, level + 1, out);
        if i + 1 < items.len() {
            out.push(',');
        }
        out.push('\n');
    }
    indent(out, level);
    out.push_str(close);
}

/// A string in quotes. The escapes are those of JSON, which the lexer reads, and every control character is escaped,
/// since the lexer refuses one written raw.
fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The value with every position replaced by one place, so that values can be compared.
#[cfg(test)]
pub(crate) fn bare(value: &Value) -> Value {
    let pos = crate::diagnostics::Pos { line: 1, col: 1 };
    let all = |items: &[Value]| items.iter().map(bare).collect::<Vec<_>>();
    let members = |ms: &[Member]| {
        ms.iter()
            .map(|m| Member {
                name: m.name.clone(),
                pos,
                value: bare(&m.value),
            })
            .collect::<Vec<_>>()
    };
    let kind = match &value.kind {
        ValueKind::Array(items) => ValueKind::Array(all(items)),
        ValueKind::Record(ms) => ValueKind::Record(members(ms)),
        ValueKind::Tagged { name, body } => ValueKind::Tagged {
            name: name.clone(),
            body: body.as_ref().map(|b| match b {
                TaggedBody::Record(ms) => TaggedBody::Record(members(ms)),
                TaggedBody::Array(items) => TaggedBody::Array(all(items)),
                TaggedBody::Tuple(args) => TaggedBody::Tuple(
                    args.iter()
                        .map(|a| Argument {
                            name: a.name.clone(),
                            pos,
                            value: bare(&a.value),
                        })
                        .collect(),
                ),
            }),
        },
        other => other.clone(),
    };
    Value { pos, kind }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::Pos;
    use crate::parser::{parse_values_with, Experimental};

    fn values(text: &str) -> Vec<Value> {
        parse_values_with(text, Experimental::default()).unwrap_or_else(|e| panic!("{text}: {e:?}"))
    }

    fn pretty(text: &str) -> String {
        render(&values(text)[0], false)
    }

    fn compact(text: &str) -> String {
        render(&values(text)[0], true)
    }

    #[test]
    fn each_form_on_one_line() {
        let cases = [
            ("5", "5;\n"),
            ("+5", "+5;\n"),
            ("-1.50", "-1.50;\n"),
            ("\"a\"", "\"a\";\n"),
            ("{}", "{};\n"),
            ("{a: 1, b: 2}", "{ a: 1, b: 2 };\n"),
            ("Move {x: 1}", "Move { x: 1 };\n"),
            ("Move {}", "Move {};\n"),
            ("Pair(1, 2)", "Pair(1, 2);\n"),
            ("Span(1, end: 3)", "Span(1, end: 3);\n"),
            ("Ping()", "Ping();\n"),
            ("[1, 2]", "[1, 2];\n"),
            ("[]", "[];\n"),
            ("Items[1, 2]", "Items[1, 2];\n"),
            ("Users[]", "Users[];\n"),
            ("Dot", "Dot;\n"),
            ("red", "red;\n"),
            ("a.b::Dot", "a.b::Dot;\n"),
        ];
        for (text, expected) in cases {
            assert_eq!(pretty(text), expected, "{text}");
            assert_eq!(compact(text), expected, "{text}");
        }
    }

    #[test]
    fn escapes() {
        let value = Value {
            pos: Pos { line: 1, col: 1 },
            kind: ValueKind::Str("\"\\\n\r\t\u{8}\u{c}\u{1}\u{7f}\u{85}é".into()),
        };
        let text = render(&value, false);
        assert_eq!(text, "\"\\\"\\\\\\n\\r\\t\\b\\f\\u0001\\u007f\\u0085é\";\n");
        assert_eq!(values(&text)[0].kind, value.kind);
    }

    #[test]
    fn a_wide_value_is_spread_by_the_rule() {
        let text = format!("[{}]", ["\"0123456789\""; 7].join(", "));
        // Seven strings of twelve characters, with their separators, are 7 * 12 + 6 * 2 + 2 = 98 wide.
        let expected = format!("[\n{}];\n", "    \"0123456789\",\n".repeat(6) + "    \"0123456789\"\n");
        assert_eq!(pretty(&text), expected);
        assert_eq!(compact(&text), format!("{text};\n"));
    }

    #[test]
    fn the_limit_is_eighty_characters() {
        // `[` + 38 one-digit elements and 37 separators + `]` is 2 + 38 + 74 = 114, so use fewer: 26 digits give
        // 2 + 26 + 50 = 78, and 27 give 2 + 27 + 52 = 81.
        let make = |n: usize| format!("[{}]", vec!["1"; n].join(", "));
        assert_eq!(pretty(&make(26)).lines().count(), 1);
        assert_eq!(pretty(&make(27)).lines().count(), 29);
        let mut parts = vec!["1"; 25];
        parts.push("111");
        let exactly = format!("[{}]", parts.join(", "));
        assert_eq!(exactly.len(), 80);
        assert_eq!(pretty(&exactly).lines().count(), 1);
    }

    #[test]
    fn nested_parts_follow_the_rule_themselves() {
        let long = "\"".to_string() + &"x".repeat(60) + "\"";
        let text = format!("Wrap {{ a: [1, 2], b: Pair({long}, {long}), c: Wrap {{ d: 1 }} }}");
        let expected = format!(
            "Wrap {{\n    a: [1, 2],\n    b: Pair(\n        {long},\n        {long}\n    ),\n    \
             c: Wrap {{ d: 1 }}\n}};\n"
        );
        assert_eq!(pretty(&text), expected);
    }

    #[test]
    fn indentation_is_not_counted() {
        // The member is 79 characters wide on its own, and is on one line however deep it is.
        let item = format!("[{}]", vec!["1"; 26].join(", "));
        let text = format!("{{ a: {{ b: {{ c: [{item}, {item}] }} }} }}");
        let out = pretty(&text);
        assert!(
            out.contains(&format!(
                "            c: [\n                {item},\n                {item}\n            ]\n"
            )),
            "{out}"
        );
    }

    #[test]
    fn a_long_name_or_literal_is_never_broken() {
        let name = "A".repeat(100);
        assert_eq!(pretty(&name), format!("{name};\n"));
        let nat = "1".repeat(100);
        assert_eq!(pretty(&format!("[{nat}]")), format!("[\n    {nat}\n];\n"));
    }

    #[test]
    fn what_is_written_is_read_back() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("syntax/examples/values/valid");
        let mut seen = 0;
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let text = std::fs::read_to_string(&path).unwrap();
            for value in values(&text) {
                for compact in [true, false] {
                    let written = render(&value, compact);
                    let again = values(&written);
                    assert_eq!(again.len(), 1, "{written}");
                    assert_eq!(bare(&again[0]), bare(&value), "{}: {written}", path.display());
                    seen += 1;
                }
            }
        }
        assert!(seen > 10);
    }
}
