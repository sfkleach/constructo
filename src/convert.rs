//! Conversion of values to other notations.
//!
//! The only format at present is `json-exact`, which writes every kind of value in an explicit form and needs no
//! types. See `docs/tasks/2026-11-08-convert-subcommand/convert-subcommand.md`.

use crate::ast::{Argument, Member, TaggedBody, Value, ValueKind};
use crate::diagnostics::{Diagnostic, Pos};
use std::collections::HashSet;

/// A JSON value, of the three kinds that `json-exact` needs. An object keeps its members in the order given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Json {
    Str(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    /// Writes the value with no white space, or when not compact, with two spaces of indentation and a new line at
    /// the end. A container that holds no other container is written on one line even then, so that a number is
    /// `{"nat": "5"}` and not five lines.
    pub fn write(&self, compact: bool) -> String {
        let mut out = String::new();
        if compact {
            self.write_compact(&mut out);
        } else {
            self.write_pretty(&mut out, 0);
            out.push('\n');
        }
        out
    }

    fn write_compact(&self, out: &mut String) {
        match self {
            Json::Str(s) => write_string(out, s),
            Json::Array(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    item.write_compact(out);
                }
                out.push(']');
            }
            Json::Object(members) => {
                out.push('{');
                for (i, (key, value)) in members.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write_string(out, key);
                    out.push(':');
                    value.write_compact(out);
                }
                out.push('}');
            }
        }
    }

    /// Whether the value is a container that holds another container, which is what makes it span several lines.
    fn is_nested(&self) -> bool {
        match self {
            Json::Str(_) => false,
            Json::Array(items) => items.iter().any(Json::is_container),
            Json::Object(members) => members.iter().any(|(_, value)| value.is_container()),
        }
    }

    fn is_container(&self) -> bool {
        !matches!(self, Json::Str(_))
    }

    fn write_pretty(&self, out: &mut String, depth: usize) {
        if !self.is_nested() {
            self.write_flat(out);
            return;
        }
        let inner = "  ".repeat(depth + 1);
        let (open, close) = if matches!(self, Json::Array(_)) {
            ('[', ']')
        } else {
            ('{', '}')
        };
        out.push(open);
        out.push('\n');
        match self {
            Json::Array(items) => {
                for (i, item) in items.iter().enumerate() {
                    out.push_str(&inner);
                    item.write_pretty(out, depth + 1);
                    out.push_str(if i + 1 < items.len() { ",\n" } else { "\n" });
                }
            }
            Json::Object(members) => {
                for (i, (key, value)) in members.iter().enumerate() {
                    out.push_str(&inner);
                    write_string(out, key);
                    out.push_str(": ");
                    value.write_pretty(out, depth + 1);
                    out.push_str(if i + 1 < members.len() { ",\n" } else { "\n" });
                }
            }
            Json::Str(_) => unreachable!("a string is never nested"),
        }
        out.push_str(&"  ".repeat(depth));
        out.push(close);
    }

    /// Writes a value whose containers hold no containers, on one line, with a space after each `:` and `,`.
    fn write_flat(&self, out: &mut String) {
        match self {
            Json::Str(s) => write_string(out, s),
            Json::Array(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    item.write_flat(out);
                }
                out.push(']');
            }
            Json::Object(members) => {
                out.push('{');
                for (i, (key, value)) in members.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    write_string(out, key);
                    out.push_str(": ");
                    value.write_flat(out);
                }
                out.push('}');
            }
        }
    }
}

/// Writes a string with the escapes of RFC 8259: the quote, the backslash and the control characters are escaped, and
/// everything else, including what is not ASCII, is written as it is.
fn write_string(out: &mut String, text: &str) {
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
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn string(text: &str) -> Json {
    Json::Str(text.to_string())
}

fn object(members: Vec<(&str, Json)>) -> Json {
    Json::Object(members.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

/// Encodes a value as `json-exact`. The `file` is only used to name the source in an error.
///
/// The only errors are those of a name that is given twice, which a JSON object cannot hold. Nothing else is checked,
/// as there are no types: `[1, "x"]` is encoded as it is.
pub fn encode_exact(file: &str, value: &Value) -> Result<Json, Diagnostic> {
    Ok(match &value.kind {
        ValueKind::Nat(text) => object(vec![("nat", string(text))]),
        ValueKind::Int(text) => object(vec![("int", string(text))]),
        ValueKind::Float(text) => object(vec![("float", string(text))]),
        ValueKind::Str(text) => object(vec![("string", string(text))]),
        ValueKind::Array(items) => object(vec![("array", encode_all(file, items)?)]),
        ValueKind::Record(members) => object(vec![("record", encode_members(file, members)?)]),
        ValueKind::Tagged { name, body } => {
            // A name whose last segment starts with a lower-case letter or `_` is a const, and any other is a
            // constructor. The qualifier, as in `a.b::red`, does not count.
            let last = name.rsplit("::").next().unwrap_or(name);
            let is_const = last.chars().next().is_some_and(|c| c.is_lowercase() || c == '_');
            if is_const {
                return Ok(object(vec![("const", string(name))]));
            }
            let mut members = vec![("constructor".to_string(), string(name))];
            match body {
                None => {}
                Some(TaggedBody::Record(fields)) => members.push(("record".to_string(), encode_members(file, fields)?)),
                Some(TaggedBody::Array(items)) => members.push(("array".to_string(), encode_all(file, items)?)),
                Some(TaggedBody::Tuple(arguments)) => encode_tuple(file, name, arguments, &mut members)?,
            }
            Json::Object(members)
        }
    })
}

fn encode_all(file: &str, values: &[Value]) -> Result<Json, Diagnostic> {
    values
        .iter()
        .map(|v| encode_exact(file, v))
        .collect::<Result<Vec<_>, _>>()
        .map(Json::Array)
}

fn encode_members(file: &str, members: &[Member]) -> Result<Json, Diagnostic> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for member in members {
        if !seen.insert(member.name.as_str()) {
            return Err(duplicate(file, member.pos, format!("Duplicate field {}", member.name)));
        }
        out.push((member.name.clone(), encode_exact(file, &member.value)?));
    }
    Ok(Json::Object(out))
}

/// Adds `"tuple"` for the positional arguments and, if there are named ones, `"named"` after it. The grammar puts the
/// positional arguments first.
fn encode_tuple(
    file: &str,
    constructor: &str,
    arguments: &[Argument],
    members: &mut Vec<(String, Json)>,
) -> Result<(), Diagnostic> {
    let mut positional = Vec::new();
    let mut named = Vec::new();
    let mut seen = HashSet::new();
    for argument in arguments {
        let encoded = encode_exact(file, &argument.value)?;
        match &argument.name {
            None => positional.push(encoded),
            Some(name) => {
                if !seen.insert(name.as_str()) {
                    let message = format!("Argument {} is given more than once in {}", name, constructor);
                    return Err(duplicate(file, argument.pos, message));
                }
                named.push((name.clone(), encoded));
            }
        }
    }
    members.push(("tuple".to_string(), Json::Array(positional)));
    if !named.is_empty() {
        members.push(("named".to_string(), Json::Object(named)));
    }
    Ok(())
}

fn duplicate(file: &str, pos: Pos, message: String) -> Diagnostic {
    Diagnostic::new(file, pos, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{parse_values_with, Experimental};

    fn convert_with(text: &str, compact: bool, experimental: Experimental) -> Result<String, Diagnostic> {
        let values = parse_values_with(text, experimental).expect("the text should parse");
        assert_eq!(values.len(), 1, "one value is expected");
        encode_exact("test.values", &values[0]).map(|json| json.write(compact))
    }

    fn compact(text: &str) -> String {
        convert_with(text, true, Experimental::default()).expect("the value should convert")
    }

    fn pretty(text: &str) -> String {
        convert_with(text, false, Experimental::default()).expect("the value should convert")
    }

    fn error(text: &str) -> String {
        convert_with(text, true, Experimental::default())
            .expect_err("the value should not convert")
            .to_string()
    }

    #[test]
    fn numbers_keep_their_text() {
        assert_eq!(compact("5"), r#"{"nat":"5"}"#);
        assert_eq!(compact("+5"), r#"{"int":"+5"}"#);
        assert_eq!(compact("-5"), r#"{"int":"-5"}"#);
        assert_eq!(compact("1.50"), r#"{"float":"1.50"}"#);
        assert_eq!(compact("1.5e3"), r#"{"float":"1.5e3"}"#);
    }

    #[test]
    fn strings_are_escaped() {
        assert_eq!(compact(r#""hi""#), r#"{"string":"hi"}"#);
        assert_eq!(compact(r#""a\"b\\c\nd\te""#), r#"{"string":"a\"b\\c\nd\te"}"#);
        assert_eq!(
            compact("\"caf\u{e9} \u{1f600}\""),
            "{\"string\":\"caf\u{e9} \u{1f600}\"}"
        );
        assert_eq!(compact(r#""\u0001""#), r#"{"string":"\u0001"}"#);
    }

    #[test]
    fn control_characters_use_the_short_escapes() {
        let mut out = String::new();
        write_string(&mut out, "\u{8}\u{c}\r\u{1f}/");
        assert_eq!(out, r#""\b\f\r\u001f/""#);
    }

    #[test]
    fn arrays_and_records() {
        assert_eq!(compact("[1, 2]"), r#"{"array":[{"nat":"1"},{"nat":"2"}]}"#);
        assert_eq!(compact("[]"), r#"{"array":[]}"#);
        assert_eq!(
            compact("{ b: 1, a: 2 }"),
            r#"{"record":{"b":{"nat":"1"},"a":{"nat":"2"}}}"#
        );
        assert_eq!(compact("{}"), r#"{"record":{}}"#);
    }

    #[test]
    fn consts_and_constructors() {
        assert_eq!(compact("red"), r#"{"const":"red"}"#);
        assert_eq!(compact("_hidden"), r#"{"const":"_hidden"}"#);
        assert_eq!(compact("Quit"), r#"{"constructor":"Quit"}"#);
        assert_eq!(compact("shapes::Circle"), r#"{"constructor":"shapes::Circle"}"#);
        assert_eq!(compact("a.b::red"), r#"{"const":"a.b::red"}"#);
    }

    #[test]
    fn constructor_bodies() {
        assert_eq!(
            compact("Move { x: 1 }"),
            r#"{"constructor":"Move","record":{"x":{"nat":"1"}}}"#
        );
        assert_eq!(compact("Move {}"), r#"{"constructor":"Move","record":{}}"#);
        assert_eq!(
            compact(r#"Write("hi")"#),
            r#"{"constructor":"Write","tuple":[{"string":"hi"}]}"#
        );
        assert_eq!(compact("Ping()"), r#"{"constructor":"Ping","tuple":[]}"#);
        assert_eq!(compact("Users[]"), r#"{"constructor":"Users","array":[]}"#);
        assert_eq!(
            compact("Users[u1, Bob]"),
            r#"{"constructor":"Users","array":[{"const":"u1"},{"constructor":"Bob"}]}"#
        );
    }

    #[test]
    fn named_arguments_follow_the_positional_ones() {
        assert_eq!(
            compact("Span(1, end: 3, start: 2)"),
            r#"{"constructor":"Span","tuple":[{"nat":"1"}],"named":{"end":{"nat":"3"},"start":{"nat":"2"}}}"#
        );
        assert_eq!(
            compact("Span(start: 2)"),
            r#"{"constructor":"Span","tuple":[],"named":{"start":{"nat":"2"}}}"#
        );
    }

    #[test]
    fn the_postfix_query_is_written_as_some() {
        let out = convert_with("5?", true, Experimental::all()).unwrap();
        assert_eq!(out, r#"{"constructor":"Some","tuple":[{"nat":"5"}]}"#);
    }

    #[test]
    fn duplicates_are_errors_at_the_second() {
        assert_eq!(
            error("{ a: 1,\n  a: 2 }"),
            "ERROR: Duplicate field a (test.values, line 2, col 3)"
        );
        assert_eq!(
            error("Move { x: 1, x: 2 }"),
            "ERROR: Duplicate field x (test.values, line 1, col 14)"
        );
        assert_eq!(
            error("Span(a: 1, a: 2)"),
            "ERROR: Argument a is given more than once in Span (test.values, line 1, col 12)"
        );
    }

    #[test]
    fn a_duplicate_deep_inside_is_found() {
        assert!(error("[1, [Move { x: 1, x: 2 }]]").contains("Duplicate field x"));
    }

    #[test]
    fn a_mixed_array_is_not_checked() {
        assert_eq!(compact(r#"[1, "x"]"#), r#"{"array":[{"nat":"1"},{"string":"x"}]}"#);
    }

    #[test]
    fn a_leaf_is_on_one_line() {
        assert_eq!(pretty("5"), "{\"nat\": \"5\"}\n");
        assert_eq!(pretty("Quit"), "{\"constructor\": \"Quit\"}\n");
        assert_eq!(pretty("[]"), "{\n  \"array\": []\n}\n");
    }

    #[test]
    fn a_container_with_a_container_is_spread_over_lines() {
        assert_eq!(
            pretty("[1, 2]"),
            "{\n  \"array\": [\n    {\"nat\": \"1\"},\n    {\"nat\": \"2\"}\n  ]\n}\n"
        );
    }

    #[test]
    fn a_nested_value_in_both_layouts() {
        let text = "Move { x: 1, tags: [a] }";
        assert_eq!(
            compact(text),
            r#"{"constructor":"Move","record":{"x":{"nat":"1"},"tags":{"array":[{"const":"a"}]}}}"#
        );
        assert_eq!(
            pretty(text),
            "{\n  \"constructor\": \"Move\",\n  \"record\": {\n    \"x\": {\"nat\": \"1\"},\n    \"tags\": {\n      \
             \"array\": [\n        {\"const\": \"a\"}\n      ]\n    }\n  }\n}\n"
        );
    }

    #[test]
    fn the_compact_form_has_no_new_line() {
        assert!(!compact("5").ends_with('\n'));
        assert!(pretty("5").ends_with("}\n"));
    }

    #[test]
    fn the_keys_of_a_constructor_are_in_order() {
        let out = compact("Span(1, end: 3)");
        let at = |key: &str| out.find(key).unwrap();
        assert!(at("constructor") < at("tuple") && at("tuple") < at("named"));
    }
}
