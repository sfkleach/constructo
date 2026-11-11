//! The reader of `xml`: it turns the tree of a document, as `xml_tree.rs` parses it, back into values.
//!
//! The reading is strict. An element has exactly the attributes and the children that `convert --to=xml` writes, in
//! the order that it writes them, and the text of a number or a name is what the real lexer and parser take for one.
//! What is a name or a literal is not decided here, as in `exact.rs`, so a change to the grammar changes the reader
//! too.
//! See `docs/tasks/2026-11-08-convert-subcommand/to_xml.md`.

use crate::ast::{Argument, Member, TaggedBody, Value, ValueKind};
use crate::diagnostics::{Diagnostic, Pos};
use crate::exact::{is_name, number_kind, Names};
use crate::xml_tree::Element;

/// The elements that are values, for the messages.
const KINDS: &str = "nat, int, float, string, array, record, const or constructor";

/// Reads the values of a document, whose root must be `values`. `file` is the name that diagnostics give. The values
/// are read one at a time as `each` is called with them, so that the first problem is found when its value is
/// reached, and the values before it have been given. The root and its attributes are checked before the first
/// value.
pub fn values_from_xml<E>(
    root: &Element,
    file: &str,
    mut each: impl FnMut(Value) -> Result<(), E>,
) -> Result<(), ReadError<E>> {
    let reader = Reader { file };
    reader.root(root).map_err(ReadError::Invalid)?;
    for child in &root.children {
        let value = reader.value(child).map_err(ReadError::Invalid)?;
        each(value).map_err(ReadError::Caller)?;
    }
    Ok(())
}

/// A problem with a document, or the failure of what was given the values.
#[derive(Debug)]
pub enum ReadError<E> {
    Invalid(Diagnostic),
    Caller(E),
}

/// Reads one value from an element. This is for the tests and for a caller that has a value alone.
pub fn value_from_xml(element: &Element, file: &str) -> Result<Value, Diagnostic> {
    Reader { file }.value(element)
}

struct Reader<'a> {
    file: &'a str,
}

impl Reader<'_> {
    fn fail(&self, pos: Pos, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(self.file, pos, message.into())
    }

    fn root(&self, root: &Element) -> Result<(), Diagnostic> {
        if root.name != "values" {
            let mut e = self.fail(root.pos, format!("Expected the root element values, not {}", root.name));
            if is_value_element(&root.name) {
                e = e.with_note("a document is a list of values, so wrap the value in <values>");
            }
            return Err(e);
        }
        self.no_attributes(root)
    }

    fn no_attributes(&self, element: &Element) -> Result<(), Diagnostic> {
        match element.attributes.first() {
            Some(attribute) => Err(self.fail(
                attribute.pos,
                format!(
                    "Unexpected attribute {} on {}; it has none",
                    attribute.name, element.name
                ),
            )),
            None => Ok(()),
        }
    }

    /// The text of the one attribute that an element has, which is named `key`.
    fn attribute<'e>(&self, element: &'e Element, key: &str) -> Result<(&'e str, Pos), Diagnostic> {
        if let Some(other) = element.attributes.iter().find(|a| a.name != key) {
            let message = format!(
                "Unexpected attribute {} on {}; it has {}",
                other.name, element.name, key
            );
            return Err(self.fail(other.pos, message));
        }
        match element.attributes.first() {
            Some(attribute) => Ok((&attribute.value, attribute.value_pos)),
            None => Err(self.fail(element.pos, format!("Expected a {} attribute on {}", key, element.name))),
        }
    }

    fn no_content(&self, element: &Element) -> Result<(), Diagnostic> {
        match element.children.first() {
            Some(child) => Err(self.fail(
                child.pos,
                format!(
                    "A {} has no content, but this has the element {}",
                    element.name, child.name
                ),
            )),
            None => Ok(()),
        }
    }

    fn value(&self, element: &Element) -> Result<Value, Diagnostic> {
        let pos = element.pos;
        let kind = match element.name.as_str() {
            "nat" | "int" | "float" => {
                let (text, text_pos) = self.attribute(element, "value")?;
                self.no_content(element)?;
                self.number(&element.name, text, text_pos)?;
                match element.name.as_str() {
                    "nat" => ValueKind::Nat(text.to_string()),
                    "int" => ValueKind::Int(text.to_string()),
                    _ => ValueKind::Float(text.to_string()),
                }
            }
            "string" => {
                let (text, _) = self.attribute(element, "value")?;
                self.no_content(element)?;
                ValueKind::Str(text.to_string())
            }
            "const" => {
                let (name, name_pos) = self.attribute(element, "name")?;
                self.no_content(element)?;
                self.name(name, name_pos, Names::Const)?;
                ValueKind::Tagged {
                    name: name.to_string(),
                    body: None,
                }
            }
            "array" => {
                self.no_attributes(element)?;
                ValueKind::Array(self.values(&element.children)?)
            }
            "record" => {
                self.no_attributes(element)?;
                ValueKind::Record(self.members(&element.children)?)
            }
            "constructor" => return self.constructor(element),
            other => {
                let message = if is_value_element(other) || matches!(other, "field" | "tuple" | "named" | "values") {
                    format!("The element {other} is not a value here; a value is {KINDS}")
                } else {
                    format!("Unknown element {other}; a value is {KINDS}")
                };
                return Err(self.fail(pos, message));
            }
        };
        Ok(Value { pos, kind })
    }

    fn values(&self, elements: &[Element]) -> Result<Vec<Value>, Diagnostic> {
        elements.iter().map(|e| self.value(e)).collect()
    }

    /// Checks the text of a number, which has to read back as a literal of the kind that the element names. Otherwise
    /// `<int value="5"/>` would be written as `5` and read back as a `Nat`.
    fn number(&self, kind: &str, text: &str, pos: Pos) -> Result<(), Diagnostic> {
        let found = number_kind(text);
        if found == Some(kind) {
            return Ok(());
        }
        let article = if kind == "int" { "an" } else { "a" };
        let mut e = self.fail(pos, format!("The text \"{text}\" is not {article} {kind}"));
        if let Some(found) = found {
            e = e.with_note(format!(
                "it reads as a {found}, so the element is <{found} value=\"{text}\"/>"
            ));
        }
        Err(e)
    }

    fn name(&self, text: &str, pos: Pos, names: Names) -> Result<(), Diagnostic> {
        if is_name(text, names) {
            return Ok(());
        }
        let what = match names {
            Names::Field => "a field name",
            Names::Const => "the name of a const",
            Names::Constructor => "the name of a constructor",
        };
        Err(self.fail(pos, format!("\"{text}\" is not {what}")))
    }

    /// The name and the one value of a `field` element.
    fn field<'e>(&self, element: &'e Element) -> Result<(&'e str, &'e Element), Diagnostic> {
        if element.name != "field" {
            let message = format!("Expected a field element here, not {}", element.name);
            return Err(self.fail(element.pos, message));
        }
        let (name, name_pos) = self.attribute(element, "name")?;
        self.name(name, name_pos, Names::Field)
            .map_err(|_| self.fail(name_pos, format!("\"{name}\" is not a name that a field can have")))?;
        match element.children.as_slice() {
            [value] => Ok((name, value)),
            _ => Err(self.fail(element.pos, "A field holds exactly one value")),
        }
    }

    /// The `field` elements as the members of a record. A name that is given twice is an error at the second.
    fn members(&self, elements: &[Element]) -> Result<Vec<Member>, Diagnostic> {
        let mut out: Vec<Member> = Vec::new();
        for element in elements {
            let (name, value) = self.field(element)?;
            if out.iter().any(|m| m.name == name) {
                // The attribute is there, since `field` has read it.
                return Err(self.fail(element.attributes[0].pos, format!("Repeated field \"{name}\"")));
            }
            out.push(Member {
                name: name.to_string(),
                pos: element.pos,
                value: self.value(value)?,
            });
        }
        Ok(out)
    }

    fn constructor(&self, element: &Element) -> Result<Value, Diagnostic> {
        let (name, name_pos) = self.attribute(element, "name")?;
        self.name(name, name_pos, Names::Constructor)?;
        let mut children = element.children.iter();
        let body = match children.next() {
            None => None,
            Some(first) => Some(match first.name.as_str() {
                "record" => {
                    self.no_attributes(first)?;
                    TaggedBody::Record(self.members(&first.children)?)
                }
                "array" => {
                    self.no_attributes(first)?;
                    TaggedBody::Array(self.values(&first.children)?)
                }
                "tuple" => {
                    self.no_attributes(first)?;
                    let mut arguments = Vec::new();
                    for child in &first.children {
                        arguments.push(Argument {
                            name: None,
                            pos: child.pos,
                            value: self.value(child)?,
                        });
                    }
                    if let Some(named) = children.next() {
                        if named.name != "named" {
                            return Err(self.after_the_body(named));
                        }
                        self.no_attributes(named)?;
                        for member in self.members(&named.children)? {
                            arguments.push(Argument {
                                name: Some(member.name),
                                pos: member.pos,
                                value: member.value,
                            });
                        }
                    }
                    TaggedBody::Tuple(arguments)
                }
                "named" => {
                    return Err(self.fail(first.pos, "A named needs a tuple before it to go with it"));
                }
                other => {
                    let message =
                        format!("Unexpected element {other} in a constructor; it holds a record, a tuple or an array");
                    return Err(self.fail(first.pos, message));
                }
            }),
        };
        if let Some(extra) = children.next() {
            return Err(self.after_the_body(extra));
        }
        Ok(Value {
            pos: element.pos,
            kind: ValueKind::Tagged {
                name: name.to_string(),
                body,
            },
        })
    }

    /// The error for an element that comes where a constructor has nothing more.
    fn after_the_body(&self, extra: &Element) -> Diagnostic {
        let message = match extra.name.as_str() {
            "record" | "tuple" | "array" => "A constructor has only one of record, tuple and array".to_string(),
            "named" => "A named needs a tuple before it to go with it".to_string(),
            other => format!("Unexpected element {other} in a constructor"),
        };
        self.fail(extra.pos, message)
    }
}

/// Whether the name is that of an element that is a value.
fn is_value_element(name: &str) -> bool {
    matches!(
        name,
        "nat" | "int" | "float" | "string" | "array" | "record" | "const" | "constructor"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convert::encode_exact;
    use crate::values_text::render;
    use crate::xml::{encode_xml, write_document};
    use crate::xml_tree::parse_document;

    fn values_in(body: &str) -> String {
        format!("<values>{body}</values>")
    }

    /// Reads a document and renders each value on a line.
    fn read(document: &str) -> Result<String, Diagnostic> {
        let root = parse_document(document).map_err(|e| Diagnostic::new("t.xml", e.pos, e.message))?;
        let mut out = String::new();
        values_from_xml(&root, "t.xml", |v| -> Result<(), ()> {
            out.push_str(&render(&v, true));
            Ok(())
        })
        .map_err(|e| match e {
            ReadError::Invalid(d) => d,
            ReadError::Caller(()) => unreachable!("The caller does not fail."),
        })?;
        Ok(out)
    }

    fn one(body: &str) -> String {
        read(&values_in(body))
            .unwrap_or_else(|e| panic!("{body}: {e}"))
            .trim_end()
            .to_string()
    }

    fn fails(body: &str) -> String {
        read(&values_in(body))
            .expect_err("The document should not be read.")
            .to_string()
    }

    #[test]
    fn every_kind() {
        assert_eq!(one(r#"<nat value="5"/>"#), "5;");
        assert_eq!(one(r#"<int value="+5"/>"#), "+5;");
        assert_eq!(one(r#"<int value="-5"/>"#), "-5;");
        assert_eq!(one(r#"<float value="1.50"/>"#), "1.50;");
        assert_eq!(one(r#"<float value="-2.5e3"/>"#), "-2.5e3;");
        assert_eq!(one(r#"<string value="a&#10;b"/>"#), r#""a\nb";"#);
        assert_eq!(
            one(r#"<array><nat value="1"/><string value="x"/></array>"#),
            r#"[1, "x"];"#
        );
        assert_eq!(one("<array/>"), "[];");
        assert_eq!(
            one(r#"<record><field name="b"><nat value="1"/></field><field name="a"><nat value="2"/></field></record>"#),
            "{ b: 1, a: 2 };"
        );
        assert_eq!(one("<record/>"), "{};");
        assert_eq!(one(r#"<const name="red"/>"#), "red;");
        assert_eq!(one(r#"<constructor name="Quit"/>"#), "Quit;");
        assert_eq!(one(r#"<constructor name="shapes::Circle"/>"#), "shapes::Circle;");
        assert_eq!(one(r#"<const name="a.b::red"/>"#), "a.b::red;");
    }

    #[test]
    fn constructors_with_bodies() {
        assert_eq!(
            one(r#"<constructor name="Move"><record><field name="x"><nat value="1"/></field></record></constructor>"#),
            "Move { x: 1 };"
        );
        assert_eq!(one(r#"<constructor name="Move"><record/></constructor>"#), "Move {};");
        assert_eq!(
            one(r#"<constructor name="Write"><tuple><string value="hi"/></tuple></constructor>"#),
            r#"Write("hi");"#
        );
        assert_eq!(one(r#"<constructor name="Ping"><tuple/></constructor>"#), "Ping();");
        assert_eq!(one(r#"<constructor name="Users"><array/></constructor>"#), "Users[];");
        assert_eq!(
            one(concat!(
                r#"<constructor name="Span"><tuple><nat value="1"/></tuple>"#,
                r#"<named><field name="end"><nat value="3"/></field></named></constructor>"#
            )),
            "Span(1, end: 3);"
        );
        assert_eq!(
            one(concat!(
                r#"<constructor name="Span"><tuple/><named><field name="a"><nat value="2"/></field>"#,
                r#"</named></constructor>"#
            )),
            "Span(a: 2);"
        );
    }

    #[test]
    fn no_values_is_an_empty_list() {
        assert_eq!(read("<values/>").unwrap(), "");
        assert_eq!(read("<values>\n</values>").unwrap(), "");
        assert_eq!(read("<values><!-- nothing --></values>").unwrap(), "");
    }

    #[test]
    fn values_come_in_order() {
        assert_eq!(one(r#"<nat value="1"/><nat value="2"/>"#), "1;\n2;");
    }

    #[test]
    fn the_root_must_be_values() {
        let e = read(r#"<nat value="5"/>"#).unwrap_err();
        assert!(e
            .to_string()
            .starts_with("ERROR: Expected the root element values, not nat (t.xml, line 1, col 1)"));
        assert!(e.to_string().contains("wrap the value in <values>"));
        assert!(read("<list/>").unwrap_err().to_string().contains("not list"));
        assert!(read(r#"<values a="1"/>"#)
            .unwrap_err()
            .to_string()
            .contains("Unexpected attribute a on values"));
    }

    #[test]
    fn a_number_must_be_the_text_of_a_literal_of_its_kind() {
        assert_eq!(
            fails(r#"<int value="5"/>"#),
            concat!(
                "ERROR: The text \"5\" is not an int (t.xml, line 1, col 20); ",
                "it reads as a nat, so the element is <nat value=\"5\"/>"
            )
        );
        assert!(fails(r#"<nat value="+5"/>"#).contains("is not a nat"));
        assert!(fails(r#"<nat value="1.5"/>"#).contains("is not a nat"));
        assert!(fails(r#"<float value="5"/>"#).contains("is not a float"));
        assert!(fails(r#"<nat value="abc"/>"#).contains("is not a nat"));
        assert!(fails(r#"<nat value=""/>"#).contains("is not a nat"));
        assert!(fails(r#"<nat value=" 5"/>"#).contains("is not a nat"));
        assert!(fails(r#"<nat value="5;"/>"#).contains("is not a nat"));
        assert!(fails(r#"<nat value="5, 6"/>"#).contains("is not a nat"));
    }

    #[test]
    fn a_name_must_be_a_name_of_its_kind() {
        assert!(fails(r#"<const name="Red"/>"#).contains("is not the name of a const"));
        assert!(fails(r#"<constructor name="quit"/>"#).contains("is not the name of a constructor"));
        assert!(fails(r#"<const name="not a name"/>"#).contains("is not the name of a const"));
        assert!(fails(r#"<const name=""/>"#).contains("is not the name of a const"));
        assert!(fails(r#"<record><field name="Bad"><nat value="1"/></field></record>"#)
            .contains("not a name that a field can have"));
    }

    #[test]
    fn the_attributes_are_exactly_those_of_the_kind() {
        assert_eq!(
            fails("<nat/>"),
            "ERROR: Expected a value attribute on nat (t.xml, line 1, col 9)"
        );
        assert_eq!(
            fails(r#"<nat name="5"/>"#),
            "ERROR: Unexpected attribute name on nat; it has value (t.xml, line 1, col 14)"
        );
        assert!(fails(r#"<nat value="5" extra="1"/>"#).contains("Unexpected attribute extra on nat"));
        assert!(fails(r#"<array value="1"/>"#).contains("Unexpected attribute value on array; it has none"));
        assert!(fails(r#"<const value="red"/>"#).contains("Unexpected attribute value on const; it has name"));
        assert!(fails("<constructor/>").contains("Expected a name attribute on constructor"));
        assert!(fails("<string/>").contains("Expected a value attribute on string"));
    }

    #[test]
    fn a_leaf_has_no_content() {
        assert!(fails(r#"<nat value="5"><nat value="6"/></nat>"#).contains("A nat has no content"));
        assert!(fails(r#"<const name="red"><nat value="6"/></const>"#).contains("A const has no content"));
    }

    #[test]
    fn an_element_that_is_not_a_value_is_refused() {
        assert!(fails("<foo/>").contains("Unknown element foo; a value is nat"));
        assert!(fails("<field/>").contains("The element field is not a value here"));
        assert!(fails("<tuple/>").contains("The element tuple is not a value here"));
        assert!(fails("<values/>").contains("The element values is not a value here"));
        assert!(fails("<array><field/></array>").contains("The element field is not a value here"));
    }

    #[test]
    fn a_record_holds_fields_with_one_value_each() {
        assert!(fails("<record><nat/></record>").contains("Expected a field element here, not nat"));
        assert!(fails(r#"<record><field name="a"/></record>"#).contains("A field holds exactly one value"));
        assert!(
            fails(r#"<record><field name="a"><nat value="1"/><nat value="2"/></field></record>"#)
                .contains("A field holds exactly one value")
        );
        assert!(
            fails("<record><field><nat value=\"1\"/></field></record>").contains("Expected a name attribute on field")
        );
        assert_eq!(
            fails(
                r#"<record><field name="a"><nat value="1"/></field><field name="a"><nat value="2"/></field></record>"#
            ),
            "ERROR: Repeated field \"a\" (t.xml, line 1, col 64)"
        );
    }

    #[test]
    fn a_constructor_has_one_body_and_then_named_after_a_tuple() {
        let both = r#"<constructor name="C"><record/><array/></constructor>"#;
        assert!(fails(both).contains("A constructor has only one of record, tuple and array"));
        let two_tuples = r#"<constructor name="C"><tuple/><tuple/></constructor>"#;
        assert!(fails(two_tuples).contains("A constructor has only one of record, tuple and array"));
        assert!(fails(r#"<constructor name="C"><named/></constructor>"#).contains("A named needs a tuple"));
        assert!(fails(r#"<constructor name="C"><record/><named/></constructor>"#).contains("A named needs a tuple"));
        assert!(fails(r#"<constructor name="C"><nat value="1"/></constructor>"#)
            .contains("Unexpected element nat in a constructor"));
        assert!(
            fails(r#"<constructor name="C"><tuple/><named/><named/></constructor>"#).contains("A named needs a tuple")
        );
        // Written the other way round, as another tool might, the order is refused and not guessed at.
        assert!(fails(r#"<constructor name="C"><named/><tuple/></constructor>"#).contains("A named needs a tuple"));
    }

    #[test]
    fn a_repeated_name_in_named_is_an_error() {
        let text = concat!(
            r#"<constructor name="C"><tuple/><named><field name="a"><nat value="1"/></field>"#,
            r#"<field name="a"><nat value="2"/></field></named></constructor>"#
        );
        assert!(fails(text).contains("Repeated field \"a\""));
    }

    #[test]
    fn a_problem_in_the_xml_comes_before_any_value_is_given() {
        let mut given = 0;
        let result = parse_document(r#"<values><nat value="1"/><nat value="2"></values>"#);
        assert!(result.is_err());
        // The reader is only given a tree, so a syntax error has been found before it is called.
        let root = parse_document(r#"<values><nat value="1"/><int value="x"/></values>"#).unwrap();
        let outcome = values_from_xml(&root, "t.xml", |_| -> Result<(), ()> {
            given += 1;
            Ok(())
        });
        assert!(matches!(outcome, Err(ReadError::Invalid(_))));
        assert_eq!(given, 1, "The value before the problem was given.");
    }

    #[test]
    fn what_to_xml_writes_reads_back() {
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/syntax/examples/convert/kinds.values"
        ))
        .expect("The example is there.");
        let values = crate::parser::parse_values_with(&text, crate::parser::Experimental::default()).unwrap();
        let elements: Vec<_> = values.iter().map(|v| encode_xml("k", v).unwrap()).collect();
        for compact in [false, true] {
            let document = write_document(&elements, compact);
            let back = read(&document).unwrap_or_else(|e| panic!("{e}"));
            let expected: String = values.iter().map(|v| render(v, true)).collect();
            assert_eq!(back, expected, "compact: {compact}");
        }
        // The same values written as json-exact are the same values, which is what exact means.
        for v in &values {
            assert!(encode_exact("k", v).is_ok());
        }
    }

    #[test]
    fn every_character_that_xml_can_hold_survives() {
        let mut all = String::new();
        for code in 0u32..=0x10ffff {
            if let Some(c) = char::from_u32(code) {
                if crate::xml_tree::xml_allows(c) && !matches!(c, '"' | '\\') {
                    all.push(c);
                }
            }
        }
        let value = Value {
            pos: Pos { line: 1, col: 1 },
            kind: ValueKind::Str(all.clone()),
        };
        let element = encode_xml("t", &value).unwrap();
        for compact in [false, true] {
            let document = write_document(std::slice::from_ref(&element), compact);
            let root = parse_document(&document).unwrap_or_else(|e| panic!("{}", e.message));
            let back = value_from_xml(&root.children[0], "t").unwrap();
            assert_eq!(back.kind, ValueKind::Str(all.clone()), "compact: {compact}");
        }
    }
}
