//! A recursive-descent parser for data-type declarations.
//!
//! The parser stops at the first syntax error: there is no error recovery.

use crate::ast::{
    Argument, ConstDecl, EnumDecl, Field, Member, Parameter, TaggedBody, Type, TypeParam, TypesFile, Value, ValueKind,
    Variant, VariantBody,
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

/// The experimental features of the syntax, each of which has to be switched on to be accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Experimental {
    /// The postfix `?` on a value, which is shorthand for `Some`.
    pub some_query: bool,
}

impl Experimental {
    /// Every experimental feature switched on.
    pub fn all() -> Experimental {
        Experimental { some_query: true }
    }
}

/// Parses the text of a types file into its declarations, with no experimental features.
pub fn parse(text: &str) -> Result<TypesFile> {
    Parser::new(text, Experimental::default()).types()
}

/// Parses the text of a values file into its values, with no experimental features.
pub fn parse_values(text: &str) -> Result<Vec<Value>> {
    parse_values_with(text, Experimental::default())
}

/// Parses the text of a values file into its values, accepting the experimental features that are switched on.
pub fn parse_values_with(text: &str, experimental: Experimental) -> Result<Vec<Value>> {
    Parser::new(text, experimental).values()
}

/// Classifies a file by its first token and parses it with the matching grammar.
///
/// A types file always starts with `enum` or `const` and a values file never does. A file with no tokens is treated as a types
/// file, which is how it was handled before values files were supported. The kind is returned even when parsing fails.
pub fn parse_file(text: &str, experimental: Experimental) -> (SourceKind, Result<Parsed>) {
    let mut parser = Parser::new(text, experimental);
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
    /// The height of the tallest of the children that have been parsed so far for the construct being parsed.
    tallest: u32,
    /// The number of postfix `?` that have been parsed so far for the construct being parsed. Each one adds a level.
    wrapped: u32,
    experimental: Experimental,
}

impl Parser {
    fn new(text: &str, experimental: Experimental) -> Parser {
        Parser {
            tokens: lex(text),
            index: 0,
            context: None,
            depth: 0,
            tallest: 0,
            wrapped: 0,
            experimental,
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
    ///
    /// The limit is on the height of the tree as well as on the depth of the calls. A postfix `?` adds a level to the
    /// tree without a call, so the height of a construct is worked out from its children and its `?`, and is passed up
    /// to the construct that contains it.
    fn nested<T>(&mut self, parse: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        if self.depth >= MAX_DEPTH {
            return Err(self.too_deep());
        }
        self.depth += 1;
        let outer = (std::mem::take(&mut self.tallest), std::mem::take(&mut self.wrapped));
        let result = parse(self);
        let height = self.tallest + 1 + self.wrapped;
        (self.tallest, self.wrapped) = outer;
        self.depth -= 1;
        if result.is_ok() && height > MAX_DEPTH {
            return Err(self.too_deep());
        }
        self.tallest = self.tallest.max(height);
        result
    }

    fn too_deep(&self) -> SyntaxError {
        SyntaxError {
            pos: self.pos(),
            message: "Nesting is too deep".to_string(),
            context: self.context.clone(),
        }
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

    /// Whether the tokens from the current one are a qualifier: names separated by dots and ending in `::`, as in
    /// `bundle::` or `acme.shapes::`.
    fn at_qualifier(&self) -> bool {
        let mut i = self.index;
        loop {
            if !matches!(self.tokens[i].tok, Tok::Name(_) | Tok::TypeName(_)) {
                return false;
            }
            // The final token is `Eof`, which is neither a name nor a dot, so this never runs off the end.
            match self.tokens[i + 1].tok {
                Tok::Dot => i += 2,
                Tok::ColonColon => return true,
                _ => return false,
            }
        }
    }

    /// Consumes a qualifier that the caller has checked is there, and returns it with the `::`, as `acme.shapes::`.
    fn qualifier(&mut self) -> String {
        let mut text = String::new();
        loop {
            match self.peek() {
                Tok::Name(part) | Tok::TypeName(part) => text.push_str(part),
                _ => unreachable!("at_qualifier has checked the tokens"),
            }
            self.bump();
            if self.peek() == &Tok::Dot {
                text.push('.');
                self.bump();
            } else {
                self.bump();
                text.push_str("::");
                return text;
            }
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

    /// `const name<Params>: Type = value;` The name may be an identifier of either case.
    fn const_decl(&mut self) -> Result<ConstDecl> {
        self.expect(Tok::Const, "'const'")?;
        let pos = self.pos();
        let name = match self.peek() {
            Tok::TypeName(name) | Tok::Name(name) => name.clone(),
            _ => return self.error("const name"),
        };
        self.bump();
        self.context = Some(format!("const {}", name));
        let params = if self.peek() == &Tok::Lt {
            self.type_params()?
        } else {
            Vec::new()
        };
        // The type is optional, and is inferred from the value if it is left out.
        let ty = if self.peek() == &Tok::Colon {
            self.bump();
            Some(self.ty()?)
        } else {
            None
        };
        self.expect(Tok::Eq, if ty.is_some() { "'='" } else { "':' or '='" })?;
        let value = self.value()?;
        self.expect(Tok::Semi, "semi-colon")?;
        Ok(ConstDecl {
            name,
            pos,
            params,
            ty,
            value,
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
            Tok::LBracket => VariantBody::Array(self.array_body()?),
            _ => VariantBody::Unit,
        };
        Ok(Variant { name, pos, body })
    }

    /// The element type in the brackets of a variant such as `Users[User]`. There is exactly one.
    fn array_body(&mut self) -> Result<Type> {
        self.expect(Tok::LBracket, "'['")?;
        let element = self.ty()?;
        if self.peek() == &Tok::Comma {
            return self.error("']' because an array variant has exactly one element type");
        }
        self.expect(Tok::RBracket, "']'")?;
        Ok(element)
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

    /// Parses the parameters of a tuple constructor. A parameter may be preceded by its name, but once one has been
    /// named all the later ones must be too.
    fn tuple_body(&mut self) -> Result<Vec<Parameter>> {
        self.expect(Tok::LParen, "'('")?;
        let mut params: Vec<Parameter> = Vec::new();
        while self.peek() != &Tok::RParen {
            let pos = self.pos();
            // A name begins a qualified type if it is followed by `::`, or by dots and then `::`.
            let name = match self.peek() {
                Tok::Name(name) if !self.at_qualifier() => {
                    let name = name.clone();
                    self.bump();
                    self.expect(Tok::Colon, "':'")?;
                    Some(name)
                }
                _ => None,
            };
            if name.is_none() && params.last().is_some_and(|p| p.name.is_some()) {
                return Err(SyntaxError {
                    pos,
                    message: "Unnamed parameter after a named parameter".to_string(),
                    context: self.context.clone(),
                });
            }
            let ty = self.ty()?;
            params.push(Parameter { name, pos, ty });
            match self.peek() {
                Tok::Comma => self.bump(),
                Tok::RParen => {}
                _ => return self.error("comma or ')'"),
            }
        }
        self.expect(Tok::RParen, "')'")?;
        Ok(params)
    }

    fn ty(&mut self) -> Result<Type> {
        self.nested(Self::ty_unchecked)
    }

    /// Parses a type and then any postfix `?`, each of which makes the type optional.
    fn ty_unchecked(&mut self) -> Result<Type> {
        let pos = self.pos();
        let mut ty = self.primary_type()?;
        while self.peek() == &Tok::Question {
            self.question()?;
            ty = Type::Ref {
                name: "Option".to_string(),
                pos,
                args: vec![ty],
            };
        }
        Ok(ty)
    }

    /// Consumes a postfix `?`. Each one wraps what it follows and so adds a level to the tree, which has to be counted
    /// at once because a long run of them would otherwise build a tree that is too tall before anything notices.
    fn question(&mut self) -> Result<()> {
        self.bump();
        self.wrapped += 1;
        if self.tallest + 1 + self.wrapped > MAX_DEPTH {
            return Err(self.too_deep());
        }
        Ok(())
    }

    fn primary_type(&mut self) -> Result<Type> {
        match self.peek() {
            Tok::TypeName(_) | Tok::Name(_) if self.at_qualifier() || matches!(self.peek(), Tok::TypeName(_)) => {
                let pos = self.pos();
                let qualifier = if self.at_qualifier() {
                    self.qualifier()
                } else {
                    String::new()
                };
                let (name, _) = self.type_name("type")?;
                let name = format!("{}{}", qualifier, name);
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

    /// Parses a value and then any postfix `?`, each of which wraps it in `Some`.
    fn value_unchecked(&mut self) -> Result<Value> {
        let mut value = self.primary_value()?;
        while self.peek() == &Tok::Question {
            if !self.experimental.some_query {
                return Err(SyntaxError {
                    pos: self.pos(),
                    message: "Postfix '?' is experimental; enable it with --experimental=some-query".to_string(),
                    context: self.context.clone(),
                });
            }
            self.question()?;
            let pos = value.pos;
            value = Value {
                pos,
                kind: ValueKind::Tagged {
                    name: "Some".to_string(),
                    body: Some(TaggedBody::Tuple(vec![Argument { name: None, pos, value }])),
                },
            };
        }
        Ok(value)
    }

    fn primary_value(&mut self) -> Result<Value> {
        let pos = self.pos();
        let kind = match self.peek().clone() {
            Tok::Name(_) | Tok::TypeName(_) if self.at_qualifier() => {
                let qualifier = self.qualifier();
                match self.peek().clone() {
                    Tok::Name(name) => {
                        self.bump();
                        ValueKind::Tagged {
                            name: format!("{}{}", qualifier, name),
                            body: None,
                        }
                    }
                    Tok::TypeName(name) => {
                        self.bump();
                        let name = format!("{}{}", qualifier, name);
                        let body = self.tagged_body()?;
                        ValueKind::Tagged { name, body }
                    }
                    _ => return self.error("name after '::'"),
                }
            }
            Tok::Nat(text) => self.literal(ValueKind::Nat(text)),
            Tok::Int(text) => self.literal(ValueKind::Int(text)),
            Tok::Float(text) => self.literal(ValueKind::Float(text)),
            Tok::Str(text) => self.literal(ValueKind::Str(text)),
            // A lowercase name can only refer to a const, so it takes no body.
            Tok::Name(name) => {
                self.bump();
                ValueKind::Tagged { name, body: None }
            }
            Tok::LBrace => ValueKind::Record(self.members()?),
            Tok::LBracket => ValueKind::Array(self.array()?),
            Tok::TypeName(name) => {
                self.bump();
                let body = self.tagged_body()?;
                ValueKind::Tagged { name, body }
            }
            _ => return self.error("value"),
        };
        Ok(Value { pos, kind })
    }

    /// The record, tuple or array that may follow the name of a constructor.
    fn tagged_body(&mut self) -> Result<Option<TaggedBody>> {
        Ok(match self.peek() {
            Tok::LBrace => Some(TaggedBody::Record(self.members()?)),
            Tok::LParen => Some(TaggedBody::Tuple(self.tuple()?)),
            Tok::LBracket => Some(TaggedBody::Array(self.array()?)),
            _ => None,
        })
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

    /// Parses the arguments of a tuple constructor. A lowercase name followed by a colon names the parameter that the
    /// argument is for. Positional arguments may not follow named ones.
    fn tuple(&mut self) -> Result<Vec<Argument>> {
        self.expect(Tok::LParen, "'('")?;
        let mut args: Vec<Argument> = Vec::new();
        while self.peek() != &Tok::RParen {
            let pos = self.pos();
            let name = match (self.peek(), self.tokens.get(self.index + 1).map(|t| &t.tok)) {
                (Tok::Name(name), Some(Tok::Colon)) => {
                    let name = name.clone();
                    self.bump();
                    self.bump();
                    Some(name)
                }
                _ => None,
            };
            if name.is_none() && args.last().is_some_and(|a| a.name.is_some()) {
                return Err(SyntaxError {
                    pos,
                    message: "Positional argument after a named argument".to_string(),
                    context: self.context.clone(),
                });
            }
            let value = self.value()?;
            args.push(Argument { name, pos, value });
            match self.peek() {
                Tok::Comma => self.bump(),
                Tok::RParen => {}
                _ => return self.error("comma or ')'"),
            }
        }
        self.expect(Tok::RParen, "')'")?;
        Ok(args)
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
    fn parses_named_parameters_after_unnamed_ones() {
        let decls = parse("enum A = X(Int, a: Int, b: [Int],);").unwrap().enums;
        let VariantBody::Tuple(params) = &decls[0].variants[0].body else {
            panic!("expected a tuple");
        };
        let names: Vec<Option<&str>> = params.iter().map(|p| p.name.as_deref()).collect();
        assert_eq!(names, vec![None, Some("a"), Some("b")]);
    }

    #[test]
    fn rejects_malformed_parameter_lists() {
        assert_eq!(
            err("enum A = X(a: Int, Int);"),
            (1, 20, "Unnamed parameter after a named parameter".to_string())
        );
        assert_eq!(err("enum A = X(a Int);").2, "Expected ':'");
        assert_eq!(err("enum A = X(const: Int);").2, "Expected type");
    }

    #[test]
    fn parses_named_arguments_after_positional_ones() {
        let values = parse_values("X(1, a: 2, b: c,)").unwrap();
        let ValueKind::Tagged {
            body: Some(TaggedBody::Tuple(args)),
            ..
        } = &values[0].kind
        else {
            panic!("expected a tuple");
        };
        let names: Vec<Option<&str>> = args.iter().map(|a| a.name.as_deref()).collect();
        assert_eq!(names, vec![None, Some("a"), Some("b")]);
        // The name before a colon is not a const reference, but the value after it can be.
        assert!(matches!(&args[2].value.kind, ValueKind::Tagged { name, body: None } if name == "c"));
    }

    #[test]
    fn rejects_positional_arguments_after_named_ones() {
        let e = parse_values("X(a: 1, 2)").unwrap_err();
        assert_eq!((e.pos.line, e.pos.col), (1, 9));
        assert_eq!(e.message, "Positional argument after a named argument");
    }

    #[test]
    fn parses_an_array_variant_with_one_element_type() {
        let decls = parse("enum L<T> = Items[T], Join[L<T>], Rows[[Nat]], Maybe[Nat?], Done;")
            .unwrap()
            .enums;
        let shown: Vec<String> = decls[0]
            .variants
            .iter()
            .map(|v| match &v.body {
                VariantBody::Array(element) => element.to_string(),
                _ => "-".to_string(),
            })
            .collect();
        assert_eq!(shown, ["T", "L<T>", "[Nat]", "Option<Nat>", "-"]);
    }

    #[test]
    fn rejects_an_array_variant_without_exactly_one_element_type() {
        let err = |text: &str| {
            let e = parse(text).unwrap_err();
            (e.pos.line, e.pos.col, e.message)
        };
        assert_eq!(err("enum A = X[];"), (1, 12, "Expected type".to_string()));
        let (line, col, message) = err("enum A = X[Nat, String];");
        assert_eq!((line, col), (1, 15));
        assert!(message.contains("exactly one element type"), "{}", message);
        assert_eq!(err("enum A = X[Nat;").2, "Expected ']'");
    }

    #[test]
    fn parses_a_tagged_array_value() {
        let values = parse_values("Items[1, 2,]; Items[]; Join[Items[1], Items[]]").unwrap();
        let sizes: Vec<usize> = values
            .iter()
            .map(|v| match &v.kind {
                ValueKind::Tagged {
                    body: Some(TaggedBody::Array(items)),
                    ..
                } => items.len(),
                other => panic!("not an array body: {:?}", other),
            })
            .collect();
        assert_eq!(sizes, [2, 0, 2]);
    }

    #[test]
    fn a_tagged_array_value_has_no_named_elements() {
        let e = parse_values("Items[x: 1]").unwrap_err();
        assert_eq!((e.pos.line, e.pos.col), (1, 8));
        assert_eq!(e.message, "Expected comma or ']'");
    }

    #[test]
    fn a_tagged_array_counts_towards_the_nesting_limit() {
        let deep = |levels: usize| format!("{}1{}", "A[".repeat(levels), "]".repeat(levels));
        assert!(parse_values(&deep(100)).is_ok());
        assert!(parse_values(&deep(MAX_DEPTH as usize + 1)).is_err());
    }

    #[test]
    fn parses_qualified_names_in_types() {
        let text =
            "enum A<T> = X(bundle::T, acme.shapes::U<Nat>, P.Q.R::Left, named: g::V?), Y[local::W], Z { f: [h::K] };";
        let decls = parse(text).unwrap().enums;
        let shown: Vec<String> = decls[0]
            .variants
            .iter()
            .flat_map(|v| match &v.body {
                VariantBody::Tuple(params) => params.iter().map(|p| p.ty.to_string()).collect::<Vec<_>>(),
                VariantBody::Array(element) => vec![element.to_string()],
                VariantBody::Record(fields) => fields.iter().map(|f| f.ty.to_string()).collect(),
                VariantBody::Unit => Vec::new(),
            })
            .collect();
        assert_eq!(
            shown,
            [
                "bundle::T",
                "acme.shapes::U<Nat>",
                "P.Q.R::Left",
                "Option<g::V>",
                "local::W",
                "[h::K]"
            ]
        );
    }

    #[test]
    fn parses_qualified_names_in_values() {
        let values = parse_values(
            "bundle::Foo(1); acme.shapes::Circle { r: 1.0 }; g::x; g::Foo[1, 2]; P.Q.R::Dot; local::Foo(a: g::y)",
        )
        .unwrap();
        let names: Vec<(&str, bool)> = values
            .iter()
            .map(|v| match &v.kind {
                ValueKind::Tagged { name, body } => (name.as_str(), body.is_some()),
                other => panic!("not tagged: {:?}", other),
            })
            .collect();
        assert_eq!(
            names,
            [
                ("bundle::Foo", true),
                ("acme.shapes::Circle", true),
                ("g::x", false),
                ("g::Foo", true),
                ("P.Q.R::Dot", false),
                ("local::Foo", true)
            ]
        );
        // The position of a qualified value is the start of its qualifier.
        assert_eq!((values[1].pos.line, values[1].pos.col), (1, 17));
    }

    #[test]
    fn a_declaration_is_never_qualified() {
        assert_eq!(parse("enum g::A = X;").unwrap_err().message, "Expected type name");
        assert_eq!(
            parse("enum A = g::X;").unwrap_err().message,
            "Expected constructor name"
        );
        assert_eq!(parse("const g::x = 1;").unwrap_err().message, "Expected ':' or '='");
    }

    #[test]
    fn rejects_a_qualifier_without_a_name() {
        assert_eq!(parse("enum A = X(g::);").unwrap_err().message, "Expected type");
        assert_eq!(parse_values("g::;").unwrap_err().message, "Expected name after '::'");
        assert_eq!(parse_values("g::1").unwrap_err().message, "Expected name after '::'");
        // A dot is only part of a qualifier, so this is not a value.
        assert_eq!(parse_values("a.b").unwrap_err().message, "Expected semi-colon");
        assert_eq!(parse("enum A = X(a.b);").unwrap_err().message, "Expected ':'");
    }

    #[test]
    fn a_parameter_name_is_not_taken_for_a_qualifier() {
        // A single colon still names a parameter or a field, and `::` begins a type.
        let decls = parse("enum A = X(n: Nat, m: g::T);").unwrap().enums;
        let VariantBody::Tuple(params) = &decls[0].variants[0].body else {
            panic!("not a tuple");
        };
        assert_eq!(params[0].name.as_deref(), Some("n"));
        assert_eq!(params[1].name.as_deref(), Some("m"));
        assert_eq!(params[1].ty.to_string(), "g::T");
        let values = parse_values("X(g::y, n: g::x)").unwrap();
        let ValueKind::Tagged {
            body: Some(TaggedBody::Tuple(args)),
            ..
        } = &values[0].kind
        else {
            panic!("not a tuple");
        };
        assert_eq!(args[0].name, None);
        assert_eq!(args[1].name.as_deref(), Some("n"));
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
        assert!(matches!(&values[3].kind, ValueKind::Tagged { name, body: None } if name == "null"));
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
        assert_eq!(parse_file("enum A = X;", Experimental::default()).0, SourceKind::Types);
        assert_eq!(parse_file("A { x: 1 }", Experimental::default()).0, SourceKind::Values);
        assert_eq!(parse_file("42", Experimental::default()).0, SourceKind::Values);
        assert_eq!(
            parse_file("const N: Nat = 1;", Experimental::default()).0,
            SourceKind::Types
        );
        assert_eq!(parse_file("n", Experimental::default()).0, SourceKind::Values);
        assert_eq!(parse_file("", Experimental::default()).0, SourceKind::Types);
        assert_eq!(
            parse_file("// Just a comment.\n", Experimental::default()).0,
            SourceKind::Types
        );
        // The kind is known even when the file does not parse.
        let (kind, parsed) = parse_file("circle {", Experimental::default());
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
        assert!(matches!(&file.consts[1].ty, Some(Type::Array(_))));
        assert!(matches!(&file.consts[2].value.kind, ValueKind::Record(m) if m.len() == 1));
    }

    #[test]
    fn reports_const_syntax_errors() {
        assert_eq!(err("const N Nat = 1;"), (1, 9, "Expected ':' or '='".to_string()));
        assert_eq!(err("const N: Nat 1;").2, "Expected '='");
        assert_eq!(err("const N: Nat = 1").2, "Expected semi-colon");
        assert_eq!(err("const N: Nat = ;").2, "Expected value");
        assert_eq!(err("const 1: Nat = 1;").2, "Expected const name");
        assert_eq!(err("const enum: Nat = 1;").2, "Expected const name");
    }

    #[test]
    fn parses_generic_consts() {
        let file = parse("const null<T>: Option<T> = None;\nconst pair<_A, B>: B = x;").unwrap();
        assert_eq!(file.consts[0].params.len(), 1);
        assert_eq!(file.consts[0].params[0].name, "T");
        assert!(file.consts[1].params[1].name == "B" && file.consts[1].params[0].underscore);
        assert_eq!(err("const n<>: Nat = 1;").2, "Expected type parameter name");
        assert_eq!(err("const n<T: Nat = 1;").2, "Expected comma or '>'");
        assert_eq!(err("const n<t>: Nat = 1;").2, "Expected type parameter name");
    }

    #[test]
    fn the_type_of_a_const_is_optional() {
        let file = parse("const N = 1;\nconst e = [];\nconst T: Nat = 2;").unwrap();
        assert!(file.consts[0].ty.is_none() && file.consts[1].ty.is_none());
        assert!(file.consts[2].ty.is_some());
        assert_eq!(err("const N = ;").2, "Expected value");
        assert_eq!(err("const N: = 1;").2, "Expected type");
        // The parameters are read before the type is known to be missing, which is reported after the parse.
        assert_eq!(parse("const N<T> = 1;").unwrap().consts[0].params.len(), 1);
    }

    #[test]
    fn displays_types_as_they_are_written() {
        let file = parse("const x: { a: [Option<Nat>], b: {}, c: Map<String, [Int]> } = y;").unwrap();
        let ty = file.consts[0].ty.as_ref().unwrap();
        assert_eq!(ty.to_string(), "{ a: [Option<Nat>], b: {}, c: Map<String, [Int]> }");
    }

    #[test]
    fn null_is_an_ordinary_name() {
        assert!(parse("const null<T>: Option<T> = None;").is_ok());
        let values = parse_values("null").unwrap();
        assert!(matches!(&values[0].kind, ValueKind::Tagged { name, body: None } if name == "null"));
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

    fn option_of(ty: &Type) -> Option<&Type> {
        match ty {
            Type::Ref { name, args, .. } if name == "Option" && args.len() == 1 => Some(&args[0]),
            _ => None,
        }
    }

    #[test]
    fn a_postfix_question_makes_a_type_optional() {
        let decls = parse("enum A = X(Nat?, [Nat]?, { a: Nat }?, Box<Nat>?, Nat??, Nat ?);")
            .unwrap()
            .enums;
        let VariantBody::Tuple(params) = &decls[0].variants[0].body else {
            panic!("expected a tuple");
        };
        let shown: Vec<String> = params.iter().map(|p| p.ty.to_string()).collect();
        assert_eq!(
            shown,
            vec![
                "Option<Nat>",
                "Option<[Nat]>",
                "Option<{ a: Nat }>",
                "Option<Box<Nat>>",
                "Option<Option<Nat>>",
                "Option<Nat>"
            ]
        );
        // Only the whole type is wrapped, so the question mark inside an array is for the elements.
        let decls = parse("enum A = X([Nat?]);").unwrap().enums;
        let VariantBody::Tuple(params) = &decls[0].variants[0].body else {
            panic!("expected a tuple");
        };
        assert_eq!(params[0].ty.to_string(), "[Option<Nat>]");
    }

    #[test]
    fn an_optional_type_is_positioned_at_the_start_of_its_operand() {
        let decls = parse("enum A = X(  Nat?);").unwrap().enums;
        let VariantBody::Tuple(params) = &decls[0].variants[0].body else {
            panic!("expected a tuple");
        };
        let Type::Ref { pos, .. } = &params[0].ty else {
            panic!("expected a reference");
        };
        assert_eq!((pos.line, pos.col), (1, 14));
        assert!(option_of(&params[0].ty).is_some());
    }

    #[test]
    fn rejects_a_question_with_nothing_to_make_optional() {
        assert_eq!(err("enum A = X(?);"), (1, 12, "Expected type".to_string()));
        assert_eq!(err("enum A = X(Nat, ?);"), (1, 17, "Expected type".to_string()));
        assert_eq!(err("enum A = ?;"), (1, 10, "Expected constructor name".to_string()));
    }

    #[test]
    fn many_question_marks_are_stopped_by_the_nesting_limit() {
        let text = format!("enum A = X(Nat{});", "?".repeat(1000));
        assert_eq!(err(&text).2, "Nesting is too deep");
        let text = format!("enum A = X(Nat{});", "?".repeat(50));
        assert!(parse(&text).is_ok());
    }

    const SOME_QUERY: Experimental = Experimental { some_query: true };

    #[test]
    fn a_postfix_question_on_a_value_needs_the_feature() {
        let e = parse_values("0?").unwrap_err();
        assert_eq!(
            (e.pos.line, e.pos.col, e.message.as_str()),
            (
                1,
                2,
                "Postfix '?' is experimental; enable it with --experimental=some-query"
            )
        );
        // The feature is for values, wherever they are written.
        let e = parse("const a = 0?;").unwrap_err();
        assert_eq!(
            e.message,
            "Postfix '?' is experimental; enable it with --experimental=some-query"
        );
        assert!(parse_values_with("0?", Experimental::default()).is_err());
        assert!(parse_values_with("0?", SOME_QUERY).is_ok());
        assert!(parse_values_with("0?", Experimental::all()).is_ok());
    }

    #[test]
    fn a_postfix_question_on_a_value_is_some() {
        let values = parse_values_with("1?; [2?]; Point(1, 2)?; x ?; 3??", SOME_QUERY).unwrap();
        let ValueKind::Tagged { name, body } = &values[0].kind else {
            panic!("expected a constructor");
        };
        assert_eq!(name, "Some");
        let Some(TaggedBody::Tuple(arguments)) = body else {
            panic!("expected a tuple");
        };
        assert_eq!(arguments.len(), 1);
        assert_eq!(arguments[0].name, None);
        assert!(matches!(&arguments[0].value.kind, ValueKind::Nat(text) if text == "1"));
        // The Some is positioned at its operand, as is the argument.
        assert_eq!((values[0].pos.line, values[0].pos.col), (1, 1));
        assert_eq!(arguments[0].pos, values[0].pos);
        // It applies to the whole value, including the body of a constructor.
        let ValueKind::Tagged {
            body: Some(TaggedBody::Tuple(arguments)),
            ..
        } = &values[2].kind
        else {
            panic!("expected a constructor");
        };
        assert!(matches!(&arguments[0].value.kind, ValueKind::Tagged { name, .. } if name == "Point"));
        // A repeated question mark wraps once for each.
        let ValueKind::Tagged {
            body: Some(TaggedBody::Tuple(arguments)),
            ..
        } = &values[4].kind
        else {
            panic!("expected a constructor");
        };
        assert!(matches!(&arguments[0].value.kind, ValueKind::Tagged { name, .. } if name == "Some"));
    }

    #[test]
    fn a_postfix_question_on_a_value_is_not_confused_with_a_named_argument() {
        let values = parse_values_with("X(a: 1?, b: 2)", SOME_QUERY).unwrap();
        let ValueKind::Tagged {
            body: Some(TaggedBody::Tuple(arguments)),
            ..
        } = &values[0].kind
        else {
            panic!("expected a constructor");
        };
        let names: Vec<Option<&str>> = arguments.iter().map(|a| a.name.as_deref()).collect();
        assert_eq!(names, vec![Some("a"), Some("b")]);
    }

    #[test]
    fn many_question_marks_on_a_value_are_stopped_by_the_nesting_limit() {
        let text = format!("1{}", "?".repeat(1000));
        assert_eq!(
            parse_values_with(&text, SOME_QUERY).unwrap_err().message,
            "Nesting is too deep"
        );
    }

    #[test]
    fn question_marks_between_arrays_count_towards_the_nesting_limit() {
        // Each level is an array and a `?`, so the tree is twice as tall as the calls are deep.
        let text = format!("enum A = X({}Nat{});", "[".repeat(150), "]?".repeat(150));
        assert_eq!(err(&text).2, "Nesting is too deep");
        let text = format!("enum A = X({}Nat{});", "[".repeat(90), "]?".repeat(90));
        assert!(parse(&text).is_ok());
        let text = format!("{}1{}", "[".repeat(150), "]?".repeat(150));
        assert_eq!(
            parse_values_with(&text, SOME_QUERY).unwrap_err().message,
            "Nesting is too deep"
        );
        let text = format!("{}1{}", "[".repeat(90), "]?".repeat(90));
        assert!(parse_values_with(&text, SOME_QUERY).is_ok());
    }

    #[test]
    fn question_marks_between_other_constructs_count_towards_the_nesting_limit() {
        // A constructor, a record and a type argument, each followed by a `?`.
        let text = format!("{}1{}", "Box(".repeat(150), ")?".repeat(150));
        assert_eq!(
            parse_values_with(&text, SOME_QUERY).unwrap_err().message,
            "Nesting is too deep"
        );
        let text = format!("{}1{}", "{ a: ".repeat(150), " }?".repeat(150));
        assert_eq!(
            parse_values_with(&text, SOME_QUERY).unwrap_err().message,
            "Nesting is too deep"
        );
        let text = format!("enum A = X({}Nat{});", "Box<".repeat(150), ">?".repeat(150));
        assert_eq!(err(&text).2, "Nesting is too deep");
    }
}
