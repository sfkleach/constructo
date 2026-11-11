//! Conversion of values to other notations.
//!
//! Two formats exist at present: `json-exact`, which writes every kind of value in an explicit form and needs no
//! types, and `jsonl-exact`, which writes the same encoding on a line for each value. See
//! `docs/tasks/2026-11-08-convert-subcommand/convert-subcommand.md`.

use crate::ast::{Argument, Member, TaggedBody, Value, ValueKind};
use crate::diagnostics::{Diagnostic, Pos};
use crate::parser::{parse_file, Experimental, Parsed};
use crate::{syntax_diagnostic, Source};
use std::collections::HashSet;
use std::io::Write;

/// The notations that values can be converted to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// JSON in which every kind of value is written explicitly, so that the values and the text of their numbers are
    /// kept and no types are needed. Comments, layout and the spelling of string escapes are not kept.
    JsonExact,
    /// The same as `JsonExact`, with each value on a line of its own: JSON lines.
    JsonlExact,
    /// Natural JSON, in the shapes that serde writes. The types decide what is written, so the values are checked
    /// against them first.
    JsonTyped,
    /// The same as `JsonTyped`, with each value on a line of its own.
    JsonlTyped,
}

impl Format {
    /// The format with the name that is given, which is matched exactly. The error is the message to show, with no
    /// `ERROR:` in front. A bare `json` is not a format, so that nobody picks one of the two JSON strategies by
    /// accident, and `json-typed` is to come.
    pub fn from_name(name: &str) -> Result<Format, String> {
        match name {
            "json-exact" => Ok(Format::JsonExact),
            "jsonl-exact" => Ok(Format::JsonlExact),
            "json-typed" => Ok(Format::JsonTyped),
            "jsonl-typed" => Ok(Format::JsonlTyped),
            "json" => Err(
                "json is not a format; choose json-exact (every kind kept, no types), json-typed (natural, needs \
                 types), or jsonl-exact or jsonl-typed (the same, a value to a line)"
                    .to_string(),
            ),
            _ => Err(format!(
                "{} is not a format; choose json-exact, jsonl-exact, json-typed or jsonl-typed",
                name
            )),
        }
    }

    /// Whether the values are checked against types, which are then among the inputs, before they are converted.
    pub fn needs_types(self) -> bool {
        matches!(self, Format::JsonTyped | Format::JsonlTyped)
    }

    /// Whether the format is one value to a line, in which case there is no array and no layout to choose.
    pub fn is_lines(self) -> bool {
        matches!(self, Format::JsonlExact | Format::JsonlTyped)
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

/// Every value of the sources, in order, each with the name of the source that it is in. It is an error if a source has
/// a syntax error or is a types file, so every source is read before any value is converted.
fn all_values(sources: &[Source], experimental: Experimental) -> Result<Vec<(&str, Value)>, Diagnostic> {
    let mut values: Vec<(&str, Value)> = Vec::new();
    for source in sources {
        values.extend(
            values_of(source, experimental)?
                .into_iter()
                .map(|v| (source.name.as_str(), v)),
        );
    }
    Ok(values)
}

/// What can go wrong with a conversion that is written as it goes.
#[derive(Debug)]
pub enum LinesError {
    /// The inputs have an error. Lines that were converted before it have been written.
    Invalid(Vec<Diagnostic>),
    /// The output could not be written, such as when the reader of a pipe has gone.
    Write(std::io::Error),
}

/// Converts every value of the sources to a line of `json-exact`, with no white space, and writes each to `out` as soon
/// as it is converted. The sources are all read first, so an error in the syntax of any of them, or a types file, is
/// found before anything is written. After that the first error in a value ends the run, and the lines before it have
/// been written. A line that is written is complete. No values give no output.
pub fn convert_lines(sources: &[Source], options: &ConvertOptions, out: &mut dyn Write) -> Result<(), LinesError> {
    let values = all_values(sources, options.experimental).map_err(|d| LinesError::Invalid(vec![d]))?;
    for (name, value) in &values {
        let line = encode_exact(name, value)
            .map_err(|d| LinesError::Invalid(vec![d]))?
            .write(true);
        out.write_all(line.as_bytes()).map_err(LinesError::Write)?;
    }
    Ok(())
}

/// Converts the values of the sources, which are in the order that they were given, and gives the output as a whole.
/// The sources are all parsed first, and the first error, of syntax or of a type of file or of encoding, is the result,
/// so that nothing is half written. The lines formats are written as they go by `convert_lines`, and here their lines
/// are only collected, so that an error gives no output.
pub fn convert(sources: &[Source], options: &ConvertOptions) -> Result<Converted, Diagnostic> {
    if options.format.needs_types() {
        // The conversion that needs the types is `convert_typed`, which is given the layout of the projects as well.
        let message = "a typed format needs the types, so it is converted with convert_typed or convert_typed_lines";
        return Err(Diagnostic::new("", Pos { line: 1, col: 1 }, message));
    }
    if options.format.is_lines() {
        let mut output = Vec::new();
        return match convert_lines(sources, options, &mut output) {
            Ok(()) => Ok(Converted {
                output: String::from_utf8(output).expect("JSON is UTF-8"),
                warnings: Vec::new(),
            }),
            Err(LinesError::Invalid(mut diagnostics)) => Err(diagnostics.remove(0)),
            // Writing to a vector cannot fail, and this is only a defensive check.
            Err(LinesError::Write(e)) => Err(Diagnostic::new("", Pos { line: 1, col: 1 }, e.to_string())),
        };
    }
    let values = all_values(sources, options.experimental)?;
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

/// A JSON value. An object keeps its members in the order given. A number is kept as the text that is to be written,
/// which is what makes a number that is too large for a double exact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Json {
    Str(String),
    Number(String),
    Bool(bool),
    Null,
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    /// Writes the value with no white space, or when not compact, with two spaces of indentation, and ends it with a
    /// new line either way. When it is not compact, a container is written on one line if that is at most `WIDTH`
    /// characters wide, not counting the indentation, so that `{"nat": "5"}` is not five lines, and is otherwise
    /// spread over lines, with one member on each.
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

    /// Writes a value that is not a container, and says whether it was one.
    fn write_scalar(&self, out: &mut String) -> bool {
        match self {
            Json::Str(s) => write_string(out, s),
            Json::Number(text) => out.push_str(text),
            Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Json::Null => out.push_str("null"),
            Json::Array(_) | Json::Object(_) => return false,
        }
        true
    }

    fn write_compact(&self, out: &mut String) {
        if self.write_scalar(out) {
            return;
        }
        match self {
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
            _ => unreachable!("a scalar has been written"),
        }
    }

    /// The width of the value written on one line, if it is at most `budget` characters, and `None` if it is more. The
    /// measuring stops as soon as the budget is spent, so a large value costs no more to turn down than a small one.
    fn flat_width(&self, budget: usize) -> Option<usize> {
        // Adds to the width so far, and gives `None` once it is over the budget.
        fn add(spent: &mut usize, width: usize, budget: usize) -> Option<()> {
            *spent += width;
            (*spent <= budget).then_some(())
        }
        let mut spent = 0;
        match self {
            Json::Str(s) => add(&mut spent, string_width(s, budget), budget)?,
            Json::Number(text) => add(&mut spent, text.chars().count(), budget)?,
            Json::Bool(b) => add(&mut spent, if *b { 4 } else { 5 }, budget)?,
            Json::Null => add(&mut spent, 4, budget)?,
            Json::Array(items) => {
                add(&mut spent, 2, budget)?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        add(&mut spent, 2, budget)?;
                    }
                    let width = item.flat_width(budget - spent)?;
                    add(&mut spent, width, budget)?;
                }
            }
            Json::Object(members) => {
                add(&mut spent, 2, budget)?;
                for (i, (key, value)) in members.iter().enumerate() {
                    if i > 0 {
                        add(&mut spent, 2, budget)?;
                    }
                    // The key, its quotes and the `: ` after it.
                    add(&mut spent, string_width(key, budget) + 2, budget)?;
                    let width = value.flat_width(budget - spent)?;
                    add(&mut spent, width, budget)?;
                }
            }
        }
        Some(spent)
    }

    /// Writes the value, which is `depth` levels in. A value that is at most `WIDTH` wide on one line is written on one
    /// line, wherever it is, and one that is not is spread over lines. A value that is not a container, and an empty
    /// one, is always on one line.
    fn write_pretty(&self, out: &mut String, depth: usize) {
        if self.flat_width(WIDTH).is_some() || !matches!(self, Json::Array(_) | Json::Object(_)) {
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
            _ => unreachable!("only a container is spread over lines"),
        }
        out.push_str(&"  ".repeat(depth));
        out.push(close);
    }

    /// Writes the value on one line, with a space after each `:` and `,`.
    fn write_flat(&self, out: &mut String) {
        if self.write_scalar(out) {
            return;
        }
        match self {
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
            _ => unreachable!("a scalar has been written"),
        }
    }
}

/// The most characters wide that a container may be, written on one line, for the pretty layout to keep it there. The
/// indentation and the key that it follows are not counted, so the layout of a value is the same wherever it is.
const WIDTH: usize = 80;

/// The characters that a string takes when it is written, with its quotes and escapes. It stops counting once the width
/// is over `budget`, and then gives a width that is over it.
fn string_width(text: &str, budget: usize) -> usize {
    let mut width = 2;
    for c in text.chars() {
        width += match c {
            '"' | '\\' | '\n' | '\r' | '\t' | '\u{8}' | '\u{c}' => 2,
            c if (c as u32) < 0x20 => 6,
            _ => 1,
        };
        if width > budget {
            break;
        }
    }
    width
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
    fn a_container_that_is_narrow_enough_is_on_one_line_however_much_it_nests() {
        assert_eq!(pretty("Quit"), "{\"constructor\": \"Quit\"}\n");
        let circle = Json::Object(vec![(
            "Circle".to_string(),
            Json::Object(vec![("r".to_string(), Json::Number("1.0".to_string()))]),
        )]);
        assert_eq!(circle.write(false), "{\"Circle\": {\"r\": 1.0}}\n");
        assert_eq!(pretty("[1, 2]"), "{\"array\": [{\"nat\": \"1\"}, {\"nat\": \"2\"}]}\n");
        assert_eq!(
            pretty("Write(\"hi\")"),
            "{\"constructor\": \"Write\", \"tuple\": [{\"string\": \"hi\"}]}\n"
        );
    }

    #[test]
    fn an_empty_container_is_on_one_line() {
        let json = Json::Object(vec![(
            "a".to_string(),
            Json::Object(vec![("b".to_string(), Json::Array(Vec::new()))]),
        )]);
        assert_eq!(json.write(false), "{\"a\": {\"b\": []}}\n");
    }

    #[test]
    fn a_nested_value_in_both_layouts() {
        let text = "Move { x: 1, tags: [a] }";
        assert_eq!(
            compact(text),
            r#"{"constructor":"Move","record":{"x":{"nat":"1"},"tags":{"array":[{"const":"a"}]}}}"#
        );
        // The whole value is too wide for a line, so it is spread, and each member that fits is on one line.
        assert_eq!(
            pretty(text),
            "{\n  \"constructor\": \"Move\",\n  \"record\": \
             {\"x\": {\"nat\": \"1\"}, \"tags\": {\"array\": [{\"const\": \"a\"}]}}\n}\n"
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
        assert_eq!(Format::from_name("jsonl-exact"), Ok(Format::JsonlExact));
        assert_eq!(Format::from_name("json-typed"), Ok(Format::JsonTyped));
        for name in ["json", "jsonl", "JSON-Exact", "Jsonl-Exact", "Json-Typed", "", "yaml"] {
            assert!(Format::from_name(name).is_err(), "{}", name);
        }
        assert!(Format::from_name("json")
            .unwrap_err()
            .starts_with("json is not a format; choose json-exact"));
        assert!(Format::JsonTyped.needs_types() && !Format::JsonExact.needs_types());
    }

    fn lines(sources: &[Source]) -> Result<Converted, String> {
        let options = ConvertOptions {
            format: Format::JsonlExact,
            ..options(false)
        };
        convert(sources, &options).map_err(|d| d.to_string())
    }

    #[test]
    fn jsonl_has_a_compact_line_for_each_value() {
        let done = lines(&[
            source("a.values", "1; [a, B];\nMove { x: 1 }"),
            source("b.values", "\"hi\""),
        ])
        .unwrap();
        assert_eq!(
            done.output,
            "{\"nat\":\"1\"}\n{\"array\":[{\"const\":\"a\"},{\"constructor\":\"B\"}]}\n\
             {\"constructor\":\"Move\",\"record\":{\"x\":{\"nat\":\"1\"}}}\n{\"string\":\"hi\"}\n"
        );
        assert!(done.warnings.is_empty());
    }

    #[test]
    fn jsonl_ignores_the_layout_and_the_array() {
        let sources = [source("a.values", "1; 2")];
        let pretty = ConvertOptions {
            format: Format::JsonlExact,
            to_array: true,
            compact: false,
            experimental: Experimental::default(),
        };
        assert_eq!(
            convert(&sources, &pretty).unwrap().output,
            lines(&sources).unwrap().output
        );
    }

    #[test]
    fn jsonl_with_no_values_is_empty_and_not_an_error() {
        assert_eq!(lines(&[source("a.values", "")]).unwrap().output, "");
        assert_eq!(lines(&[]).unwrap().output, "");
    }

    #[test]
    fn jsonl_stops_at_the_first_error() {
        let error = lines(&[source("a.values", "1; { a: 1, a: 2 }; Span(b: 1, b: 2)")]).unwrap_err();
        assert!(error.contains("Duplicate field a"), "{}", error);
        assert!(lines(&[source("a.values", "1; ["), source("t.types", "enum E = A;")]).is_err());
    }

    #[test]
    fn numbers_booleans_and_null_are_written_as_they_are() {
        let json = Json::Object(vec![
            ("n".to_string(), Json::Number("12345678901234567890.0".to_string())),
            ("t".to_string(), Json::Bool(true)),
            ("f".to_string(), Json::Bool(false)),
            ("z".to_string(), Json::Null),
            (
                "a".to_string(),
                Json::Array(vec![Json::Number("1".to_string()), Json::Null]),
            ),
        ]);
        assert_eq!(
            json.write(true),
            "{\"n\":12345678901234567890.0,\"t\":true,\"f\":false,\"z\":null,\"a\":[1,null]}\n"
        );
        assert_eq!(
            json.write(false),
            "{\"n\": 12345678901234567890.0, \"t\": true, \"f\": false, \"z\": null, \"a\": [1, null]}\n"
        );
        let nested = Json::Array(vec![Json::Array(vec![Json::Bool(true)])]);
        assert_eq!(nested.write(false), "[[true]]\n");
    }

    /// An array that holds one string of `n` characters, which is `n + 4` characters wide on one line.
    fn wide(n: usize) -> Json {
        Json::Array(vec![Json::Str("x".repeat(n))])
    }

    #[test]
    fn a_container_is_on_one_line_if_it_is_at_most_80_wide() {
        assert_eq!(wide(76).write(false).lines().count(), 1);
        assert_eq!(wide(77).write(false).lines().count(), 3);
        assert_eq!(wide(77).write(false), format!("[\n  \"{}\"\n]\n", "x".repeat(77)));
    }

    #[test]
    fn the_indentation_and_the_key_are_not_counted() {
        // The outer array is too wide, so each inner one is indented, and is on one line though the line is longer.
        let inner = Json::Object(vec![("key".to_string(), wide(66))]);
        let outer = Json::Array(vec![inner.clone(), inner]);
        let text = outer.write(false);
        assert_eq!(text.lines().count(), 4);
        assert!(text.lines().nth(1).unwrap().len() > 80);
    }

    #[test]
    fn a_long_string_is_never_broken() {
        let json = Json::Object(vec![("s".to_string(), Json::Str("y".repeat(200)))]);
        assert_eq!(json.write(false), format!("{{\n  \"s\": \"{}\"\n}}\n", "y".repeat(200)));
    }

    #[test]
    fn the_width_is_in_characters_and_counts_the_escapes() {
        // Seventy-six characters that are not ASCII are 76 wide, which fits.
        assert_eq!(
            Json::Array(vec![Json::Str("\u{e9}".repeat(76))])
                .write(false)
                .lines()
                .count(),
            1
        );
        // Each newline is written as two characters, so 38 of them are 80 wide, and 39 are not.
        assert_eq!(
            Json::Array(vec![Json::Str("\n".repeat(38))])
                .write(false)
                .lines()
                .count(),
            1
        );
        assert_eq!(
            Json::Array(vec![Json::Str("\n".repeat(39))])
                .write(false)
                .lines()
                .count(),
            3
        );
    }

    #[test]
    fn a_very_deep_value_is_written_without_overflowing_the_stack() {
        let mut json = Json::Null;
        for _ in 0..300 {
            json = Json::Array(vec![json, Json::Str("x".repeat(100))]);
        }
        assert!(json.write(false).lines().count() > 300);
    }

    /// A writer that keeps what it is given in the chunks that it was given, and can be made to fail.
    #[derive(Default)]
    struct Sink {
        chunks: Vec<String>,
        fail_after: Option<usize>,
    }

    impl Write for Sink {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.fail_after == Some(self.chunks.len()) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "the reader has gone",
                ));
            }
            self.chunks.push(String::from_utf8(bytes.to_vec()).unwrap());
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn stream(sources: &[Source], sink: &mut Sink) -> Result<(), LinesError> {
        let options = ConvertOptions {
            format: Format::JsonlExact,
            ..options(false)
        };
        convert_lines(sources, &options, sink)
    }

    #[test]
    fn the_lines_are_written_one_at_a_time_as_they_are_converted() {
        let mut sink = Sink::default();
        stream(&[source("a.values", "1; 2"), source("b.values", "red")], &mut sink).unwrap();
        assert_eq!(
            sink.chunks,
            ["{\"nat\":\"1\"}\n", "{\"nat\":\"2\"}\n", "{\"const\":\"red\"}\n"]
        );
    }

    #[test]
    fn the_lines_before_an_error_in_a_value_have_been_written() {
        let mut sink = Sink::default();
        let result = stream(&[source("a.values", "1; { a: 1, a: 2 }; 3")], &mut sink);
        let Err(LinesError::Invalid(diagnostics)) = result else {
            panic!("an error in a value was expected");
        };
        assert!(diagnostics[0].to_string().contains("Duplicate field a"));
        assert_eq!(sink.chunks, ["{\"nat\":\"1\"}\n"]);
    }

    #[test]
    fn an_error_in_the_syntax_of_any_source_is_found_before_anything_is_written() {
        let mut sink = Sink::default();
        let result = stream(&[source("a.values", "1; 2"), source("b.values", "[1,")], &mut sink);
        assert!(matches!(result, Err(LinesError::Invalid(_))));
        assert!(sink.chunks.is_empty());
        let result = stream(&[source("a.values", "1"), source("t.types", "enum E = A;")], &mut sink);
        assert!(matches!(result, Err(LinesError::Invalid(_))));
        assert!(sink.chunks.is_empty());
    }

    #[test]
    fn a_failure_to_write_ends_the_run_and_is_reported_as_one() {
        let mut sink = Sink {
            fail_after: Some(1),
            ..Sink::default()
        };
        let result = stream(&[source("a.values", "1; 2; 3")], &mut sink);
        let Err(LinesError::Write(e)) = result else {
            panic!("a failure to write was expected");
        };
        assert_eq!(e.kind(), std::io::ErrorKind::BrokenPipe);
        assert_eq!(sink.chunks.len(), 1);
    }
}
