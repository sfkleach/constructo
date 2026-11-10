//! The reader of `json-exact`: it turns the tree of a JSON document, as `json.rs` parses it, back into values.
//!
//! The reading is strict. An object has exactly the keys that `convert` writes, in any order, and a number is the
//! text of a literal of the kind that its key names. What is a name, a literal or a reserved word is not decided
//! here: the text is given to the real parser, so a change to the grammar changes the reader too.

use crate::ast::{Argument, Member, TaggedBody, Value, ValueKind};
use crate::diagnostics::{Diagnostic, Pos};
use crate::json::{Kind, Member as JsonMember, Node};
use crate::parser::{parse_values_with, Experimental};

/// The keys that say what kind of value an object is, when it is not a constructor.
const KINDS: [&str; 7] = ["nat", "int", "float", "string", "array", "record", "const"];

/// Reads one value from a node, which must be an object. `file` is the name that diagnostics give. Each part of the
/// value has the position of the node that it came from.
pub fn value_from_exact(node: &Node, file: &str) -> Result<Value, Diagnostic> {
    Reader { file }.value(node)
}

struct Reader<'a> {
    file: &'a str,
}

impl Reader<'_> {
    fn fail(&self, pos: Pos, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(self.file, pos, message.into())
    }

    fn value(&self, node: &Node) -> Result<Value, Diagnostic> {
        let Kind::Object(members) = &node.kind else {
            return Err(self.fail(node.pos, "Expected an object for a value"));
        };
        self.no_repeats(members)?;
        let pos = node.pos;
        if let Some(name) = members.iter().find(|m| m.key == "constructor") {
            return self.constructor(pos, name, members);
        }
        let Some(first) = members.first() else {
            return Err(self.fail(
                pos,
                "Expected a key that says what kind of value this is, as nat or constructor",
            ));
        };
        if !KINDS.contains(&first.key.as_str()) {
            let mut e = self.fail(first.key_pos, format!("Unknown key \"{}\"", first.key));
            if members.iter().all(|m| !KINDS.contains(&m.key.as_str())) {
                e = e.with_note(
                    "a json-exact value has a key such as nat, string or constructor; was this written with \
                     --to=json-typed?",
                );
            }
            return Err(e);
        }
        if let Some(extra) = members.get(1) {
            let message = format!(
                "Unexpected key \"{}\"; a value has one key, which says its kind",
                extra.key
            );
            return Err(self.fail(extra.key_pos, message));
        }
        let kind = match first.key.as_str() {
            "nat" => ValueKind::Nat(self.number(first, "nat")?),
            "int" => ValueKind::Int(self.number(first, "int")?),
            "float" => ValueKind::Float(self.number(first, "float")?),
            "string" => ValueKind::Str(self.string(&first.value, "string")?),
            "array" => ValueKind::Array(self.values(&first.value, "array")?),
            "record" => ValueKind::Record(self.members(&first.value, "record")?),
            _ => {
                let name = self.string(&first.value, "const")?;
                self.name(&first.value, &name, Names::Const)?;
                ValueKind::Tagged { name, body: None }
            }
        };
        Ok(Value { pos, kind })
    }

    fn no_repeats(&self, members: &[JsonMember]) -> Result<(), Diagnostic> {
        for (i, member) in members.iter().enumerate() {
            if members[..i].iter().any(|earlier| earlier.key == member.key) {
                return Err(self.fail(member.key_pos, format!("Repeated key \"{}\"", member.key)));
            }
        }
        Ok(())
    }

    fn constructor(&self, pos: Pos, name: &JsonMember, members: &[JsonMember]) -> Result<Value, Diagnostic> {
        let text = self.string(&name.value, "constructor")?;
        self.name(&name.value, &text, Names::Constructor)?;
        let mut body = None;
        let mut named = None;
        let mut tuple_seen = false;
        for member in members.iter().filter(|m| m.key != "constructor") {
            if matches!(member.key.as_str(), "record" | "tuple" | "array") && body.is_some() {
                return Err(self.fail(member.key_pos, "A constructor has only one of record, tuple and array"));
            }
            match member.key.as_str() {
                "record" => body = Some(TaggedBody::Record(self.members(&member.value, "record")?)),
                "array" => body = Some(TaggedBody::Array(self.values(&member.value, "array")?)),
                "tuple" => {
                    tuple_seen = true;
                    body = Some(TaggedBody::Tuple(Vec::new()));
                }
                "named" => named = Some(member),
                other => return Err(self.fail(member.key_pos, format!("Unknown key \"{other}\" in a constructor"))),
            }
        }
        if let Some(named) = named {
            if !tuple_seen {
                return Err(self.fail(named.key_pos, "\"named\" needs a \"tuple\" to go with it"));
            }
        }
        if tuple_seen {
            let tuple = members.iter().find(|m| m.key == "tuple").expect("The tuple was seen.");
            let mut arguments = Vec::new();
            for node in self.array_of(&tuple.value, "tuple")? {
                arguments.push(Argument {
                    name: None,
                    pos: node.pos,
                    value: self.value(node)?,
                });
            }
            if let Some(named) = named {
                for member in self.object_of(&named.value, "named")? {
                    self.name(&member.value, &member.key, Names::Field)
                        .map_err(|_| self.bad_field(member))?;
                    arguments.push(Argument {
                        name: Some(member.key.clone()),
                        pos: member.key_pos,
                        value: self.value(&member.value)?,
                    });
                }
            }
            body = Some(TaggedBody::Tuple(arguments));
        }
        Ok(Value {
            pos,
            kind: ValueKind::Tagged { name: text, body },
        })
    }

    fn bad_field(&self, member: &JsonMember) -> Diagnostic {
        self.fail(
            member.key_pos,
            format!("\"{}\" is not a name that a field can have", member.key),
        )
    }

    fn array_of<'n>(&self, node: &'n Node, key: &str) -> Result<&'n [Node], Diagnostic> {
        match &node.kind {
            Kind::Array(items) => Ok(items),
            _ => Err(self.fail(node.pos, format!("Expected an array for \"{key}\""))),
        }
    }

    fn object_of<'n>(&self, node: &'n Node, key: &str) -> Result<&'n [JsonMember], Diagnostic> {
        match &node.kind {
            Kind::Object(members) => {
                self.no_repeats(members)?;
                Ok(members)
            }
            _ => Err(self.fail(node.pos, format!("Expected an object for \"{key}\""))),
        }
    }

    fn values(&self, node: &Node, key: &str) -> Result<Vec<Value>, Diagnostic> {
        self.array_of(node, key)?.iter().map(|n| self.value(n)).collect()
    }

    fn members(&self, node: &Node, key: &str) -> Result<Vec<Member>, Diagnostic> {
        let mut out = Vec::new();
        for member in self.object_of(node, key)? {
            self.name(&member.value, &member.key, Names::Field)
                .map_err(|_| self.bad_field(member))?;
            out.push(Member {
                name: member.key.clone(),
                pos: member.key_pos,
                value: self.value(&member.value)?,
            });
        }
        Ok(out)
    }

    fn string(&self, node: &Node, key: &str) -> Result<String, Diagnostic> {
        match &node.kind {
            Kind::Str(text) => Ok(text.clone()),
            _ => Err(self.fail(node.pos, format!("Expected a string for \"{key}\""))),
        }
    }

    /// The text of a number, which has to be a string that reads back as a literal of the kind that the key names.
    fn number(&self, member: &JsonMember, kind: &str) -> Result<String, Diagnostic> {
        let node = &member.value;
        if let Kind::Number(_) = node.kind {
            let message = format!("Expected a string for \"{kind}\"; a number is written as a string, as \"5\"");
            return Err(self.fail(node.pos, message));
        }
        let text = self.string(node, kind)?;
        let read = parse_values_with(&format!("{text};"), Experimental::default());
        let same = match read.as_deref() {
            Ok([Value { kind: found, .. }]) => match (kind, found) {
                ("nat", ValueKind::Nat(t)) | ("int", ValueKind::Int(t)) | ("float", ValueKind::Float(t)) => *t == text,
                _ => false,
            },
            _ => false,
        };
        if same {
            Ok(text)
        } else {
            Err(self.fail(
                node.pos,
                format!(
                    "The text \"{text}\" is not {} {kind}",
                    if kind == "int" { "an" } else { "a" }
                ),
            ))
        }
    }

    /// Checks that the text is a name of the kind, by giving it to the parser.
    fn name(&self, node: &Node, text: &str, names: Names) -> Result<(), Diagnostic> {
        let ok = match names {
            Names::Field => matches!(
                parse_values_with(&format!("{{ {text}: 0 }};"), Experimental::default()).as_deref(),
                Ok([Value { kind: ValueKind::Record(ms), .. }]) if ms.len() == 1 && ms[0].name == text
            ),
            Names::Const | Names::Constructor => {
                match parse_values_with(&format!("{text};"), Experimental::default()).as_deref() {
                    Ok(
                        [Value {
                            kind: ValueKind::Tagged { name, body: None },
                            ..
                        }],
                    ) if name == text => {
                        let last = name.rsplit("::").next().unwrap_or(name);
                        let lower = last.chars().next().is_some_and(|c| c.is_lowercase() || c == '_');
                        lower == (names == Names::Const)
                    }
                    _ => false,
                }
            }
        };
        if ok {
            Ok(())
        } else {
            let what = match names {
                Names::Field => "a field name",
                Names::Const => "the name of a const",
                Names::Constructor => "the name of a constructor",
            };
            Err(self.fail(node.pos, format!("\"{text}\" is not {what}")))
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Names {
    Field,
    Const,
    Constructor,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convert::encode_exact;
    use crate::json::parse_document;
    use crate::values_text::bare;

    fn read(json: &str) -> Result<Value, Diagnostic> {
        value_from_exact(&parse_document(json).unwrap(), "t.json")
    }

    fn fails(json: &str) -> String {
        let e = read(json).unwrap_err();
        e.to_string()
    }

    fn text(value: &Value) -> String {
        crate::values_text::render(value, true)
    }

    fn reads_as(json: &str, expected: &str) {
        let value = read(json).unwrap_or_else(|e| panic!("{json}: {e}"));
        assert_eq!(text(&value), format!("{expected};\n"), "{json}");
    }

    #[test]
    fn every_kind() {
        reads_as(r#"{"nat": "5"}"#, "5");
        reads_as(r#"{"int": "+5"}"#, "+5");
        reads_as(r#"{"int": "-5"}"#, "-5");
        reads_as(r#"{"float": "1.50"}"#, "1.50");
        reads_as(r#"{"string": "a\nb"}"#, r#""a\nb""#);
        reads_as(r#"{"array": [{"nat": "1"}, {"string": "x"}]}"#, r#"[1, "x"]"#);
        reads_as(r#"{"array": []}"#, "[]");
        reads_as(
            r#"{"record": {"b": {"nat": "1"}, "a": {"nat": "2"}}}"#,
            "{ b: 1, a: 2 }",
        );
        reads_as(r#"{"record": {}}"#, "{}");
        reads_as(r#"{"const": "red"}"#, "red");
        reads_as(r#"{"const": "a.b::_x"}"#, "a.b::_x");
    }

    #[test]
    fn every_constructor() {
        reads_as(r#"{"constructor": "Dot"}"#, "Dot");
        reads_as(r#"{"constructor": "a.b::Dot"}"#, "a.b::Dot");
        reads_as(
            r#"{"constructor": "Move", "record": {"x": {"nat": "1"}}}"#,
            "Move { x: 1 }",
        );
        reads_as(r#"{"constructor": "Move", "record": {}}"#, "Move {}");
        reads_as(
            r#"{"constructor": "Pair", "tuple": [{"nat": "1"}, {"nat": "2"}]}"#,
            "Pair(1, 2)",
        );
        reads_as(r#"{"constructor": "Ping", "tuple": []}"#, "Ping()");
        reads_as(
            r#"{"constructor": "Span", "tuple": [{"nat": "1"}], "named": {"end": {"nat": "3"}}}"#,
            "Span(1, end: 3)",
        );
        reads_as(
            r#"{"constructor": "Span", "tuple": [], "named": {"end": {"nat": "3"}}}"#,
            "Span(end: 3)",
        );
        reads_as(r#"{"constructor": "Items", "array": [{"nat": "1"}]}"#, "Items[1]");
        reads_as(r#"{"constructor": "Users", "array": []}"#, "Users[]");
        // The order of the keys does not matter.
        reads_as(r#"{"tuple": [{"nat": "1"}], "constructor": "Box"}"#, "Box(1)");
    }

    #[test]
    fn positions_are_those_of_the_nodes() {
        let json = "{\n  \"constructor\": \"Span\",\n  \"tuple\": [{\"nat\": \"1\"}],\n  \
                    \"named\": {\"e\": {\"nat\": \"3\"}}\n}";
        let value = read(json).unwrap();
        assert_eq!(value.pos, Pos { line: 1, col: 1 });
        let ValueKind::Tagged {
            body: Some(TaggedBody::Tuple(args)),
            ..
        } = value.kind
        else {
            panic!()
        };
        assert_eq!(args[0].pos, Pos { line: 3, col: 13 });
        assert_eq!(args[1].pos, Pos { line: 4, col: 13 });
        assert_eq!(args[1].value.pos, Pos { line: 4, col: 18 });
    }

    #[test]
    fn keys_are_strict() {
        assert!(fails(r#"{"nat": "5", "nat": "6"}"#).contains("Repeated key \"nat\" (t.json, line 1, col 14)"));
        assert!(fails("{}").contains("Expected a key that says what kind of value this is"));
        assert!(fails(r#"{"nat": "5", "int": "6"}"#).contains("Unexpected key \"int\""));
        assert!(fails(r#"{"nat": "5", "x": 1}"#).contains("Unexpected key \"x\""));
        assert!(fails(r#"{"constructor": "A", "x": []}"#).contains("Unknown key \"x\" in a constructor"));
        assert!(
            fails(r#"{"constructor": "A", "record": {}, "tuple": []}"#).contains("only one of record, tuple and array")
        );
        assert!(fails(r#"{"constructor": "A", "array": [], "record": {}}"#).contains("(t.json, line 1, col 35)"));
        assert!(fails(r#"{"constructor": "A", "named": {}}"#).contains("\"named\" needs a \"tuple\""));
        assert!(fails(r#"{"constructor": "A", "tuple": [], "tuple": []}"#).contains("Repeated key \"tuple\""));
        assert!(fails(r#"{"record": {"a": {"nat": "1"}, "a": {"nat": "2"}}}"#).contains("Repeated key \"a\""));
    }

    #[test]
    fn a_typed_object_gets_the_hint() {
        let message = fails(r#"{"Circle": {"r": 1.5}}"#);
        assert!(
            message.contains("Unknown key \"Circle\" (t.json, line 1, col 2)"),
            "{message}"
        );
        assert!(message.contains("--to=json-typed"), "{message}");
        // With a known key present the unknown one is not a typed shape, so there is no hint.
        assert!(!fails(r#"{"nat": "1", "x": "2"}"#).contains("json-typed"));
    }

    #[test]
    fn wrong_json_for_a_part() {
        assert!(fails("5").contains("Expected an object for a value (t.json, line 1, col 1)"));
        assert!(fails("[]").contains("Expected an object for a value"));
        assert!(fails(r#"{"array": [1]}"#).contains("Expected an object for a value (t.json, line 1, col 12)"));
        assert!(fails(r#"{"array": {}}"#).contains("Expected an array for \"array\""));
        assert!(fails(r#"{"record": []}"#).contains("Expected an object for \"record\""));
        assert!(fails(r#"{"string": 1}"#).contains("Expected a string for \"string\""));
        assert!(fails(r#"{"const": null}"#).contains("Expected a string for \"const\""));
        assert!(fails(r#"{"constructor": ["A"]}"#).contains("Expected a string for \"constructor\""));
        assert!(fails(r#"{"constructor": "A", "tuple": {}}"#).contains("Expected an array for \"tuple\""));
        let message = fails(r#"{"constructor": "A", "tuple": [], "named": []}"#);
        assert!(message.contains("Expected an object for \"named\""));
    }

    #[test]
    fn numbers_are_strings_of_the_right_kind() {
        let message = fails(r#"{"nat": 5}"#);
        assert!(
            message.contains("Expected a string for \"nat\"; a number is written as a string"),
            "{message}"
        );
        assert!(fails(r#"{"nat": "+5"}"#).contains("The text \"+5\" is not a nat"));
        assert!(fails(r#"{"nat": "-5"}"#).contains("is not a nat"));
        assert!(
            fails(r#"{"int": "5"}"#).contains("is not a int") || fails(r#"{"int": "5"}"#).contains("is not an int")
        );
        assert!(fails(r#"{"float": "5"}"#).contains("is not a float"));
        assert!(fails(r#"{"nat": "5 6"}"#).contains("is not a nat"));
        assert!(fails(r#"{"nat": " 5"}"#).contains("is not a nat"));
        assert!(fails(r#"{"nat": "5 // c"}"#).contains("is not a nat"));
        assert!(fails(r#"{"nat": ""}"#).contains("is not a nat"));
        assert!(fails(r#"{"nat": "x"}"#).contains("is not a nat"));
    }

    #[test]
    fn names_are_checked_by_the_parser() {
        assert!(fails(r#"{"const": "Red"}"#).contains("\"Red\" is not the name of a const (t.json, line 1, col 11)"));
        assert!(fails(r#"{"constructor": "red"}"#).contains("\"red\" is not the name of a constructor"));
        assert!(fails(r#"{"constructor": "a b"}"#).contains("is not the name of a constructor"));
        assert!(fails(r#"{"constructor": ""}"#).contains("is not the name of a constructor"));
        assert!(fails(r#"{"const": "enum"}"#).contains("is not the name of a const"));
        assert!(fails(r#"{"const": "a;b"}"#).contains("is not the name of a const"));
        assert!(fails(r#"{"record": {"a b": {"nat": "1"}}}"#).contains("\"a b\" is not a name that a field can have"));
        assert!(fails(r#"{"record": {"": {"nat": "1"}}}"#).contains("is not a name that a field can have"));
        assert!(
            fails(r#"{"constructor": "A", "tuple": [], "named": {"1": {"nat": "1"}}}"#)
                .contains("is not a name that a field can have")
        );
    }

    #[test]
    fn an_error_deep_in_a_value_is_where_it_is() {
        let message = fails("{\"array\": [\n  {\"nat\": \"1\"},\n  {\"nat\": \"x\"}\n]}");
        assert!(message.contains("(t.json, line 3, col 11)"), "{message}");
    }

    #[test]
    fn what_convert_writes_is_read_back() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("syntax/examples/convert");
        let mut seen = 0;
        let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
        files.sort();
        for path in files.iter().filter(|p| p.extension().is_some_and(|e| e == "values")) {
            let source = std::fs::read_to_string(path).unwrap();
            let Ok(values) = parse_values_with(&source, Experimental::default()) else {
                continue;
            };
            for value in values {
                // A value that `convert` refuses, as one with a duplicate field, is not part of the round trip.
                let Ok(json) = encode_exact("t", &value) else { continue };
                for compact in [true, false] {
                    let written = json.write(compact);
                    let node = parse_document(&written).unwrap();
                    let back = value_from_exact(&node, "t.json").unwrap_or_else(|e| panic!("{written}: {e}"));
                    assert_eq!(bare(&back), bare(&value), "{}: {written}", path.display());
                    seen += 1;
                }
            }
        }
        assert!(seen > 20, "{seen}");
    }
}
