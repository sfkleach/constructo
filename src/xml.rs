//! The `xml` format: values written as XML in which every value is an element that says what kind of value it is.
//!
//! It is exact in the sense of `json-exact`: the kind, the structure and the text of the numbers are kept, and no types
//! are needed. Every element is empty or holds only elements, and the text of a literal or a name is an attribute, so
//! that there is no text content and no white space that could be taken for a part of a value. See
//! `docs/tasks/2026-11-08-convert-subcommand/to_xml.md`.

use crate::ast::{Argument, Member, TaggedBody, Value, ValueKind};
use crate::convert::is_const_name;
use crate::diagnostics::Diagnostic;
use crate::xml_tree::xml_allows;
use std::collections::HashSet;

/// The declaration that starts every document.
const DECLARATION: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>";

/// An element: its tag, at most one attribute, and the elements in it. The attribute is `value` for a literal and
/// `name` for a name, so there is never more than one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Element {
    pub tag: &'static str,
    pub attribute: Option<(&'static str, String)>,
    pub children: Vec<Element>,
}

impl Element {
    /// An element with no attribute that holds `children`.
    fn container(tag: &'static str, children: Vec<Element>) -> Element {
        Element {
            tag,
            attribute: None,
            children,
        }
    }

    /// An element with no content, whose one attribute is `attribute`.
    fn leaf(tag: &'static str, attribute: &'static str, text: &str) -> Element {
        Element {
            tag,
            attribute: Some((attribute, text.to_string())),
            children: Vec::new(),
        }
    }

    /// An element that holds `children` and has one attribute, as a constructor or a field does with its name.
    fn named(tag: &'static str, name: &str, children: Vec<Element>) -> Element {
        Element {
            tag,
            attribute: Some(("name", name.to_string())),
            children,
        }
    }

    /// Writes the element, which is `depth` levels in. When it is not compact there is one element on each line, with
    /// two spaces for each level, and an element with nothing in it is closed by `/>`.
    fn write(&self, out: &mut String, depth: usize, compact: bool) {
        if !compact {
            out.push_str(&"  ".repeat(depth));
        }
        out.push('<');
        out.push_str(self.tag);
        if let Some((key, text)) = &self.attribute {
            out.push(' ');
            out.push_str(key);
            out.push_str("=\"");
            write_attribute(out, text);
            out.push('"');
        }
        if self.children.is_empty() {
            out.push_str("/>");
            if !compact {
                out.push('\n');
            }
            return;
        }
        out.push('>');
        if !compact {
            out.push('\n');
        }
        for child in &self.children {
            child.write(out, depth + 1, compact);
        }
        if !compact {
            out.push_str(&"  ".repeat(depth));
        }
        out.push_str("</");
        out.push_str(self.tag);
        out.push('>');
        if !compact {
            out.push('\n');
        }
    }
}

/// Writes the text of an attribute, which is in double quotes. The characters that XML gives a meaning are written as
/// entities. A tab, a newline and a carriage return are written as references to them, since a parser reads one that is
/// written as itself in an attribute as a space, which would change the string. Everything else is written as it is.
fn write_attribute(out: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\t' => out.push_str("&#9;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            c => out.push(c),
        }
    }
}

/// Writes a document of the values, which are in the order that they are to be read back. The root is always `values`,
/// and a list of no values is `<values/>`. The document ends with a new line. When it is compact the declaration is
/// followed by a new line and then the rest is on one line.
pub fn write_document(values: &[Element], compact: bool) -> String {
    let mut out = String::from(DECLARATION);
    out.push('\n');
    Element::container("values", values.to_vec()).write(&mut out, 0, compact);
    if compact {
        out.push('\n');
    }
    out
}

/// Encodes a value as an element. The `file` is only used to name the source in an error.
///
/// The errors are a name that is given twice, which `json-exact` cannot hold and so the two formats are kept to the
/// same inputs, and a string with a character that XML 1.0 cannot write. Nothing else is checked, as there are no
/// types.
pub fn encode_xml(file: &str, value: &Value) -> Result<Element, Diagnostic> {
    Ok(match &value.kind {
        ValueKind::Nat(text) => Element::leaf("nat", "value", text),
        ValueKind::Int(text) => Element::leaf("int", "value", text),
        ValueKind::Float(text) => Element::leaf("float", "value", text),
        ValueKind::Str(text) => {
            if let Some(c) = text.chars().find(|c| !xml_allows(*c)) {
                let message = format!("The string holds U+{:04X}, which XML 1.0 cannot write", c as u32);
                return Err(Diagnostic::new(file, value.pos, message));
            }
            Element::leaf("string", "value", text)
        }
        ValueKind::Array(items) => Element::container("array", encode_all(file, items)?),
        ValueKind::Record(members) => Element::container("record", encode_members(file, members)?),
        ValueKind::Tagged { name, body } => {
            if is_const_name(name) {
                return Ok(Element::leaf("const", "name", name));
            }
            let mut children = Vec::new();
            match body {
                None => {}
                Some(TaggedBody::Record(fields)) => {
                    children.push(Element::container("record", encode_members(file, fields)?))
                }
                Some(TaggedBody::Array(items)) => children.push(Element::container("array", encode_all(file, items)?)),
                Some(TaggedBody::Tuple(arguments)) => encode_tuple(file, name, arguments, &mut children)?,
            }
            Element::named("constructor", name, children)
        }
    })
}

fn encode_all(file: &str, values: &[Value]) -> Result<Vec<Element>, Diagnostic> {
    values.iter().map(|v| encode_xml(file, v)).collect()
}

/// The `field` elements of a record, in the order that they were written.
fn encode_members(file: &str, members: &[Member]) -> Result<Vec<Element>, Diagnostic> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for member in members {
        if !seen.insert(member.name.as_str()) {
            let message = format!("Duplicate field {}", member.name);
            return Err(Diagnostic::new(file, member.pos, message));
        }
        out.push(Element::named(
            "field",
            &member.name,
            vec![encode_xml(file, &member.value)?],
        ));
    }
    Ok(out)
}

/// Adds `tuple` for the positional arguments and, if there are named ones, `named` after it. The grammar puts the
/// positional arguments first.
fn encode_tuple(
    file: &str,
    constructor: &str,
    arguments: &[Argument],
    children: &mut Vec<Element>,
) -> Result<(), Diagnostic> {
    let mut positional = Vec::new();
    let mut named = Vec::new();
    let mut seen = HashSet::new();
    for argument in arguments {
        let encoded = encode_xml(file, &argument.value)?;
        match &argument.name {
            None => positional.push(encoded),
            Some(name) => {
                if !seen.insert(name.as_str()) {
                    let message = format!("Argument {} is given more than once in {}", name, constructor);
                    return Err(Diagnostic::new(file, argument.pos, message));
                }
                named.push(Element::named("field", name, vec![encoded]));
            }
        }
    }
    children.push(Element::container("tuple", positional));
    if !named.is_empty() {
        children.push(Element::container("named", named));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_values_with;
    use crate::parser::Experimental;

    fn values(text: &str) -> Vec<Value> {
        parse_values_with(text, Experimental::default()).expect("the text should parse")
    }

    /// The compact document for the values of the text, without the declaration and the final new line.
    fn compact(text: &str) -> String {
        let elements: Vec<Element> = values(text)
            .iter()
            .map(|v| encode_xml("test.values", v).expect("the value should convert"))
            .collect();
        let doc = write_document(&elements, true);
        let body = doc
            .strip_prefix(&format!("{}\n", DECLARATION))
            .expect("the declaration comes first");
        body.strip_suffix('\n')
            .expect("the document ends with a new line")
            .to_string()
    }

    fn pretty(text: &str) -> String {
        let elements: Vec<Element> = values(text)
            .iter()
            .map(|v| encode_xml("test.values", v).expect("the value should convert"))
            .collect();
        write_document(&elements, false)
    }

    fn error(text: &str) -> String {
        let v = values(text);
        encode_xml("test.values", &v[0])
            .expect_err("the value should not convert")
            .to_string()
    }

    #[test]
    fn numbers_keep_their_text() {
        assert_eq!(compact("5;"), r#"<values><nat value="5"/></values>"#);
        assert_eq!(compact("+5;"), r#"<values><int value="+5"/></values>"#);
        assert_eq!(compact("-7;"), r#"<values><int value="-7"/></values>"#);
        assert_eq!(compact("1.50;"), r#"<values><float value="1.50"/></values>"#);
        assert_eq!(compact("-2.5e3;"), r#"<values><float value="-2.5e3"/></values>"#);
    }

    #[test]
    fn strings_are_in_an_attribute() {
        assert_eq!(compact(r#""hi";"#), r#"<values><string value="hi"/></values>"#);
        assert_eq!(compact(r#"" a ";"#), r#"<values><string value=" a "/></values>"#);
        assert_eq!(compact(r#""";"#), r#"<values><string value=""/></values>"#);
        assert_eq!(
            compact("\"caf\u{e9} \u{1f600}\";"),
            "<values><string value=\"caf\u{e9} \u{1f600}\"/></values>"
        );
    }

    #[test]
    fn each_character_with_a_meaning_is_escaped() {
        assert_eq!(compact(r#""&";"#), r#"<values><string value="&amp;"/></values>"#);
        assert_eq!(compact(r#""<";"#), r#"<values><string value="&lt;"/></values>"#);
        assert_eq!(compact(r#"">";"#), r#"<values><string value="&gt;"/></values>"#);
        assert_eq!(compact(r#""\"";"#), r#"<values><string value="&quot;"/></values>"#);
        // The apostrophe is not escaped, since the attribute is in double quotes.
        assert_eq!(compact(r#""it's";"#), r#"<values><string value="it's"/></values>"#);
    }

    #[test]
    fn white_space_in_a_string_is_written_as_references() {
        assert_eq!(compact(r#""a\tb";"#), r#"<values><string value="a&#9;b"/></values>"#);
        assert_eq!(compact(r#""a\nb";"#), r#"<values><string value="a&#10;b"/></values>"#);
        assert_eq!(compact(r#""a\rb";"#), r#"<values><string value="a&#13;b"/></values>"#);
    }

    #[test]
    fn a_string_that_xml_cannot_hold_is_an_error() {
        assert_eq!(
            error(r#""a\u0001";"#),
            "ERROR: The string holds U+0001, which XML 1.0 cannot write (test.values, line 1, col 1)"
        );
        assert!(error(r#""\u0000";"#).contains("U+0000"));
        assert!(error(r#""\u001f";"#).contains("U+001F"));
        assert!(error("\"\\uFFFE\";").contains("U+FFFE"));
        assert!(error("\"\\uFFFF\";").contains("U+FFFF"));
        // The characters just inside the limits are written.
        assert!(compact("\"\\u0020\\uFFFD\\uE000\";").contains("value"));
    }

    #[test]
    fn the_position_of_the_error_is_that_of_the_string() {
        assert_eq!(
            error("[1, \"\\u0001\"];"),
            "ERROR: The string holds U+0001, which XML 1.0 cannot write (test.values, line 1, col 5)"
        );
    }

    #[test]
    fn arrays_and_records() {
        assert_eq!(
            compact("[1, 2];"),
            r#"<values><array><nat value="1"/><nat value="2"/></array></values>"#
        );
        assert_eq!(compact("[];"), "<values><array/></values>");
        assert_eq!(
            compact("{ b: 1, a: 2 };"),
            concat!(
                r#"<values><record><field name="b"><nat value="1"/></field>"#,
                r#"<field name="a"><nat value="2"/></field></record></values>"#
            )
        );
        assert_eq!(compact("{};"), "<values><record/></values>");
    }

    #[test]
    fn consts_and_constructors() {
        assert_eq!(compact("red;"), r#"<values><const name="red"/></values>"#);
        assert_eq!(compact("_hidden;"), r#"<values><const name="_hidden"/></values>"#);
        assert_eq!(compact("Quit;"), r#"<values><constructor name="Quit"/></values>"#);
        assert_eq!(
            compact("shapes::Circle;"),
            r#"<values><constructor name="shapes::Circle"/></values>"#
        );
        assert_eq!(compact("a.b::red;"), r#"<values><const name="a.b::red"/></values>"#);
    }

    #[test]
    fn constructor_bodies() {
        assert_eq!(
            compact("Move { x: 1 };"),
            concat!(
                r#"<values><constructor name="Move"><record><field name="x"><nat value="1"/></field>"#,
                r#"</record></constructor></values>"#
            )
        );
        assert_eq!(
            compact("Move {};"),
            r#"<values><constructor name="Move"><record/></constructor></values>"#
        );
        assert_eq!(
            compact(r#"Write("hi");"#),
            r#"<values><constructor name="Write"><tuple><string value="hi"/></tuple></constructor></values>"#
        );
        assert_eq!(
            compact("Ping();"),
            r#"<values><constructor name="Ping"><tuple/></constructor></values>"#
        );
        assert_eq!(
            compact("Users[];"),
            r#"<values><constructor name="Users"><array/></constructor></values>"#
        );
        assert_eq!(
            compact("Users[u1, Bob];"),
            concat!(
                r#"<values><constructor name="Users"><array><const name="u1"/><constructor name="Bob"/></array>"#,
                r#"</constructor></values>"#
            )
        );
    }

    #[test]
    fn named_arguments_follow_the_positional_ones() {
        assert_eq!(
            compact("Span(1, end: 3, start: 2);"),
            concat!(
                r#"<values><constructor name="Span"><tuple><nat value="1"/></tuple><named>"#,
                r#"<field name="end"><nat value="3"/></field><field name="start"><nat value="2"/></field>"#,
                r#"</named></constructor></values>"#
            )
        );
        assert_eq!(
            compact("Span(start: 2);"),
            concat!(
                r#"<values><constructor name="Span"><tuple/><named><field name="start"><nat value="2"/></field>"#,
                r#"</named></constructor></values>"#
            )
        );
    }

    #[test]
    fn a_name_given_twice_is_an_error() {
        assert_eq!(
            error("{ a: 1, a: 2 };"),
            "ERROR: Duplicate field a (test.values, line 1, col 9)"
        );
        assert!(error("Span(end: 1, end: 2);").contains("Argument end is given more than once in Span"));
    }

    #[test]
    fn no_values_is_an_empty_list() {
        assert_eq!(
            write_document(&[], false),
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<values/>\n"
        );
        assert_eq!(
            write_document(&[], true),
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<values/>\n"
        );
    }

    #[test]
    fn the_pretty_layout_is_one_element_on_each_line() {
        assert_eq!(
            pretty("Move { x: 1, y: 2 }; red;"),
            "\
<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<values>
  <constructor name=\"Move\">
    <record>
      <field name=\"x\">
        <nat value=\"1\"/>
      </field>
      <field name=\"y\">
        <nat value=\"2\"/>
      </field>
    </record>
  </constructor>
  <const name=\"red\"/>
</values>
"
        );
    }

    #[test]
    fn the_compact_layout_has_the_declaration_on_its_own_line() {
        let elements = vec![
            encode_xml("t", &values("1;")[0]).unwrap(),
            encode_xml("t", &values("2;")[0]).unwrap(),
        ];
        assert_eq!(
            write_document(&elements, true),
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<values><nat value=\"1\"/><nat value=\"2\"/></values>\n"
        );
    }
}
