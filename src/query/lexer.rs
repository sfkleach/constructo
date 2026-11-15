//! Converts the text of a query into tokens.
//!
//! The tokens of a query differ from those of a values file in three ways. There are operators and keywords. A sign is
//! never a part of a number, so that `n - 5` is a subtraction; the parser joins a sign to the number that follows it
//! with no space between, so that `-1` is still an `Int` literal. And after a `.` a number is only digits, so that
//! `v.0.1` is two positions, as in Rust.

use crate::diagnostics::Pos;
use crate::lexer::scan_string;

/// The words that are keywords in a query, which cannot be the name of a variable.
pub const KEYWORDS: [&str; 10] = [
    "if", "then", "elseif", "else", "endif", "match", "with", "endmatch", "as", "_",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tok {
    /// An identifier starting with an uppercase letter: a constructor, or a type after `as`.
    TypeName(String),
    /// An identifier starting with a lowercase letter or an underscore, which is not a keyword.
    Name(String),
    /// A keyword, such as `if` or `_`.
    Keyword(&'static str),
    /// A sigil and its word, as in `#values`.
    Sigil(String),
    /// An unsigned whole number, such as `42`.
    Nat(String),
    /// A number with a decimal point, such as `3.0` or `2.5e3`.
    Float(String),
    /// A string literal, with its escapes decoded.
    Str(String),
    /// Punctuation and operators, such as `(`, `..`, `=>` or `+=`.
    Punct(&'static str),
    /// A character that cannot start any token.
    Invalid(char),
    /// A malformed literal, with its message. The token's position is that of the offending character.
    Error(String),
    Eof,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub tok: Tok,
    pub pos: Pos,
    /// The position just after the token, which tells whether two tokens touch, as a sign and its number must.
    pub end: Pos,
}

/// The operators and punctuation, longest first so that `..` is found before `.`.
const PUNCTUATION: [&str; 36] = [
    "..", "::", "=>", "==", "!=", "<=", ">=", "&&", "||", "+=", "-=", "*=", "/=", "%=", "(", ")", "[", "]", "{", "}",
    ",", ";", ":", ".", "=", "<", ">", "+", "-", "*", "/", "%", "!", "|", "?", "@",
];

/// Splits `text` into tokens, always ending with a single `Tok::Eof`.
pub fn lex(text: &str) -> Vec<Token> {
    let chars: Vec<char> = text.chars().collect();
    let mut tokens: Vec<Token> = Vec::new();
    let (mut i, mut line, mut col) = (0usize, 1u32, 1u32);
    while i < chars.len() {
        let c = chars[i];
        let pos = Pos { line, col };
        let start = i;
        let tok = if c == '\n' {
            i += 1;
            line += 1;
            col = 1;
            continue;
        } else if c.is_whitespace() {
            i += 1;
            col += 1;
            continue;
        } else if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
                col += 1;
            }
            continue;
        } else if c.is_ascii_digit() {
            let after_dot = matches!(
                tokens.last(),
                Some(Token {
                    tok: Tok::Punct("."),
                    ..
                })
            );
            let (tok, end) = scan_number(&chars, i, after_dot);
            i = end;
            tok
        } else if c == '"' {
            match scan_string(&chars, i) {
                Ok((text, end)) => {
                    i = end;
                    Tok::Str(text)
                }
                Err((index, message, resume)) => {
                    let at = Pos {
                        line,
                        col: col + (index - start) as u32,
                    };
                    col += (resume - start) as u32;
                    i = resume;
                    tokens.push(Token {
                        tok: Tok::Error(message),
                        pos: at,
                        end: Pos { line, col },
                    });
                    continue;
                }
            }
        } else if c.is_ascii_alphabetic() || c == '_' {
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            match KEYWORDS.iter().find(|k| **k == word) {
                Some(keyword) => Tok::Keyword(keyword),
                None if c.is_ascii_uppercase() => Tok::TypeName(word),
                None => Tok::Name(word),
            }
        } else if c == '#' && chars.get(i + 1).is_some_and(|c| c.is_ascii_alphabetic()) {
            i += 1;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            Tok::Sigil(chars[start + 1..i].iter().collect())
        } else {
            match PUNCTUATION
                .iter()
                .find(|p| p.chars().enumerate().all(|(k, pc)| chars.get(i + k) == Some(&pc)))
            {
                Some(p) => {
                    i += p.chars().count();
                    Tok::Punct(p)
                }
                None => {
                    i += 1;
                    Tok::Invalid(c)
                }
            }
        };
        col += (i - start) as u32;
        tokens.push(Token {
            tok,
            pos,
            end: Pos { line, col },
        });
    }
    let end = Pos { line, col };
    tokens.push(Token {
        tok: Tok::Eof,
        pos: end,
        end,
    });
    tokens
}

/// Scans an unsigned number at `start`, returning the token and the index after it. A number is a `Float` if it has a
/// decimal point followed by a digit, with an optional exponent after the fraction, as in a values file. After a `.`
/// only digits are taken, so that `v.0.1` reads as two positions.
fn scan_number(chars: &[char], start: usize, after_dot: bool) -> (Tok, usize) {
    let digits_from = |mut i: usize| {
        while i < chars.len() && chars[i].is_ascii_digit() {
            i += 1;
        }
        i
    };
    let text = |end: usize| chars[start..end].iter().collect::<String>();
    let after_int = digits_from(start);
    let fraction = chars.get(after_int) == Some(&'.') && chars.get(after_int + 1).is_some_and(|c| c.is_ascii_digit());
    if after_dot || !fraction {
        return (Tok::Nat(text(after_int)), after_int);
    }
    let mut end = digits_from(after_int + 1);
    if matches!(chars.get(end), Some('e') | Some('E')) {
        let mut exponent = end + 1;
        if matches!(chars.get(exponent), Some('+') | Some('-')) {
            exponent += 1;
        }
        let after_exponent = digits_from(exponent);
        if after_exponent > exponent {
            end = after_exponent;
        }
    }
    (Tok::Float(text(end)), end)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(text: &str) -> Vec<Tok> {
        lex(text).into_iter().map(|t| t.tok).collect()
    }

    #[test]
    fn lexes_a_query() {
        assert_eq!(
            toks("#values.map(|m| m.x)"),
            vec![
                Tok::Sigil("values".into()),
                Tok::Punct("."),
                Tok::Name("map".into()),
                Tok::Punct("("),
                Tok::Punct("|"),
                Tok::Name("m".into()),
                Tok::Punct("|"),
                Tok::Name("m".into()),
                Tok::Punct("."),
                Tok::Name("x".into()),
                Tok::Punct(")"),
                Tok::Eof
            ]
        );
    }

    #[test]
    fn the_binding_of_a_pattern_is_not_a_sigil() {
        assert_eq!(
            toks("p@Poly"),
            vec![
                Tok::Name("p".into()),
                Tok::Punct("@"),
                Tok::TypeName("Poly".into()),
                Tok::Eof
            ]
        );
    }

    #[test]
    fn a_sign_is_not_part_of_a_number() {
        assert_eq!(
            toks("n - 5 -1 2.5e3"),
            vec![
                Tok::Name("n".into()),
                Tok::Punct("-"),
                Tok::Nat("5".into()),
                Tok::Punct("-"),
                Tok::Nat("1".into()),
                Tok::Float("2.5e3".into()),
                Tok::Eof
            ]
        );
    }

    #[test]
    fn positions_after_a_dot_are_digits_only() {
        assert_eq!(
            toks("v.0.1"),
            vec![
                Tok::Name("v".into()),
                Tok::Punct("."),
                Tok::Nat("0".into()),
                Tok::Punct("."),
                Tok::Nat("1".into()),
                Tok::Eof
            ]
        );
    }

    #[test]
    fn longest_punctuation_wins() {
        assert_eq!(
            toks("..c => a == b += 1"),
            vec![
                Tok::Punct(".."),
                Tok::Name("c".into()),
                Tok::Punct("=>"),
                Tok::Name("a".into()),
                Tok::Punct("=="),
                Tok::Name("b".into()),
                Tok::Punct("+="),
                Tok::Nat("1".into()),
                Tok::Eof
            ]
        );
    }

    #[test]
    fn keywords_and_names() {
        assert_eq!(
            toks("if x then _ endif Move"),
            vec![
                Tok::Keyword("if"),
                Tok::Name("x".into()),
                Tok::Keyword("then"),
                Tok::Keyword("_"),
                Tok::Keyword("endif"),
                Tok::TypeName("Move".into()),
                Tok::Eof
            ]
        );
    }

    #[test]
    fn tokens_know_where_they_end() {
        let tokens = lex("-1 - 1");
        assert_eq!(tokens[0].end, tokens[1].pos);
        assert_ne!(tokens[2].end, tokens[3].pos);
    }
}
