//! Conversion of values to other notations.
//!
//! The only format at present is `json-exact`, which writes every kind of value in an explicit form and needs no
//! types. See `docs/tasks/2026-11-08-convert-subcommand/convert-subcommand.md`.

use crate::ast::{Argument, Member, TaggedBody, Value, ValueKind};
use crate::diagnostics::{Diagnostic, Pos};
use crate::parser::{parse_file, Experimental, Parsed};
use crate::{syntax_diagnostic, Source};
use std::collections::HashSet;

/// The notations that values can be converted to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// JSON in which every kind of value is written explicitly, so that nothing is lost and no types are needed.
    JsonExact,
}

impl Format {
    /// The format with the name that is given, which is matched exactly. The error is the message to show, with no
    /// `ERROR:` in front. A bare `json` is not a format, so that nobody picks one of the two JSON strategies by
    /// accident, and `json-typed` is to come.
    pub fn from_name(name: &str) -> Result<Format, String> {
        match name {
            "json-exact" => Ok(Format::JsonExact),
            "json" => Err(
                "json is not a format; choose json-exact (every kind kept, no types); json-typed (natural, \
                           needs types) is not available yet"
                    .to_string(),
            ),
            "json-typed" => Err("json-typed is not available yet; choose json-exact".to_string()),
            _ => Err(format!("{} is not a format; choose json-exact", name)),
        }
    }
}

/// The choices of one conversion.
#[derive(Debug, Clone, Copy)]
pub struct ConvertOptions {
    pub format: Format,
    /// Convert every value, as an array, and not just the first.
    pub to_array: bool,
    /// Write no white space.
    pub compact: bool,
    pub experimental: Experimental,
}

/// The result of a conversion: the text to write, and the warnings to report. The exit code is 0 for a result.
#[derive(Debug)]
pub struct Converted {
    pub output: String,
    pub warnings: Vec<Diagnostic>,
}

/// The values of a source. It is an error if the source has a syntax error or is a types file. A file with nothing in
/// it, which the parser takes for a types file, has no values.
pub fn values_of(source: &Source, experimental: Experimental) -> Result<Vec<Value>, Diagnostic> {
    match parse_file(&source.text, experimental).1 {
        Err(e) => Err(syntax_diagnostic(source, e)),
        Ok(Parsed::Types(file)) if file.enums.is_empty() && file.consts.is_empty() => Ok(Vec::new()),
        Ok(Parsed::Types(_)) => {
            let message = format!("{} is a types file; convert reads values files", source.name);
            Err(Diagnostic::new(&source.name, Pos { line: 1, col: 1 }, message))
        }
        Ok(Parsed::Values(found)) => Ok(found),
    }
}

/// Converts the values of the sources, which are in the order that they were given. The sources are all parsed first,
/// and the first error, of syntax or of a type of file or of encoding, is the result, so that nothing is half written.
pub fn convert(sources: &[Source], options: &ConvertOptions) -> Result<Converted, Diagnostic> {
    let mut values: Vec<(&str, Value)> = Vec::new();
    for source in sources {
        values.extend(
            values_of(source, options.experimental)?
                .into_iter()
                .map(|v| (source.name.as_str(), v)),
        );
    }
    let Format::JsonExact = options.format;
    let mut warnings = Vec::new();
    let json = if options.to_array {
        let mut items = Vec::new();
        for (name, value) in &values {
            items.push(encode_exact(name, value)?);
        }
        Json::Array(items)
    } else {
        let Some((name, first)) = values.first() else {
            let name = sources.first().map_or("", |s| s.name.as_str());
            return Err(Diagnostic::new(name, Pos { line: 1, col: 1 }, "No value to convert"));
        };
        if let Some((later, second)) = values.get(1) {
            let message = "More values follow; only the first was converted, so use --to-array for all";
            warnings.push(Diagnostic::warning(later, second.pos, message));
        }
        encode_exact(name, first)?
    };
    Ok(Converted {
        output: json.write(options.compact),
        warnings,
    })
}

/// A JSON value, of the three kinds that `json-exact` needs. An object keeps its members in the order given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Json {
    Str(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    /// Writes the value with no white space, or when not compact, with two spaces of indentation, and ends it with a
    /// new line either way. A container that holds no other container is written on one line even then, so that a
    /// number is `{"nat": "5"}` and not five lines.
    pub fn write(&self, compact: bool) -> String {
        let mut out = String::new();
        if compact {
            self.write_compact(&mut out);
        } else {
            self.write_pretty(&mut out, 0);
        }
        out.push('\n');
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

    /// Whether the value is a container that holds another container that is not empty, which is what makes it span
    /// several lines. An empty container is as short as a string, so it does not count, and `{"array": []}` stays on
    /// one line.
    fn is_nested(&self) -> bool {
        match self {
            Json::Str(_) => false,
            Json::Array(items) => items.iter().any(Json::is_full),
            Json::Object(members) => members.iter().any(|(_, value)| value.is_full()),
        }
    }

    fn is_full(&self) -> bool {
        match self {
            Json::Str(_) => false,
            Json::Array(items) => !items.is_empty(),
            Json::Object(members) => !members.is_empty(),
        }
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
        let out = convert_with(text, true, Experimental::default()).expect("the value should convert");
        out.strip_suffix('\n')
            .expect("the output ends with a new line")
            .to_string()
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
        let out = convert_with("5?", true, Experimental::all())
            .unwrap()
            .trim_end()
            .to_string();
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
    fn a_leaf_or_an_empty_container_is_on_one_line() {
        assert_eq!(pretty("5"), "{\"nat\": \"5\"}\n");
        assert_eq!(pretty("Quit"), "{\"constructor\": \"Quit\"}\n");
        assert_eq!(pretty("[]"), "{\"array\": []}\n");
        assert_eq!(pretty("Ping()"), "{\"constructor\": \"Ping\", \"tuple\": []}\n");
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
        assert_eq!(
            convert_with("5", true, Experimental::default()).unwrap(),
            "{\"nat\":\"5\"}\n"
        );
        assert!(pretty("5").ends_with("}\n"));
    }

    #[test]
    fn the_keys_of_a_constructor_are_in_order() {
        let out = compact("Span(1, end: 3)");
        let at = |key: &str| out.find(key).unwrap();
        assert!(at("constructor") < at("tuple") && at("tuple") < at("named"));
    }

    fn source(name: &str, text: &str) -> Source {
        Source {
            name: name.to_string(),
            text: text.to_string(),
        }
    }

    fn options(to_array: bool) -> ConvertOptions {
        ConvertOptions {
            format: Format::JsonExact,
            to_array,
            compact: true,
            experimental: Experimental::default(),
        }
    }

    fn run(sources: &[Source], to_array: bool) -> Result<Converted, String> {
        convert(sources, &options(to_array)).map_err(|d| d.to_string())
    }

    #[test]
    fn the_first_value_is_converted_and_the_second_is_warned_of() {
        let done = run(&[source("a.values", "1;\n2;\n3")], false).unwrap();
        assert_eq!(done.output.trim_end(), "{\"nat\":\"1\"}");
        let warnings: Vec<String> = done.warnings.iter().map(|w| w.to_string()).collect();
        assert_eq!(
            warnings,
            [
                "WARNING: More values follow; only the first was converted, so use --to-array for all \
              (a.values, line 2, col 1)"
            ]
        );
    }

    #[test]
    fn a_single_value_has_no_warning() {
        assert!(run(&[source("a.values", "1")], false).unwrap().warnings.is_empty());
    }

    #[test]
    fn an_empty_input_is_skipped_and_the_warning_may_be_in_a_later_input() {
        let sources = [source("a.values", ""), source("b.values", "7"), source("c.values", "8")];
        let done = run(&sources, false).unwrap();
        assert_eq!(done.output.trim_end(), "{\"nat\":\"7\"}");
        assert_eq!(done.warnings.len(), 1);
        assert!(done.warnings[0].to_string().ends_with("(c.values, line 1, col 1)"));
    }

    #[test]
    fn to_array_takes_every_value_of_every_input_in_order() {
        let sources = [
            source("a.values", "1; 2"),
            source("b.values", ""),
            source("c.values", "red"),
        ];
        let done = run(&sources, true).unwrap();
        assert_eq!(done.output.trim_end(), r#"[{"nat":"1"},{"nat":"2"},{"const":"red"}]"#);
        assert!(done.warnings.is_empty());
    }

    #[test]
    fn to_array_with_no_values_is_an_empty_array() {
        assert_eq!(run(&[source("a.values", "")], true).unwrap().output.trim_end(), "[]");
        assert_eq!(run(&[], true).unwrap().output.trim_end(), "[]");
    }

    #[test]
    fn no_value_is_an_error_naming_the_first_input() {
        let sources = [source("a.values", ""), source("b.values", "")];
        assert_eq!(
            run(&sources, false).unwrap_err(),
            "ERROR: No value to convert (a.values, line 1, col 1)"
        );
    }

    #[test]
    fn a_types_file_is_an_error() {
        let sources = [source("ok.values", "1"), source("t.types", "enum E = A;")];
        assert_eq!(
            run(&sources, false).unwrap_err(),
            "ERROR: t.types is a types file; convert reads values files (t.types, line 1, col 1)"
        );
    }

    #[test]
    fn a_file_with_only_comments_has_no_values() {
        let sources = [source("e.values", "// Nothing here.\n")];
        assert_eq!(run(&sources, true).unwrap().output.trim_end(), "[]");
        assert!(run(&sources, false).is_err());
    }

    #[test]
    fn a_syntax_error_is_the_result() {
        let error = run(&[source("a.values", "1; 2"), source("b.values", "[1,")], false).unwrap_err();
        assert!(error.starts_with("ERROR: "), "{}", error);
        assert!(error.contains("b.values"), "{}", error);
    }

    #[test]
    fn without_to_array_only_the_first_value_is_encoded() {
        // The duplicate is in the second value, which is not converted, so it is not found.
        assert!(run(&[source("a.values", "1; { a: 1, a: 2 }")], false).is_ok());
        assert!(run(&[source("a.values", "1; { a: 1, a: 2 }")], true).is_err());
    }

    #[test]
    fn the_formats_are_matched_exactly() {
        assert_eq!(Format::from_name("json-exact"), Ok(Format::JsonExact));
        for name in ["json", "JSON-Exact", "json-typed", "", "yaml"] {
            assert!(Format::from_name(name).is_err(), "{}", name);
        }
        assert!(Format::from_name("json")
            .unwrap_err()
            .starts_with("json is not a format; choose json-exact"));
    }
}
