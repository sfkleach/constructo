//! A parser of JSON (RFC 8259) that keeps the position of every value and every key.
//!
//! It is used to read back what `convert` writes. It is a plain parser of JSON: what the values mean is for the
//! reader in `exact.rs`. Numbers are kept as the text that was written, and a repeated key in an object is kept.

use crate::diagnostics::{shown, Pos};

/// The deepest nesting of arrays and objects that is accepted, which keeps the stack of the recursive parser bounded.
const MAX_DEPTH: u32 = 200;

/// A JSON value and where it starts.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub pos: Pos,
    pub kind: Kind,
}

/// The kinds of JSON value.
#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Null,
    Bool(bool),
    /// The text of the number, as it was written.
    Number(String),
    /// The decoded string.
    Str(String),
    Array(Vec<Node>),
    /// The members in the order written.
    Object(Vec<Member>),
}

/// A member of an object.
#[derive(Debug, Clone, PartialEq)]
pub struct Member {
    pub key: String,
    pub key_pos: Pos,
    pub value: Node,
}

/// A problem with the text, and the place where the parser could not go on.
#[derive(Debug, Clone, PartialEq)]
pub struct JsonError {
    pub pos: Pos,
    pub message: String,
}

/// Parses a document: white space, one value and white space. A byte order mark at the start is skipped.
pub fn parse_document(text: &str) -> Result<Node, JsonError> {
    parse_from(text, 1)
}

/// Parses one line of a file of JSON lines. The positions say `line`, the number of the line in the file.
pub fn parse_line(text: &str, line: u32) -> Result<Node, JsonError> {
    parse_from(text, line)
}

fn parse_from(text: &str, line: u32) -> Result<Node, JsonError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut parser = Parser {
        chars: text.chars().collect(),
        at: 0,
        line,
        col: 1,
    };
    parser.skip_space();
    let node = parser.value(0)?;
    parser.skip_space();
    if parser.peek().is_some() {
        return Err(parser.error("Expected the end of the JSON after the value"));
    }
    Ok(node)
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

    fn pos(&self) -> Pos {
        Pos {
            line: self.line,
            col: self.col,
        }
    }

    fn error(&self, message: &str) -> JsonError {
        JsonError {
            pos: self.pos(),
            message: message.to_string(),
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

    fn skip_space(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t' | '\n' | '\r')) {
            self.bump();
        }
    }

    fn value(&mut self, depth: u32) -> Result<Node, JsonError> {
        let pos = self.pos();
        let kind = match self.peek() {
            Some('{') => self.object(depth)?,
            Some('[') => self.array(depth)?,
            Some('"') => Kind::Str(self.string()?),
            Some('-' | '0'..='9') => Kind::Number(self.number()?),
            Some('t') => self.word("true", Kind::Bool(true))?,
            Some('f') => self.word("false", Kind::Bool(false))?,
            Some('n') => self.word("null", Kind::Null)?,
            _ => return Err(self.error("Expected a JSON value")),
        };
        Ok(Node { pos, kind })
    }

    fn word(&mut self, word: &str, kind: Kind) -> Result<Kind, JsonError> {
        for expected in word.chars() {
            if self.peek() != Some(expected) {
                return Err(self.error("Expected a JSON value"));
            }
            self.bump();
        }
        Ok(kind)
    }

    fn enter(&self, depth: u32) -> Result<u32, JsonError> {
        if depth >= MAX_DEPTH {
            return Err(self.error("JSON is nested too deeply"));
        }
        Ok(depth + 1)
    }

    fn array(&mut self, depth: u32) -> Result<Kind, JsonError> {
        let depth = self.enter(depth)?;
        self.bump();
        let mut items = Vec::new();
        self.skip_space();
        if self.peek() == Some(']') {
            self.bump();
            return Ok(Kind::Array(items));
        }
        loop {
            self.skip_space();
            items.push(self.value(depth)?);
            self.skip_space();
            match self.peek() {
                Some(',') => {
                    self.bump();
                }
                Some(']') => {
                    self.bump();
                    return Ok(Kind::Array(items));
                }
                _ => return Err(self.error("Expected \",\" or \"]\"")),
            }
        }
    }

    fn object(&mut self, depth: u32) -> Result<Kind, JsonError> {
        let depth = self.enter(depth)?;
        self.bump();
        let mut members = Vec::new();
        self.skip_space();
        if self.peek() == Some('}') {
            self.bump();
            return Ok(Kind::Object(members));
        }
        loop {
            self.skip_space();
            if self.peek() != Some('"') {
                return Err(self.error("Expected a string for the name of a member"));
            }
            let key_pos = self.pos();
            let key = self.string()?;
            self.skip_space();
            if self.peek() != Some(':') {
                return Err(self.error("Expected \":\" after the name of a member"));
            }
            self.bump();
            self.skip_space();
            let value = self.value(depth)?;
            members.push(Member { key, key_pos, value });
            self.skip_space();
            match self.peek() {
                Some(',') => {
                    self.bump();
                }
                Some('}') => {
                    self.bump();
                    return Ok(Kind::Object(members));
                }
                _ => return Err(self.error("Expected \",\" or \"}\"")),
            }
        }
    }

    fn digits(&mut self, text: &mut String) -> usize {
        let mut count = 0;
        while let Some(c @ '0'..='9') = self.peek() {
            text.push(c);
            self.bump();
            count += 1;
        }
        count
    }

    fn number(&mut self) -> Result<String, JsonError> {
        let mut text = String::new();
        if self.peek() == Some('-') {
            text.push('-');
            self.bump();
        }
        match self.peek() {
            Some('0') => {
                text.push('0');
                self.bump();
                if matches!(self.peek(), Some('0'..='9')) {
                    return Err(self.error("A number may not have a leading zero"));
                }
            }
            Some('1'..='9') => {
                self.digits(&mut text);
            }
            _ => return Err(self.error("Expected a digit in the number")),
        }
        if self.peek() == Some('.') {
            text.push('.');
            self.bump();
            if self.digits(&mut text) == 0 {
                return Err(self.error("Expected a digit after the point"));
            }
        }
        if let Some(e @ ('e' | 'E')) = self.peek() {
            text.push(e);
            self.bump();
            if let Some(sign @ ('+' | '-')) = self.peek() {
                text.push(sign);
                self.bump();
            }
            if self.digits(&mut text) == 0 {
                return Err(self.error("Expected a digit in the exponent"));
            }
        }
        Ok(text)
    }

    fn hex4(&mut self) -> Result<u32, JsonError> {
        let mut code = 0;
        for _ in 0..4 {
            match self.peek().and_then(|c| c.to_digit(16)) {
                Some(d) => {
                    code = code * 16 + d;
                    self.bump();
                }
                None => return Err(self.error("Expected four hexadecimal digits after \\u")),
            }
        }
        Ok(code)
    }

    fn string(&mut self) -> Result<String, JsonError> {
        let start = self.pos();
        self.bump();
        let mut out = String::new();
        loop {
            let here = self.pos();
            let Some(c) = self.bump() else {
                return Err(JsonError {
                    pos: start,
                    message: "Unterminated string".to_string(),
                });
            };
            match c {
                '"' => return Ok(out),
                '\\' => out.push(self.escape(here)?),
                c if (c as u32) < 0x20 => {
                    return Err(JsonError {
                        pos: here,
                        message: "A control character in a string must be escaped".to_string(),
                    })
                }
                c => out.push(c),
            }
        }
    }

    /// Reads what follows a backslash. `here` is the place of the backslash.
    fn escape(&mut self, here: Pos) -> Result<char, JsonError> {
        let Some(c) = self.bump() else {
            return Err(self.error("Unterminated string"));
        };
        Ok(match c {
            '"' => '"',
            '\\' => '\\',
            '/' => '/',
            'b' => '\u{8}',
            'f' => '\u{c}',
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            'u' => {
                let first = self.hex4()?;
                let code = if (0xD800..0xDC00).contains(&first) {
                    // A high surrogate must be followed by the escape of a low one.
                    if self.peek() == Some('\\') && self.chars.get(self.at + 1) == Some(&'u') {
                        self.bump();
                        self.bump();
                        let second = self.hex4()?;
                        if !(0xDC00..0xE000).contains(&second) {
                            return Err(JsonError {
                                pos: here,
                                message: "A lone surrogate in a string".to_string(),
                            });
                        }
                        0x10000 + ((first - 0xD800) << 10) + (second - 0xDC00)
                    } else {
                        return Err(JsonError {
                            pos: here,
                            message: "A lone surrogate in a string".to_string(),
                        });
                    }
                } else if (0xDC00..0xE000).contains(&first) {
                    return Err(JsonError {
                        pos: here,
                        message: "A lone surrogate in a string".to_string(),
                    });
                } else {
                    first
                };
                char::from_u32(code).ok_or(JsonError {
                    pos: here,
                    message: "Not a character".to_string(),
                })?
            }
            _ => {
                return Err(JsonError {
                    pos: here,
                    message: format!("Unknown escape \\{}", shown(&c.to_string())),
                })
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(line: u32, col: u32) -> Pos {
        Pos { line, col }
    }

    fn ok(text: &str) -> Kind {
        parse_document(text).unwrap().kind
    }

    fn err(text: &str) -> (Pos, String) {
        let e = parse_document(text).unwrap_err();
        (e.pos, e.message)
    }

    #[test]
    fn an_unknown_escape_is_shown_without_a_control_character() {
        assert_eq!(err("\"\\\u{1b}\"").1, "Unknown escape \\\\u001b");
        assert_eq!(err("\"\\q\"").1, "Unknown escape \\q");
    }

    #[test]
    fn scalars() {
        assert_eq!(ok("null"), Kind::Null);
        assert_eq!(ok("true"), Kind::Bool(true));
        assert_eq!(ok(" false "), Kind::Bool(false));
        assert_eq!(ok("\"a\""), Kind::Str("a".into()));
        assert_eq!(ok("-1.50e+3"), Kind::Number("-1.50e+3".into()));
        assert_eq!(ok("0"), Kind::Number("0".into()));
        assert_eq!(ok("10E2"), Kind::Number("10E2".into()));
    }

    #[test]
    fn containers() {
        assert_eq!(ok("[]"), Kind::Array(vec![]));
        assert_eq!(ok("{ }"), Kind::Object(vec![]));
        let Kind::Array(items) = ok("[1, [2], {}]") else {
            panic!()
        };
        assert_eq!(items.len(), 3);
        assert_eq!(items[1].pos, p(1, 5));
    }

    #[test]
    fn repeated_keys_are_kept_in_order() {
        let Kind::Object(members) = ok("{\"a\": 1, \"b\": 2, \"a\": 3}") else {
            panic!()
        };
        let keys: Vec<_> = members.iter().map(|m| m.key.as_str()).collect();
        assert_eq!(keys, ["a", "b", "a"]);
    }

    #[test]
    fn escapes() {
        assert_eq!(
            ok(r#""\" \\ \/ \b \f \n \r \t A""#),
            Kind::Str("\" \\ / \u{8} \u{c} \n \r \t A".into())
        );
        assert_eq!(ok(r#""😀""#), Kind::Str("\u{1F600}".into()));
        assert_eq!(ok("\"é€\""), Kind::Str("é€".into()));
    }

    #[test]
    fn bad_strings() {
        assert_eq!(err(r#""\ud83d""#), (p(1, 2), "A lone surrogate in a string".into()));
        assert_eq!(err(r#""\ude00""#).1, "A lone surrogate in a string");
        assert_eq!(err(r#""\ud83dA""#).1, "A lone surrogate in a string");
        assert_eq!(err(r#""\x""#), (p(1, 2), "Unknown escape \\x".into()));
        assert_eq!(err(r#""\u12""#).1, "Expected four hexadecimal digits after \\u");
        assert_eq!(
            err("\"a\nb\""),
            (p(1, 3), "A control character in a string must be escaped".into())
        );
        // DEL is not a control character in the sense of RFC 8259, so it is allowed raw.
        assert_eq!(ok("\"a\u{7f}\""), Kind::Str("a\u{7f}".into()));
    }

    #[test]
    fn unterminated() {
        assert_eq!(err("  \"abc"), (p(1, 3), "Unterminated string".into()));
        assert_eq!(err("[1, 2").1, "Expected \",\" or \"]\"");
        assert_eq!(err("{\"a\": 1").1, "Expected \",\" or \"}\"");
    }

    #[test]
    fn bad_numbers() {
        assert_eq!(err("+1").1, "Expected a JSON value");
        assert_eq!(err(".5").1, "Expected a JSON value");
        assert_eq!(err("-").1, "Expected a digit in the number");
        assert_eq!(err("-a").1, "Expected a digit in the number");
        assert_eq!(err("01"), (p(1, 2), "A number may not have a leading zero".into()));
        assert_eq!(err("-01").1, "A number may not have a leading zero");
        assert_eq!(err("1.").1, "Expected a digit after the point");
        assert_eq!(err("1e").1, "Expected a digit in the exponent");
        assert_eq!(err("1e+").1, "Expected a digit in the exponent");
    }

    #[test]
    fn white_space_and_byte_order_mark() {
        assert_eq!(ok("\u{feff}\n\t 1 \r\n"), Kind::Number("1".into()));
    }

    #[test]
    fn empty_and_trailing() {
        assert_eq!(err(""), (p(1, 1), "Expected a JSON value".into()));
        assert_eq!(err(" \n "), (p(2, 2), "Expected a JSON value".into()));
        assert_eq!(
            err("1 2"),
            (p(1, 3), "Expected the end of the JSON after the value".into())
        );
        assert_eq!(err("[1,]").1, "Expected a JSON value");
        assert_eq!(err("{\"a\":1,}").1, "Expected a string for the name of a member");
        assert_eq!(err("{a: 1}").1, "Expected a string for the name of a member");
        assert_eq!(err("{\"a\" 1}").1, "Expected \":\" after the name of a member");
        assert_eq!(err("tru").1, "Expected a JSON value");
        assert_eq!(err("// c\n1").1, "Expected a JSON value");
    }

    #[test]
    fn positions() {
        let node = parse_document("{\n  \"a\": [1,\n    true],\n  \"bc\": null\n}").unwrap();
        assert_eq!(node.pos, p(1, 1));
        let Kind::Object(members) = node.kind else { panic!() };
        assert_eq!(members[0].key_pos, p(2, 3));
        assert_eq!(members[0].value.pos, p(2, 8));
        let Kind::Array(items) = &members[0].value.kind else {
            panic!()
        };
        assert_eq!(items[1].pos, p(3, 5));
        assert_eq!(members[1].key_pos, p(4, 3));
        assert_eq!(members[1].value.pos, p(4, 9));
    }

    #[test]
    fn columns_count_characters() {
        assert_eq!(err("[\"é€\", x]").0, p(1, 8));
    }

    #[test]
    fn a_line_has_the_number_of_its_line() {
        let e = parse_line("{\"a\": ?}", 5).unwrap_err();
        assert_eq!(e.pos, p(5, 7));
        assert_eq!(parse_line(" 1", 9).unwrap().pos, p(9, 2));
    }

    #[test]
    fn nesting_is_limited() {
        let deep = |n: usize| format!("{}{}", "[".repeat(n), "]".repeat(n));
        assert!(parse_document(&deep(200)).is_ok());
        assert_eq!(err(&deep(201)).1, "JSON is nested too deeply");
        assert_eq!(err(&"[".repeat(100000)).1, "JSON is nested too deeply");
    }

    #[test]
    fn what_convert_writes_is_read_back() {
        use crate::convert::Json;
        let tree = Json::Object(vec![(
            "array".into(),
            Json::Array(vec![
                Json::Object(vec![("nat".into(), Json::Str("1".into()))]),
                Json::Object(vec![("string".into(), Json::Str("a\"\\\n\t\u{1}é\u{1F600}".into()))]),
                Json::Number("-1.5e3".into()),
                Json::Bool(true),
                Json::Null,
                Json::Array(vec![]),
                Json::Object(vec![]),
            ]),
        )]);
        for compact in [true, false] {
            let text = tree.write(compact);
            assert_eq!(to_json(&parse_document(&text).unwrap()), tree, "{text}");
        }
    }

    fn to_json(node: &Node) -> crate::convert::Json {
        use crate::convert::Json;
        match &node.kind {
            Kind::Null => Json::Null,
            Kind::Bool(b) => Json::Bool(*b),
            Kind::Number(n) => Json::Number(n.clone()),
            Kind::Str(s) => Json::Str(s.clone()),
            Kind::Array(items) => Json::Array(items.iter().map(to_json).collect()),
            Kind::Object(ms) => Json::Object(ms.iter().map(|m| (m.key.clone(), to_json(&m.value))).collect()),
        }
    }
}
