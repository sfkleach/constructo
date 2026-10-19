//! Converts the text of a types file into tokens.

use crate::diagnostics::Pos;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tok {
    Enum,
    /// An identifier starting with an uppercase letter.
    TypeName(String),
    /// An identifier starting with a lowercase letter or underscore, other than `enum`.
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
    /// A character that cannot start any token. The parser reports it as a syntax error.
    Invalid(char),
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
        } else if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
                col += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let tok = if word == "enum" {
                Tok::Enum
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
}
