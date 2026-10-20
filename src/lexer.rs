//! Converts the text of a types file into tokens.

use crate::diagnostics::Pos;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tok {
    Enum,
    Const,
    /// An identifier starting with an uppercase letter.
    TypeName(String),
    /// An identifier starting with a lowercase letter or underscore, other than `enum` and `const`.
    Name(String),
    Eq,
    Comma,
    Semi,
    Lt,
    Gt,
    LBrace,
    RBrace,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Colon,
    /// An unsigned integer literal, such as `42`, with its text unchanged.
    Nat(String),
    /// A signed integer literal, such as `+8` or `-1`.
    Int(String),
    /// A floating-point literal, such as `3.0` or `-2.5e3`.
    Float(String),
    /// A string literal, with its escapes decoded.
    Str(String),
    /// A character that cannot start any token. The parser reports it as a syntax error.
    Invalid(char),
    /// A malformed literal. The token's position is that of the offending character.
    Error(String),
    Eof,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub tok: Tok,
    pub pos: Pos,
}

/// Splits `text` into tokens, always ending with a single `Tok::Eof`.
pub fn lex(text: &str) -> Vec<Token> {
    let chars: Vec<char> = text.chars().collect();
    let mut tokens = Vec::new();
    let (mut i, mut line, mut col) = (0usize, 1u32, 1u32);
    while i < chars.len() {
        let c = chars[i];
        let pos = Pos { line, col };
        if c == '\n' {
            i += 1;
            line += 1;
            col = 1;
        } else if c.is_whitespace() {
            i += 1;
            col += 1;
        } else if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
                col += 1;
            }
        } else if c.is_ascii_digit() || c == '+' || c == '-' {
            let (tok, end, error_at) = scan_number(&chars, i);
            let (tok, pos) = match error_at {
                Some((index, message)) => (
                    Tok::Error(message),
                    Pos {
                        line,
                        col: col + (index - i) as u32,
                    },
                ),
                None => (tok, pos),
            };
            tokens.push(Token { tok, pos });
            col += (end - i) as u32;
            i = end;
        } else if c == '"' {
            let (tok, end, error_at) = match scan_string(&chars, i) {
                Ok((text, end)) => (Tok::Str(text), end, None),
                Err((index, message, resume)) => (Tok::Error(message), resume, Some(index)),
            };
            let pos = match error_at {
                Some(index) => Pos {
                    line,
                    col: col + (index - i) as u32,
                },
                None => pos,
            };
            tokens.push(Token { tok, pos });
            col += (end - i) as u32;
            i = end;
        } else if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
                col += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let tok = if word == "enum" {
                Tok::Enum
            } else if word == "const" {
                Tok::Const
            } else if c.is_ascii_uppercase() {
                Tok::TypeName(word)
            } else {
                Tok::Name(word)
            };
            tokens.push(Token { tok, pos });
        } else {
            let tok = match c {
                '=' => Tok::Eq,
                ',' => Tok::Comma,
                ';' => Tok::Semi,
                '<' => Tok::Lt,
                '>' => Tok::Gt,
                '{' => Tok::LBrace,
                '}' => Tok::RBrace,
                '(' => Tok::LParen,
                ')' => Tok::RParen,
                '[' => Tok::LBracket,
                ']' => Tok::RBracket,
                ':' => Tok::Colon,
                other => Tok::Invalid(other),
            };
            tokens.push(Token { tok, pos });
            i += 1;
            col += 1;
        }
    }
    tokens.push(Token {
        tok: Tok::Eof,
        pos: Pos { line, col },
    });
    tokens
}

/// Scans a number starting at `start`, which holds a digit or a sign.
///
/// Returns the token, the index after the text that was consumed, and for a malformed number the index of the
/// character where the error was found and the message. A malformed number consumes the text up to that character.
fn scan_number(chars: &[char], start: usize) -> (Tok, usize, Option<(usize, String)>) {
    let digits_from = |mut i: usize| {
        while i < chars.len() && chars[i].is_ascii_digit() {
            i += 1;
        }
        i
    };
    let signed = chars[start] == '+' || chars[start] == '-';
    let after_int = digits_from(if signed { start + 1 } else { start });
    if after_int == start + usize::from(signed) {
        let error = (start + 1, "Expected a digit after the sign".to_string());
        return (Tok::Nat(String::new()), start + 1, Some(error));
    }
    let text = |end: usize| chars[start..end].iter().collect::<String>();
    if chars.get(after_int) != Some(&'.') {
        let tok = if signed {
            Tok::Int(text(after_int))
        } else {
            Tok::Nat(text(after_int))
        };
        return (tok, after_int, None);
    }
    let after_fraction = digits_from(after_int + 1);
    if after_fraction == after_int + 1 {
        let error = (after_int + 1, "Expected a digit after the decimal point".to_string());
        return (Tok::Nat(String::new()), after_int + 1, Some(error));
    }
    let mut end = after_fraction;
    if matches!(chars.get(end), Some('e') | Some('E')) {
        let mut exponent = end + 1;
        if matches!(chars.get(exponent), Some('+') | Some('-')) {
            exponent += 1;
        }
        let after_exponent = digits_from(exponent);
        if after_exponent == exponent {
            let error = (exponent, "Expected a digit in the exponent".to_string());
            return (Tok::Nat(String::new()), exponent, Some(error));
        }
        end = after_exponent;
    }
    (Tok::Float(text(end)), end, None)
}

/// Scans a string literal whose opening quote is at `start`.
///
/// Returns the decoded text and the index after the closing quote. For a malformed string it returns the index of the
/// error, the message, and the index at which lexing should resume.
fn scan_string(chars: &[char], start: usize) -> Result<(String, usize), (usize, String, usize)> {
    let mut text = String::new();
    let mut i = start + 1;
    loop {
        match chars.get(i) {
            None | Some('\n') => return Err((start, "Unterminated string".to_string(), i)),
            Some('"') => return Ok((text, i + 1)),
            Some('\\') => {
                let escape = chars.get(i + 1).copied();
                let simple = match escape {
                    Some('"') => Some('"'),
                    Some('\\') => Some('\\'),
                    Some('/') => Some('/'),
                    Some('b') => Some('\u{8}'),
                    Some('f') => Some('\u{c}'),
                    Some('n') => Some('\n'),
                    Some('r') => Some('\r'),
                    Some('t') => Some('\t'),
                    _ => None,
                };
                if let Some(c) = simple {
                    text.push(c);
                    i += 2;
                } else if escape == Some('u') {
                    match scan_unicode_escape(chars, i) {
                        Some((c, next)) => {
                            text.push(c);
                            i = next;
                        }
                        None => return Err((i, "Invalid Unicode escape".to_string(), i + 2)),
                    }
                } else {
                    return Err((i, "Invalid escape in string".to_string(), i + 2));
                }
            }
            Some(c) if c.is_control() => return Err((i, "Control character in string".to_string(), i + 1)),
            Some(c) => {
                text.push(*c);
                i += 1;
            }
        }
    }
}

/// Reads the four hex digits of a `\u` escape at `at`, pairing a high surrogate with the low surrogate that must
/// follow it. Returns the character and the index after the escape, or `None` if the escape is not valid.
fn scan_unicode_escape(chars: &[char], at: usize) -> Option<(char, usize)> {
    let unit = |at: usize| -> Option<u32> {
        let digits: String = chars.get(at + 2..at + 6)?.iter().collect();
        if digits.len() == 4 && digits.chars().all(|c| c.is_ascii_hexdigit()) {
            u32::from_str_radix(&digits, 16).ok()
        } else {
            None
        }
    };
    let first = unit(at)?;
    match first {
        0xD800..=0xDBFF => {
            // A high surrogate is only valid as the first half of a pair.
            if chars.get(at + 6) != Some(&'\\') || chars.get(at + 7) != Some(&'u') {
                return None;
            }
            let second = unit(at + 6)?;
            if !(0xDC00..=0xDFFF).contains(&second) {
                return None;
            }
            let c = char::from_u32(0x10000 + ((first - 0xD800) << 10) + (second - 0xDC00))?;
            Some((c, at + 12))
        }
        0xDC00..=0xDFFF => None,
        _ => Some((char::from_u32(first)?, at + 6)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(text: &str) -> Vec<Tok> {
        lex(text).into_iter().map(|t| t.tok).collect()
    }

    #[test]
    fn classifies_identifiers() {
        assert_eq!(
            toks("enum Foo bar _t"),
            vec![
                Tok::Enum,
                Tok::TypeName("Foo".into()),
                Tok::Name("bar".into()),
                Tok::Name("_t".into()),
                Tok::Eof
            ]
        );
    }

    #[test]
    fn skips_comments_and_tracks_positions() {
        let tokens = lex("// note\n  A;");
        assert_eq!(tokens[0].pos, Pos { line: 2, col: 3 });
        assert_eq!(tokens[1].tok, Tok::Semi);
        assert_eq!(tokens[1].pos, Pos { line: 2, col: 4 });
    }

    #[test]
    fn reports_invalid_characters() {
        assert_eq!(toks("é"), vec![Tok::Invalid('é'), Tok::Eof]);
    }

    #[test]
    fn lexes_numbers() {
        assert_eq!(
            toks("42 +8 -1 3.0 -2.5e3 1.5E+2"),
            vec![
                Tok::Nat("42".into()),
                Tok::Int("+8".into()),
                Tok::Int("-1".into()),
                Tok::Float("3.0".into()),
                Tok::Float("-2.5e3".into()),
                Tok::Float("1.5E+2".into()),
                Tok::Eof
            ]
        );
    }

    #[test]
    fn reports_malformed_numbers_at_the_offending_character() {
        let error = |text: &str| {
            let t = lex(text).remove(0);
            match t.tok {
                Tok::Error(m) => (t.pos.col, m),
                other => panic!("expected an error, got {:?}", other),
            }
        };
        assert_eq!(error("3."), (3, "Expected a digit after the decimal point".to_string()));
        assert_eq!(error("+x"), (2, "Expected a digit after the sign".to_string()));
        assert_eq!(error("1.5e"), (5, "Expected a digit in the exponent".to_string()));
    }

    #[test]
    fn lexes_strings_and_decodes_escapes() {
        assert_eq!(
            toks(r#""esc \" \\ \n \u00e9 é""#)[0],
            Tok::Str("esc \" \\ \n é é".into())
        );
        assert_eq!(toks(r#""\ud83d\ude00""#)[0], Tok::Str("😀".into()));
        assert_eq!(toks(r#""""#)[0], Tok::Str(String::new()));
    }

    #[test]
    fn reports_malformed_strings() {
        let error = |text: &str| {
            let t = lex(text).remove(0);
            match t.tok {
                Tok::Error(m) => (t.pos.col, m),
                other => panic!("expected an error, got {:?}", other),
            }
        };
        assert_eq!(error("\"oops"), (1, "Unterminated string".to_string()));
        assert_eq!(error("\"oops\nmore\""), (1, "Unterminated string".to_string()));
        assert_eq!(error(r#""a\qb""#), (3, "Invalid escape in string".to_string()));
        assert_eq!(error(r#""\u12""#), (2, "Invalid Unicode escape".to_string()));
        assert_eq!(error(r#""\ud800""#), (2, "Invalid Unicode escape".to_string()));
        assert_eq!(error("\"a\u{1}b\""), (3, "Control character in string".to_string()));
    }
}
