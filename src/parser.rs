//! A recursive-descent parser for data-type declarations.
//!
//! The parser stops at the first syntax error: there is no error recovery.

use crate::ast::{
    ConstDecl, EnumDecl, Field, Member, TaggedBody, Type, TypeParam, TypesFile, Value, ValueKind, Variant, VariantBody,
};
use crate::diagnostics::Pos;
use crate::lexer::{lex, Tok, Token};

/// The first syntax error found in a text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxError {
    pub pos: Pos,
    pub message: String,
    /// The enum or const being parsed when the error occurred, if its name had been read, e.g. `enum Shape`.
    pub context: Option<String>,
}

type Result<T> = std::result::Result<T, SyntaxError>;

/// The deepest nesting of arrays, records, tuples and type arguments that the parser accepts.
const MAX_DEPTH: u32 = 200;

/// The kind of a file, which is decided by its first token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    Types,
    Values,
}

impl SourceKind {
    /// The name used for the kind in messages.
    pub fn label(self) -> &'static str {
        match self {
            SourceKind::Types => "data-types",
            SourceKind::Values => "data-values",
        }
    }
}

/// The contents of a successfully parsed file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parsed {
    Types(TypesFile),
    Values(Vec<Value>),
}

/// Parses the text of a types file into its declarations.
pub fn parse(text: &str) -> Result<TypesFile> {
    Parser::new(text).types()
}

/// Parses the text of a values file into its values.
pub fn parse_values(text: &str) -> Result<Vec<Value>> {
    Parser::new(text).values()
}

/// Classifies a file by its first token and parses it with the matching grammar.
///
/// A types file always starts with `enum` or `const` and a values file never does. A file with no tokens is treated as a types
/// file, which is how it was handled before values files were supported. The kind is returned even when parsing fails.
pub fn parse_file(text: &str) -> (SourceKind, Result<Parsed>) {
    let mut parser = Parser::new(text);
    let kind = match parser.peek() {
        Tok::Enum | Tok::Const | Tok::Eof => SourceKind::Types,
        _ => SourceKind::Values,
    };
    let parsed = match kind {
        SourceKind::Types => parser.types().map(Parsed::Types),
        SourceKind::Values => parser.values().map(Parsed::Values),
    };
    (kind, parsed)
}

struct Parser {
    tokens: Vec<Token>,
    index: usize,
    context: Option<String>,
    depth: u32,
}

impl Parser {
    fn new(text: &str) -> Parser {
        Parser {
            tokens: lex(text),
            index: 0,
            context: None,
            depth: 0,
        }
    }

    fn types(&mut self) -> Result<TypesFile> {
        let mut file = TypesFile::default();
        while self.peek() != &Tok::Eof {
            // Errors before the name has been read are not attributed to the previous declaration.
            self.context = None;
            match self.peek() {
                Tok::Const => file.consts.push(self.const_decl()?),
                Tok::Enum => file.enums.push(self.enum_decl()?),
                _ => return self.error("'enum' or 'const'"),
            }
        }
        Ok(file)
    }

    /// Runs a parser for a nested construct. The limit is a defensive check: the parser is recursive, so without it
    /// deeply nested input would overflow the stack and crash instead of being reported as a syntax error.
    fn nested<T>(&mut self, parse: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        if self.depth >= MAX_DEPTH {
            return Err(SyntaxError {
                pos: self.pos(),
                message: "Nesting is too deep".to_string(),
                context: self.context.clone(),
            });
        }
        self.depth += 1;
        let result = parse(self);
        self.depth -= 1;
        result
    }

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
            Tok::Error(message) => message.clone(),
            _ => format!("Expected {}", expected),
        };
        Err(SyntaxError {
            pos: self.pos(),
            message,
            context: self.context.clone(),
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
        self.context = Some(format!("enum {}", name));
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

    /// `const name: Type = value;` The name may be an identifier of either case, other than `null`.
    fn const_decl(&mut self) -> Result<ConstDecl> {
        self.expect(Tok::Const, "'const'")?;
        let pos = self.pos();
        let name = match self.peek() {
            Tok::TypeName(name) => name.clone(),
            Tok::Name(name) if name == "null" => {
                return Err(SyntaxError {
                    pos,
                    message: "null is a literal and cannot be the name of a const".to_string(),
                    context: None,
                })
            }
            Tok::Name(name) => name.clone(),
            _ => return self.error("const name"),
        };
        self.bump();
        self.context = Some(format!("const {}", name));
        self.expect(Tok::Colon, "':'")?;
        let ty = self.ty()?;
        self.expect(Tok::Eq, "'='")?;
        let value = self.value()?;
        self.expect(Tok::Semi, "semi-colon")?;
        Ok(ConstDecl { name, pos, ty, value })
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
        self.nested(Self::ty_unchecked)
    }

    fn ty_unchecked(&mut self) -> Result<Type> {
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

impl Parser {
    /// A values file is a sequence of values separated by semi-colons, with an optional final semi-colon.
    fn values(&mut self) -> Result<Vec<Value>> {
        let mut values = Vec::new();
        if self.peek() == &Tok::Eof {
            return Ok(values);
        }
        loop {
            values.push(self.value()?);
            match self.peek() {
                Tok::Semi => {
                    self.bump();
                    if self.peek() == &Tok::Eof {
                        break;
                    }
                }
                Tok::Eof => break,
                _ => return self.error("semi-colon"),
            }
        }
        Ok(values)
    }

    fn value(&mut self) -> Result<Value> {
        self.nested(Self::value_unchecked)
    }

    fn value_unchecked(&mut self) -> Result<Value> {
        let pos = self.pos();
        let kind = match self.peek().clone() {
            Tok::Nat(text) => self.literal(ValueKind::Nat(text)),
            Tok::Int(text) => self.literal(ValueKind::Int(text)),
            Tok::Float(text) => self.literal(ValueKind::Float(text)),
            Tok::Str(text) => self.literal(ValueKind::Str(text)),
            Tok::Name(name) if name == "null" => self.literal(ValueKind::Null),
            // A lowercase name can only refer to a const, so it takes no body.
            Tok::Name(name) => {
                self.bump();
                ValueKind::Tagged { name, body: None }
            }
            Tok::LBrace => ValueKind::Record(self.members()?),
            Tok::LBracket => ValueKind::Array(self.array()?),
            Tok::TypeName(name) => {
                self.bump();
                let body = match self.peek() {
                    Tok::LBrace => Some(TaggedBody::Record(self.members()?)),
                    Tok::LParen => Some(TaggedBody::Tuple(self.tuple()?)),
                    _ => None,
                };
                ValueKind::Tagged { name, body }
            }
            _ => return self.error("value"),
        };
        Ok(Value { pos, kind })
    }

    /// Consumes the current token, which has already been read as a literal.
    fn literal(&mut self, kind: ValueKind) -> ValueKind {
        self.bump();
        kind
    }

    fn members(&mut self) -> Result<Vec<Member>> {
        self.expect(Tok::LBrace, "'{'")?;
        let mut members = Vec::new();
        while self.peek() != &Tok::RBrace {
            let pos = self.pos();
            let name = match self.peek() {
                Tok::Name(name) => name.clone(),
                _ => return self.error("field name"),
            };
            self.bump();
            self.expect(Tok::Colon, "':'")?;
            let value = self.value()?;
            members.push(Member { name, pos, value });
            match self.peek() {
                Tok::Comma => self.bump(),
                Tok::RBrace => {}
                _ => return self.error("comma or '}'"),
            }
        }
        self.expect(Tok::RBrace, "'}'")?;
        Ok(members)
    }

    fn tuple(&mut self) -> Result<Vec<Value>> {
        self.expect(Tok::LParen, "'('")?;
        self.value_list(Tok::RParen, "')'")
    }

    fn array(&mut self) -> Result<Vec<Value>> {
        self.expect(Tok::LBracket, "'['")?;
        self.value_list(Tok::RBracket, "']'")
    }

    /// Parses comma-separated values, with an optional trailing comma, up to and including the closing token.
    fn value_list(&mut self, close: Tok, close_name: &str) -> Result<Vec<Value>> {
        let mut values = Vec::new();
        while self.peek() != &close {
            values.push(self.value()?);
            match self.peek() {
                Tok::Comma => self.bump(),
                tok if tok == &close => {}
                _ => return self.error(&format!("comma or {}", close_name)),
            }
        }
        self.expect(close, close_name)?;
        Ok(values)
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
        let decls = parse("enum List<T> = Nil, Cons { head: T, tail: List<T> };")
            .unwrap()
            .enums;
        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0].params.len(), 1);
        assert_eq!(decls[0].variants.len(), 2);
    }

    #[test]
    fn accepts_empty_input_and_trailing_commas() {
        assert_eq!(parse("// Nothing here.\n").unwrap(), TypesFile::default());
        assert!(parse("enum A = X(Int,), Y {a: Int,},;").is_ok());
        assert!(parse("enum A = Ping(), Pong {};").is_ok());
    }

    #[test]
    fn accepts_underscore_type_parameters() {
        let decls = parse("enum Phantom<_T, U> = P(U);").unwrap().enums;
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
    fn attributes_errors_to_the_enclosing_enum_once_it_is_named() {
        let context_of = |text: &str| parse(text).unwrap_err().context;
        assert_eq!(context_of("enum Shape = Circle { r: }"), Some("enum Shape".to_string()));
        assert_eq!(context_of("enum Shape"), Some("enum Shape".to_string()));
        assert_eq!(context_of("enum a = X;"), None);
        assert_eq!(context_of("enum A = X; B"), None);
        assert_eq!(context_of("enum A = X; enum B = ;"), Some("enum B".to_string()));
        assert_eq!(context_of("const N: Nat = ;"), Some("const N".to_string()));
        assert_eq!(context_of("const N Nat = 1;"), Some("const N".to_string()));
        assert_eq!(context_of("const : Nat = 1;"), None);
        assert_eq!(context_of("const A: Nat = 1; enum"), None);
    }

    #[test]
    fn reports_other_syntax_errors() {
        assert_eq!(err("enum A = ;").2, "Expected constructor name");
        assert_eq!(err("enum a = X;").2, "Expected type name");
        assert_eq!(err("enum A = X([]);").2, "Expected type");
        assert_eq!(err("enum L{T} = Nil;").2, "Expected '='");
        assert_eq!(err("enum A = X; B").2, "Expected 'enum' or 'const'");
        assert_eq!(err("enum A = X(Int").2, "Expected comma or ')'");
        assert_eq!(err("enum A = X #").2, "Unexpected character '#'");
    }

    fn value_err(text: &str) -> (u32, u32, String) {
        let e = parse_values(text).unwrap_err();
        (e.pos.line, e.pos.col, e.message)
    }

    #[test]
    fn parses_values_separated_by_semicolons() {
        let values = parse_values("Circle { radius: 3.0 };\nQuit; [1, 2,]; null; \"s\"; +8; -2.5e3;").unwrap();
        assert_eq!(values.len(), 7);
        assert!(
            matches!(&values[0].kind, ValueKind::Tagged { name, body: Some(TaggedBody::Record(m)) }
            if name == "Circle" && m.len() == 1)
        );
        assert!(matches!(&values[1].kind, ValueKind::Tagged { body: None, .. }));
        assert!(matches!(&values[3].kind, ValueKind::Null));
        assert!(matches!(&values[5].kind, ValueKind::Int(t) if t == "+8"));
    }

    #[test]
    fn a_single_value_needs_no_semicolon() {
        assert_eq!(parse_values("{ x: 1 }").unwrap().len(), 1);
        assert_eq!(parse_values("{ x: 1 };").unwrap().len(), 1);
        assert!(parse_values("// Only a comment.\n").unwrap().is_empty());
    }

    #[test]
    fn a_semicolon_separates_values_so_a_tag_does_not_swallow_the_next_value() {
        let values = parse_values("Quit; { x: 1 }").unwrap();
        assert!(matches!(&values[0].kind, ValueKind::Tagged { body: None, .. }));
        assert!(matches!(&values[1].kind, ValueKind::Record(_)));
        // Without the separator the record is the body of the tag.
        let values = parse_values("Quit { x: 1 }").unwrap();
        assert_eq!(values.len(), 1);
    }

    #[test]
    fn distinguishes_an_empty_tuple_from_a_bare_tag() {
        let values = parse_values("Ping(); Ping; Pong {}").unwrap();
        assert!(matches!(&values[0].kind, ValueKind::Tagged { body: Some(TaggedBody::Tuple(t)), .. } if t.is_empty()));
        assert!(matches!(&values[1].kind, ValueKind::Tagged { body: None, .. }));
        assert!(matches!(&values[2].kind, ValueKind::Tagged { body: Some(TaggedBody::Record(m)), .. } if m.is_empty()));
    }

    #[test]
    fn reports_value_syntax_errors() {
        assert_eq!(value_err("1 2"), (1, 3, "Expected semi-colon".to_string()));
        assert_eq!(value_err("1;;2").2, "Expected value");
        assert_eq!(value_err(";").2, "Expected value");
        assert_eq!(value_err("[1 2]").2, "Expected comma or ']'");
        assert_eq!(value_err("{ x: 1 y: 2 }").2, "Expected comma or '}'");
        assert_eq!(value_err("{ \"x\": 1 }").2, "Expected field name");
        assert_eq!(value_err("(1, 2)").2, "Expected value");
        assert_eq!(value_err("circle { radius: 3.0 }").2, "Expected semi-colon");
        assert_eq!(value_err("Circle { radius: 3.0").2, "Expected comma or '}'");
        assert_eq!(
            value_err("3."),
            (1, 3, "Expected a digit after the decimal point".to_string())
        );
        assert_eq!(value_err("\"\\q\"").2, "Invalid escape in string");
        assert_eq!(value_err("Write(\"oops)").2, "Unterminated string");
    }

    #[test]
    fn classifies_a_file_by_its_first_token() {
        assert_eq!(parse_file("enum A = X;").0, SourceKind::Types);
        assert_eq!(parse_file("A { x: 1 }").0, SourceKind::Values);
        assert_eq!(parse_file("42").0, SourceKind::Values);
        assert_eq!(parse_file("const N: Nat = 1;").0, SourceKind::Types);
        assert_eq!(parse_file("n").0, SourceKind::Values);
        assert_eq!(parse_file("").0, SourceKind::Types);
        assert_eq!(parse_file("// Just a comment.\n").0, SourceKind::Types);
        // The kind is known even when the file does not parse.
        let (kind, parsed) = parse_file("circle {");
        assert_eq!(kind, SourceKind::Values);
        assert!(parsed.is_err());
    }

    #[test]
    fn parses_const_definitions_interleaved_with_enums() {
        let file =
            parse("const A: Nat = 1;\nenum E = X;\nconst b: [E] = [X, X];\nconst C: { a: Nat } = { a: 2 };").unwrap();
        assert_eq!(file.enums.len(), 1);
        assert_eq!(
            file.consts.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
            ["A", "b", "C"]
        );
        assert!(matches!(&file.consts[1].ty, Type::Array(_)));
        assert!(matches!(&file.consts[2].value.kind, ValueKind::Record(m) if m.len() == 1));
    }

    #[test]
    fn reports_const_syntax_errors() {
        assert_eq!(err("const N = 1;"), (1, 9, "Expected ':'".to_string()));
        assert_eq!(err("const N: Nat 1;").2, "Expected '='");
        assert_eq!(err("const N: Nat = 1").2, "Expected semi-colon");
        assert_eq!(err("const N: Nat = ;").2, "Expected value");
        assert_eq!(err("const 1: Nat = 1;").2, "Expected const name");
        assert_eq!(err("const enum: Nat = 1;").2, "Expected const name");
        assert_eq!(
            err("const null: Option<Nat> = None;").2,
            "null is a literal and cannot be the name of a const"
        );
    }

    #[test]
    fn const_is_a_reserved_word() {
        assert!(parse("enum E = X { const: Nat };").is_err());
        assert!(parse_values("{ const: 1 }").is_err());
        assert!(parse_values("const").is_err());
    }

    #[test]
    fn a_lowercase_name_is_a_value_that_takes_no_body() {
        let values = parse_values("limit; [Limit, limit]").unwrap();
        assert!(matches!(&values[0].kind, ValueKind::Tagged { name, body: None } if name == "limit"));
        assert_eq!(value_err("limit(1)").2, "Expected semi-colon");
    }

    #[test]
    fn rejects_very_deep_nesting_instead_of_overflowing_the_stack() {
        let deep_value = "[".repeat(10_000);
        assert_eq!(value_err(&deep_value).2, "Nesting is too deep");
        let deep_type = format!("enum A = X({});", "[".repeat(10_000));
        assert_eq!(err(&deep_type).2, "Nesting is too deep");
        // Moderate nesting is fine.
        let nested = format!("{}1{}", "[".repeat(50), "]".repeat(50));
        assert!(parse_values(&nested).is_ok());
    }
}
