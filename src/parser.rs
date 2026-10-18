//! A recursive-descent parser for data-type declarations.
//!
//! The parser stops at the first syntax error: there is no error recovery.

use crate::ast::{EnumDecl, Field, Type, TypeParam, Variant, VariantBody};
use crate::diagnostics::Pos;
use crate::lexer::{lex, Tok, Token};

/// The first syntax error found in a text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxError {
    pub pos: Pos,
    pub message: String,
}

type Result<T> = std::result::Result<T, SyntaxError>;

/// Parses the text of a types file into its declarations.
pub fn parse(text: &str) -> Result<Vec<EnumDecl>> {
    let mut parser = Parser {
        tokens: lex(text),
        index: 0,
    };
    let mut decls = Vec::new();
    while parser.peek() != &Tok::Eof {
        decls.push(parser.enum_decl()?);
    }
    Ok(decls)
}

struct Parser {
    tokens: Vec<Token>,
    index: usize,
}

impl Parser {
    fn peek(&self) -> &Tok {
        &self.tokens[self.index].tok
    }

    fn pos(&self) -> Pos {
        self.tokens[self.index].pos
    }

    /// Advances past the current token. The final `Eof` token is never consumed.
    fn bump(&mut self) {
        if self.index + 1 < self.tokens.len() {
            self.index += 1;
        }
    }

    fn error<T>(&self, expected: &str) -> Result<T> {
        let message = match self.peek() {
            Tok::Invalid(c) => format!("Unexpected character '{}'", c),
            _ => format!("Expected {}", expected),
        };
        Err(SyntaxError {
            pos: self.pos(),
            message,
        })
    }

    fn expect(&mut self, tok: Tok, expected: &str) -> Result<()> {
        if self.peek() == &tok {
            self.bump();
            Ok(())
        } else {
            self.error(expected)
        }
    }

    fn type_name(&mut self, expected: &str) -> Result<(String, Pos)> {
        if let Tok::TypeName(name) = self.peek() {
            let result = (name.clone(), self.pos());
            self.bump();
            Ok(result)
        } else {
            self.error(expected)
        }
    }

    fn enum_decl(&mut self) -> Result<EnumDecl> {
        self.expect(Tok::Enum, "'enum'")?;
        let (name, pos) = self.type_name("type name")?;
        let params = if self.peek() == &Tok::Lt {
            self.type_params()?
        } else {
            Vec::new()
        };
        self.expect(Tok::Eq, "'='")?;
        let mut variants = vec![self.variant()?];
        loop {
            match self.peek() {
                Tok::Comma => {
                    self.bump();
                    // A trailing comma is allowed before the semi-colon.
                    if self.peek() == &Tok::Semi {
                        break;
                    }
                    variants.push(self.variant()?);
                }
                Tok::Semi => break,
                _ => return self.error("comma or semi-colon"),
            }
        }
        self.expect(Tok::Semi, "semi-colon")?;
        Ok(EnumDecl {
            name,
            pos,
            params,
            variants,
        })
    }

    fn type_params(&mut self) -> Result<Vec<TypeParam>> {
        self.expect(Tok::Lt, "'<'")?;
        let mut params = Vec::new();
        loop {
            params.push(self.type_param()?);
            match self.peek() {
                Tok::Comma => self.bump(),
                Tok::Gt => break,
                _ => return self.error("comma or '>'"),
            }
        }
        self.expect(Tok::Gt, "'>'")?;
        Ok(params)
    }

    /// A type parameter is a type name, optionally preceded by an underscore.
    fn type_param(&mut self) -> Result<TypeParam> {
        let pos = self.pos();
        match self.peek() {
            Tok::TypeName(name) => {
                let name = name.clone();
                self.bump();
                Ok(TypeParam {
                    name,
                    pos,
                    underscore: false,
                })
            }
            Tok::Name(name) if name.chars().nth(1).is_some_and(|c| c.is_ascii_uppercase()) && name.starts_with('_') => {
                let name = name.clone();
                self.bump();
                Ok(TypeParam {
                    name,
                    pos,
                    underscore: true,
                })
            }
            _ => self.error("type parameter name"),
        }
    }

    fn variant(&mut self) -> Result<Variant> {
        let (name, pos) = self.type_name("constructor name")?;
        let body = match self.peek() {
            Tok::LBrace => VariantBody::Record(self.record_body()?),
            Tok::LParen => VariantBody::Tuple(self.tuple_body()?),
            _ => VariantBody::Unit,
        };
        Ok(Variant { name, pos, body })
    }

    fn record_body(&mut self) -> Result<Vec<Field>> {
        self.expect(Tok::LBrace, "'{'")?;
        let mut fields = Vec::new();
        while self.peek() != &Tok::RBrace {
            let pos = self.pos();
            let name = match self.peek() {
                Tok::Name(name) => name.clone(),
                _ => return self.error("field name"),
            };
            self.bump();
            self.expect(Tok::Colon, "':'")?;
            let ty = self.ty()?;
            fields.push(Field { name, pos, ty });
            match self.peek() {
                Tok::Comma => self.bump(),
                Tok::RBrace => {}
                _ => return self.error("comma or '}'"),
            }
        }
        self.expect(Tok::RBrace, "'}'")?;
        Ok(fields)
    }

    fn tuple_body(&mut self) -> Result<Vec<Type>> {
        self.expect(Tok::LParen, "'('")?;
        let mut types = Vec::new();
        while self.peek() != &Tok::RParen {
            types.push(self.ty()?);
            match self.peek() {
                Tok::Comma => self.bump(),
                Tok::RParen => {}
                _ => return self.error("comma or ')'"),
            }
        }
        self.expect(Tok::RParen, "')'")?;
        Ok(types)
    }

    fn ty(&mut self) -> Result<Type> {
        match self.peek() {
            Tok::TypeName(_) => {
                let (name, pos) = self.type_name("type")?;
                let mut args = Vec::new();
                if self.peek() == &Tok::Lt {
                    self.bump();
                    loop {
                        args.push(self.ty()?);
                        match self.peek() {
                            Tok::Comma => self.bump(),
                            Tok::Gt => break,
                            _ => return self.error("comma or '>'"),
                        }
                    }
                    self.expect(Tok::Gt, "'>'")?;
                }
                Ok(Type::Ref { name, pos, args })
            }
            Tok::LBracket => {
                self.bump();
                let element = self.ty()?;
                self.expect(Tok::RBracket, "']'")?;
                Ok(Type::Array(Box::new(element)))
            }
            Tok::LBrace => Ok(Type::Record(self.record_body()?)),
            _ => self.error("type"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err(text: &str) -> (u32, u32, String) {
        let e = parse(text).unwrap_err();
        (e.pos.line, e.pos.col, e.message)
    }

    #[test]
    fn parses_a_generic_recursive_enum() {
        let decls = parse("enum List<T> = Nil, Cons { head: T, tail: List<T> };").unwrap();
        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0].params.len(), 1);
        assert_eq!(decls[0].variants.len(), 2);
    }

    #[test]
    fn accepts_empty_input_and_trailing_commas() {
        assert!(parse("// Nothing here.\n").unwrap().is_empty());
        assert!(parse("enum A = X(Int,), Y {a: Int,},;").is_ok());
        assert!(parse("enum A = Ping(), Pong {};").is_ok());
    }

    #[test]
    fn accepts_underscore_type_parameters() {
        let decls = parse("enum Phantom<_T, U> = P(U);").unwrap();
        assert!(decls[0].params[0].underscore);
        assert!(!decls[0].params[1].underscore);
    }

    #[test]
    fn rejects_bad_underscore_parameters() {
        assert!(parse("enum A<_> = X;").is_err());
        assert!(parse("enum A<_t> = X;").is_err());
    }

    #[test]
    fn reports_missing_separators_at_the_next_token() {
        assert_eq!(
            err("enum A = X\nenum B = Y;"),
            (2, 1, "Expected comma or semi-colon".to_string())
        );
        assert_eq!(err("enum A = X {a: Int b: Int};").2, "Expected comma or '}'");
    }

    #[test]
    fn reports_other_syntax_errors() {
        assert_eq!(err("enum A = ;").2, "Expected constructor name");
        assert_eq!(err("enum a = X;").2, "Expected type name");
        assert_eq!(err("enum A = X([]);").2, "Expected type");
        assert_eq!(err("enum L{T} = Nil;").2, "Expected '='");
        assert_eq!(err("enum A = X; B").2, "Expected 'enum'");
        assert_eq!(err("enum A = X(Int").2, "Expected comma or ')'");
        assert_eq!(err("enum A = X #").2, "Unexpected character '#'");
    }
}
