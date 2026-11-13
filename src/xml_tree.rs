//! A parser of the small subset of XML 1.0 that `convert --to=xml` writes, which keeps the position of every element
//! and attribute.
//!
//! It is used to read back what `convert` writes, and it reads only that. It accepts an optional declaration, comments,
//! white space and elements with attributes, and it refuses everything else, so that nothing is declared, expanded or
//! fetched: a DOCTYPE, CDATA, processing instructions, text between elements, entities other than the five that XML
//! defines, and namespaces. The processing of entities is our own, which is the reason that the parser is. What the
//! elements mean is for the reader in `xml_back.rs`. See `docs/tasks/2026-11-08-convert-subcommand/to_xml.md`.

use crate::diagnostics::Pos;

/// The deepest nesting of elements that is accepted, which keeps the stack of the recursive parser bounded.
const MAX_DEPTH: u32 = 200;

/// An element, where it starts, its attributes in the order written, and the elements in it.
#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    pub name: String,
    pub pos: Pos,
    pub attributes: Vec<Attribute>,
    pub children: Vec<Element>,
}

/// An attribute: the name and where it starts, and the value, with its entities decoded, and where it starts.
#[derive(Debug, Clone, PartialEq)]
pub struct Attribute {
    pub name: String,
    pub pos: Pos,
    pub value: String,
    pub value_pos: Pos,
}

/// A problem with the text, and the place where the parser could not go on.
#[derive(Debug, Clone, PartialEq)]
pub struct XmlError {
    pub pos: Pos,
    pub message: String,
    /// What to do about it, when that is not obvious.
    pub note: Option<String>,
}

/// Whether XML 1.0 can hold the character at all. It cannot hold the control characters other than tab, newline and
/// carriage return, nor U+FFFE and U+FFFF, and a reference to one is no better than the character itself.
pub fn xml_allows(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')
}

/// Parses a document: an optional byte order mark and declaration, then the root element, with comments and white space
/// around it. The root is the element that is returned.
pub fn parse_document(text: &str) -> Result<Element, XmlError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut parser = Parser {
        chars: text.chars().collect(),
        at: 0,
        line: 1,
        col: 1,
    };
    if parser.looking_at("<?xml") && parser.after("<?xml").is_none_or(|c| is_space(c) || c == '?') {
        parser.declaration()?;
    }
    parser.misc()?;
    if parser.peek().is_none() {
        return Err(parser.error("Expected an element, but the document is empty"));
    }
    if parser.peek() != Some('<') {
        return Err(parser.error("Expected an element; text outside the root is not accepted"));
    }
    let root = parser.element(0)?;
    parser.misc()?;
    if parser.peek().is_some() {
        return Err(parser.error("Expected the end of the document after the root element"));
    }
    Ok(root)
}

fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r')
}

struct Parser {
    chars: Vec<char>,
    at: usize,
    line: u32,
    col: u32,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    /// The character after the given text, if the parser is at it.
    fn after(&self, text: &str) -> Option<char> {
        self.chars.get(self.at + text.chars().count()).copied()
    }

    fn looking_at(&self, text: &str) -> bool {
        text.chars()
            .enumerate()
            .all(|(i, c)| self.chars.get(self.at + i) == Some(&c))
    }

    fn pos(&self) -> Pos {
        Pos {
            line: self.line,
            col: self.col,
        }
    }

    fn error(&self, message: &str) -> XmlError {
        self.error_at(self.pos(), message)
    }

    fn error_at(&self, pos: Pos, message: &str) -> XmlError {
        XmlError {
            pos,
            message: message.to_string(),
            note: None,
        }
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.at += 1;
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn skip(&mut self, text: &str) {
        for _ in text.chars() {
            self.bump();
        }
    }

    fn skip_space(&mut self) {
        while self.peek().is_some_and(is_space) {
            self.bump();
        }
    }

    /// Skips white space and comments, which may be anywhere that an element may be, and refuses the other things that
    /// can start with `<!` or `<?`.
    fn misc(&mut self) -> Result<(), XmlError> {
        loop {
            self.skip_space();
            if self.looking_at("<!--") {
                self.comment()?;
            } else if self.looking_at("<!DOCTYPE") {
                return Err(self.error("A DOCTYPE is not accepted, since it can declare entities"));
            } else if self.looking_at("<![CDATA[") {
                return Err(self.error("A CDATA section is not accepted; a value is in an attribute"));
            } else if self.looking_at("<?") {
                return Err(self.error("A processing instruction is not accepted"));
            } else {
                return Ok(());
            }
        }
    }

    /// Skips a comment. XML does not allow `--` inside one.
    fn comment(&mut self) -> Result<(), XmlError> {
        self.skip("<!--");
        loop {
            if self.looking_at("-->") {
                self.skip("-->");
                return Ok(());
            }
            if self.looking_at("--") {
                return Err(self.error("`--` is not allowed inside a comment"));
            }
            match self.bump() {
                Some(c) if xml_allows(c) => {}
                Some(_) => {
                    return Err(self.error_at(
                        self.previous_pos(),
                        "A comment holds a character that XML 1.0 cannot hold",
                    ))
                }
                None => return Err(self.error("The comment is not closed; expected -->")),
            }
        }
    }

    /// Where the character that was just read is. Only a character that is not a new line is looked back at.
    fn previous_pos(&self) -> Pos {
        Pos {
            line: self.line,
            col: self.col.saturating_sub(1).max(1),
        }
    }

    /// Reads the declaration, `<?xml version="1.0" encoding="UTF-8"?>`. The version is 1.0 and the encoding, if there
    /// is one, is UTF-8, which is what the text of a file is read as.
    fn declaration(&mut self) -> Result<(), XmlError> {
        self.skip("<?xml");
        let mut version = false;
        loop {
            let spaced = self.peek().is_some_and(is_space);
            self.skip_space();
            if self.looking_at("?>") {
                self.skip("?>");
                break;
            }
            if !spaced {
                return Err(self.error("Expected white space or ?> in the declaration"));
            }
            let pos = self.pos();
            let name = self.name()?;
            self.skip_space();
            if self.peek() != Some('=') {
                return Err(self.error("Expected = after the name in the declaration"));
            }
            self.bump();
            self.skip_space();
            let value_pos = self.pos();
            let value = self.quoted(false)?;
            match name.as_str() {
                "version" if !version => {
                    if value != "1.0" {
                        let mut e = self.error_at(value_pos, "Only XML version 1.0 is accepted");
                        e.note = Some("XML 1.1 is not written, and many tools do not read it".to_string());
                        return Err(e);
                    }
                    version = true;
                }
                "encoding" if version => {
                    if !value.eq_ignore_ascii_case("utf-8") {
                        return Err(self.error_at(value_pos, "Only the encoding UTF-8 is accepted"));
                    }
                }
                "standalone" => return Err(self.error_at(pos, "The declaration has no standalone; it is not accepted")),
                _ => return Err(self.error_at(pos, "Expected version, and then encoding, in the declaration")),
            }
        }
        if !version {
            return Err(self.error("The declaration needs a version"));
        }
        Ok(())
    }

    /// A name: ASCII letters, digits, `_`, `-` and `.`, not starting with a digit, `-` or `.`. A colon is refused, so
    /// no namespaces are read.
    fn name(&mut self) -> Result<String, XmlError> {
        let start = self.pos();
        let mut name = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.') {
                name.push(c);
                self.bump();
            } else if c == ':' {
                return Err(self.error("A name with a colon is not accepted, since namespaces are not read"));
            } else {
                break;
            }
        }
        if name.is_empty() || name.starts_with(|c: char| !(c.is_ascii_alphabetic() || c == '_')) {
            return Err(self.error_at(start, "Expected a name"));
        }
        Ok(name)
    }

    /// An attribute value or a quoted value of the declaration: in single or double quotes, with the entities decoded.
    /// A tab, a newline or a carriage return that is written as itself is refused, for the reason in the note.
    fn quoted(&mut self, entities: bool) -> Result<String, XmlError> {
        let quote = match self.peek() {
            Some(q @ ('"' | '\'')) => q,
            _ => return Err(self.error("Expected a value in quotes")),
        };
        self.bump();
        let mut value = String::new();
        loop {
            let pos = self.pos();
            match self.peek() {
                None => return Err(self.error("The value is not closed; expected the closing quote")),
                Some(c) if c == quote => {
                    self.bump();
                    return Ok(value);
                }
                Some('<') => return Err(self.error("A < is not allowed in a value; write &lt;")),
                Some('&') if entities => value.push(self.entity()?),
                Some('&') => return Err(self.error("An entity is not accepted here")),
                Some(c @ ('\t' | '\n' | '\r')) => {
                    let reference = match c {
                        '\t' => "&#9;",
                        '\n' => "&#10;",
                        _ => "&#13;",
                    };
                    let name = match c {
                        '\t' => "tab",
                        '\n' => "new line",
                        _ => "carriage return",
                    };
                    let mut e = self.error_at(pos, &format!("A {name} in an attribute value is not accepted"));
                    e.note = Some(format!(
                        "an XML parser reads it as a space, so write {reference} to keep it, as --to=xml does"
                    ));
                    return Err(e);
                }
                Some(c) if !xml_allows(c) => {
                    return Err(self.error(&format!("U+{:04X} is a character that XML 1.0 cannot hold", c as u32)));
                }
                Some(c) => {
                    value.push(c);
                    self.bump();
                }
            }
        }
    }

    /// Reads a reference to an entity or a character, which is the five entities that XML defines and numeric
    /// references to a character that XML 1.0 can hold. The parser is at the `&`.
    fn entity(&mut self) -> Result<char, XmlError> {
        let pos = self.pos();
        self.bump();
        let mut body = String::new();
        loop {
            match self.peek() {
                Some(';') => {
                    self.bump();
                    break;
                }
                // The longest reference that is valid is `&#x10FFFF;`, so a longer one is not looked at further.
                Some(c) if body.chars().count() < 12 && (c.is_ascii_alphanumeric() || c == '#') => {
                    body.push(c);
                    self.bump();
                }
                _ => return Err(self.error_at(pos, "An & that starts no entity; write &amp; for a literal &")),
            }
        }
        let c = match body.as_str() {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            _ => match body.strip_prefix('#') {
                Some(digits) => {
                    let code = match digits.strip_prefix('x') {
                        Some(hex) if !hex.is_empty() => u32::from_str_radix(hex, 16).ok(),
                        None if !digits.is_empty() => digits.parse::<u32>().ok(),
                        _ => None,
                    };
                    match code.and_then(char::from_u32) {
                        Some(c) if xml_allows(c) => c,
                        Some(_) | None => {
                            let message = format!("&{body}; is not a character that XML 1.0 can hold");
                            return Err(self.error_at(pos, &message));
                        }
                    }
                }
                None => {
                    let message =
                        format!("The entity &{body}; is not accepted; only &amp;, &lt;, &gt;, &quot; and &apos; are");
                    return Err(self.error_at(pos, &message));
                }
            },
        };
        Ok(c)
    }

    fn element(&mut self, depth: u32) -> Result<Element, XmlError> {
        if depth >= MAX_DEPTH {
            return Err(self.error("The XML is nested too deeply"));
        }
        let pos = self.pos();
        self.bump();
        let name = self.name()?;
        let mut attributes: Vec<Attribute> = Vec::new();
        loop {
            let spaced = self.peek().is_some_and(is_space);
            self.skip_space();
            match self.peek() {
                Some('/') => {
                    self.bump();
                    if self.peek() != Some('>') {
                        return Err(self.error("Expected > after the / that ends an element"));
                    }
                    self.bump();
                    return Ok(Element {
                        name,
                        pos,
                        attributes,
                        children: Vec::new(),
                    });
                }
                Some('>') => {
                    self.bump();
                    break;
                }
                Some(_) if spaced => {
                    let attribute_pos = self.pos();
                    let attribute = self.name()?;
                    if attribute == "xmlns" {
                        return Err(
                            self.error_at(attribute_pos, "xmlns is not accepted, since namespaces are not read")
                        );
                    }
                    self.skip_space();
                    if self.peek() != Some('=') {
                        return Err(self.error("Expected = after the name of an attribute"));
                    }
                    self.bump();
                    self.skip_space();
                    let value_pos = self.pos();
                    let value = self.quoted(true)?;
                    if attributes.iter().any(|a| a.name == attribute) {
                        let message = format!("The attribute {attribute} is given more than once");
                        return Err(self.error_at(attribute_pos, &message));
                    }
                    attributes.push(Attribute {
                        name: attribute,
                        pos: attribute_pos,
                        value,
                        value_pos,
                    });
                }
                Some(_) => return Err(self.error("Expected white space, > or /> in the element")),
                None => return Err(self.error("The element is not closed")),
            }
        }
        let mut children = Vec::new();
        loop {
            self.misc()?;
            match self.peek() {
                None => return Err(self.error(&format!("The element {name} is not closed; expected </{name}>"))),
                Some('<') if self.after("<") == Some('/') => {
                    let closing = self.pos();
                    self.skip("</");
                    let closed = self.name()?;
                    if closed != name {
                        let message = format!("Expected </{name}> to close the element, not </{closed}>");
                        return Err(self.error_at(closing, &message));
                    }
                    self.skip_space();
                    if self.peek() != Some('>') {
                        return Err(self.error("Expected > to end the closing tag"));
                    }
                    self.bump();
                    return Ok(Element {
                        name,
                        pos,
                        attributes,
                        children,
                    });
                }
                Some('<') => children.push(self.element(depth + 1)?),
                Some(_) => {
                    let mut e = self.error("Text between elements is not accepted");
                    e.note = Some("a value is in an attribute, as in <nat value=\"5\"/>".to_string());
                    return Err(e);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Element {
        parse_document(text).unwrap_or_else(|e| panic!("{text}: {} at {}:{}", e.message, e.pos.line, e.pos.col))
    }

    /// The message and position of the error, as `message at line:col`.
    fn fails(text: &str) -> String {
        let e = parse_document(text).expect_err("The text should not parse.");
        format!("{} at {}:{}", e.message, e.pos.line, e.pos.col)
    }

    fn value_of(text: &str) -> String {
        parse(text).attributes[0].value.clone()
    }

    #[test]
    fn an_element_has_attributes_and_children() {
        let root = parse(r#"<a x="1" y='2'><b/><c z="3"></c></a>"#);
        assert_eq!(root.name, "a");
        assert_eq!(root.attributes.len(), 2);
        assert_eq!(root.attributes[1].name, "y");
        assert_eq!(root.attributes[1].value, "2");
        assert_eq!(root.children.len(), 2);
        assert_eq!(root.children[1].attributes[0].value, "3");
    }

    #[test]
    fn positions_are_those_of_the_element_and_the_attribute() {
        let root = parse("<a>\n  <b  x=\"1\"/>\n</a>");
        let b = &root.children[0];
        assert_eq!((b.pos.line, b.pos.col), (2, 3));
        assert_eq!((b.attributes[0].pos.line, b.attributes[0].pos.col), (2, 7));
        assert_eq!((b.attributes[0].value_pos.line, b.attributes[0].value_pos.col), (2, 9));
    }

    #[test]
    fn the_declaration_is_optional_and_may_have_an_encoding() {
        parse(r#"<?xml version="1.0" encoding="UTF-8"?><a/>"#);
        parse(r#"<?xml version="1.0"?><a/>"#);
        parse("<?xml version='1.0' encoding='utf-8' ?>\n<a/>");
        parse("<a/>");
        parse("\u{feff}<a/>");
    }

    #[test]
    fn the_declaration_is_strict() {
        assert_eq!(
            fails(r#"<?xml version="1.1"?><a/>"#),
            "Only XML version 1.0 is accepted at 1:15"
        );
        assert_eq!(
            fails(r#"<?xml version="1.0" encoding="latin-1"?><a/>"#),
            "Only the encoding UTF-8 is accepted at 1:30"
        );
        let standalone = fails(r#"<?xml version="1.0" standalone="yes"?><a/>"#);
        assert!(standalone.starts_with("The declaration has no standalone"));
        assert!(fails(r#"<?xml encoding="UTF-8"?><a/>"#).starts_with("Expected version"));
        assert!(fails("<?xml?><a/>").starts_with("The declaration needs a version"));
        // A declaration that is not first is a processing instruction.
        assert!(fails("\n<?xml version=\"1.0\"?><a/>").starts_with("A processing instruction is not accepted"));
    }

    #[test]
    fn comments_and_white_space_are_skipped_around_and_between_elements() {
        parse("<!-- before -->\n<a>\n  <!-- inside -->\n  <b/>\n</a>\n<!-- after -->\n");
        assert!(fails("<a><!-- a -- b --></a>").starts_with("`--` is not allowed inside a comment"));
        assert!(fails("<a><!-- never closed </a>").starts_with("The comment is not closed"));
    }

    #[test]
    fn the_five_entities_are_decoded() {
        assert_eq!(value_of(r#"<a v="&amp;&lt;&gt;&quot;&apos;"/>"#), "&<>\"'");
        assert_eq!(value_of(r#"<a v='"it&apos;s"'/>"#), "\"it's\"");
    }

    #[test]
    fn numeric_references_are_decoded_and_keep_white_space() {
        assert_eq!(
            value_of(r#"<a v="&#10;&#9;&#13;&#xA;&#x41;&#233;&#x1F600;"/>"#),
            "\n\t\r\nAé😀"
        );
    }

    #[test]
    fn a_reference_to_a_character_that_xml_cannot_hold_is_refused() {
        assert_eq!(
            fails(r#"<a v="&#1;"/>"#),
            "&#1; is not a character that XML 1.0 can hold at 1:7"
        );
        assert!(fails(r#"<a v="&#0;"/>"#).contains("&#0;"));
        assert!(fails(r#"<a v="&#xFFFE;"/>"#).contains("not a character"));
        assert!(fails(r#"<a v="&#x110000;"/>"#).contains("not a character"));
        assert!(fails(r#"<a v="&#xD800;"/>"#).contains("not a character"));
        assert!(fails(r#"<a v="&#;"/>"#).contains("not a character"));
        assert!(fails(r#"<a v="&#x;"/>"#).contains("not a character"));
        assert!(fails(r#"<a v="&#12a;"/>"#).contains("not a character"));
    }

    #[test]
    fn another_entity_is_refused_and_named() {
        assert_eq!(
            fails(r#"<a v="&nbsp;"/>"#),
            "The entity &nbsp; is not accepted; only &amp;, &lt;, &gt;, &quot; and &apos; are at 1:7"
        );
        assert!(fails(r#"<a v="a & b"/>"#).starts_with("An & that starts no entity"));
        assert!(fails(r#"<a v="&amp"/>"#).starts_with("An & that starts no entity"));
        assert!(fails(r#"<a v="&aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa;"/>"#).starts_with("An & that starts no entity"));
    }

    #[test]
    fn a_literal_tab_newline_or_carriage_return_in_a_value_is_refused_with_a_note() {
        for (text, word) in [
            ("<a v=\"x\ty\"/>", "tab"),
            ("<a v=\"x\ny\"/>", "new line"),
            ("<a v=\"x\ry\"/>", "carriage return"),
        ] {
            let e = parse_document(text).expect_err("The value should be refused.");
            assert_eq!(e.message, format!("A {word} in an attribute value is not accepted"));
            assert!(e.note.expect("There is a note.").contains("&#"));
        }
        assert_eq!(
            fails("<a v=\"x\ny\"/>"),
            "A new line in an attribute value is not accepted at 1:8"
        );
    }

    #[test]
    fn a_character_that_xml_cannot_hold_is_refused_as_itself() {
        assert_eq!(
            fails("<a v=\"x\u{1}y\"/>"),
            "U+0001 is a character that XML 1.0 cannot hold at 1:8"
        );
        assert!(fails("<a v=\"\u{ffff}\"/>").contains("U+FFFF"));
    }

    #[test]
    fn white_space_and_other_characters_in_a_value_are_kept() {
        assert_eq!(value_of("<a v=\"  x  y  \"/>"), "  x  y  ");
        assert_eq!(value_of("<a v=\"café ☕ 😀\"/>"), "café ☕ 😀");
        assert_eq!(value_of("<a v=\"\"/>"), "");
    }

    #[test]
    fn a_less_than_in_a_value_is_refused() {
        assert!(fails(r#"<a v="a<b"/>"#).starts_with("A < is not allowed in a value"));
    }

    #[test]
    fn a_doctype_cdata_and_a_processing_instruction_are_refused() {
        assert!(fails("<!DOCTYPE a [<!ENTITY e \"x\">]><a/>").starts_with("A DOCTYPE is not accepted"));
        assert!(fails("<a><![CDATA[x]]></a>").starts_with("A CDATA section is not accepted"));
        assert!(fails("<?xml-stylesheet href=\"s\"?><a/>").starts_with("A processing instruction"));
        assert!(fails("<a><?pi x?></a>").starts_with("A processing instruction"));
    }

    #[test]
    fn text_between_elements_is_refused() {
        assert_eq!(fails("<a>text</a>"), "Text between elements is not accepted at 1:4");
        assert!(fails("<a><b/>text</a>").starts_with("Text between elements"));
        assert!(fails("<a>&amp;</a>").starts_with("Text between elements"));
        assert!(fails("text<a/>").starts_with("Expected an element; text outside the root"));
        assert!(fails("<a/>text").starts_with("Expected the end of the document"));
    }

    #[test]
    fn namespaces_are_refused() {
        assert!(fails("<x:a/>").starts_with("A name with a colon"));
        assert!(fails(r#"<a x:v="1"/>"#).starts_with("A name with a colon"));
        assert!(fails(r#"<a xmlns="urn:x"/>"#).starts_with("xmlns is not accepted"));
    }

    #[test]
    fn the_structure_must_be_well_formed() {
        assert!(fails("<a></b>").starts_with("Expected </a> to close the element, not </b>"));
        assert!(fails("<a>").starts_with("The element a is not closed"));
        assert!(fails("<a><b/>").starts_with("The element a is not closed"));
        assert!(fails("<a").starts_with("The element is not closed"));
        assert!(fails(r#"<a v="1"#).starts_with("The value is not closed"));
        assert!(fails(r#"<a v=1/>"#).starts_with("Expected a value in quotes"));
        assert!(fails(r#"<a v/>"#).starts_with("Expected = after"));
        assert!(fails(r#"<a v="1"w="2"/>"#).starts_with("Expected white space"));
        assert!(fails(r#"<a v="1" v="2"/>"#).starts_with("The attribute v is given more than once"));
        assert!(fails("<a/><b/>").starts_with("Expected the end of the document"));
        assert!(fails("<1/>").starts_with("Expected a name"));
        assert!(fails("<>").starts_with("Expected a name"));
        assert!(fails("<a / >").starts_with("Expected > after the /"));
    }

    #[test]
    fn an_empty_document_is_an_error() {
        assert!(fails("").starts_with("Expected an element, but the document is empty"));
        assert!(fails("  \n ").starts_with("Expected an element, but the document is empty"));
        assert!(fails("<!-- only a comment -->").starts_with("Expected an element, but the document is empty"));
        assert!(fails(r#"<?xml version="1.0"?>"#).starts_with("Expected an element, but the document is empty"));
    }

    #[test]
    fn the_nesting_is_limited() {
        let deep = |n: usize| format!("{}{}", "<a>".repeat(n), "</a>".repeat(n));
        parse(&deep(200));
        assert!(fails(&deep(201)).starts_with("The XML is nested too deeply"));
        // A document that is cut off deep inside is still an error and not a crash.
        assert!(parse_document(&"<a>".repeat(100_000)).is_err());
    }

    #[test]
    fn white_space_may_be_in_the_tags() {
        let root = parse("<a\n  x = \"1\"\n  y='2'\n>\n</a\n>");
        assert_eq!(root.attributes.len(), 2);
        assert_eq!(root.attributes[1].pos.line, 3);
    }
}
