//! Parses the text of a query into its syntax tree.
//!
//! The grammar is Rust's expressions with the changes that decision 44 and the query task settle: `{` starts a record
//! except in the braced forms of `if` and `match`, there are no blocks, `if` and `match` also have outfix forms, field
//! shorthand is allowed only after a constructor name, and `as` names a Constructo type.

use super::lexer::{lex, Tok, Token};
use super::syntax::*;
use crate::diagnostics::{Diagnostic, Pos};
use crate::limits::MAX_DEPTH;
use std::collections::HashMap;
use std::rc::Rc;

/// The name that diagnostics give to the text of the query.
pub const QUERY_NAME: &str = "<query>";

type Result<T> = std::result::Result<T, Diagnostic>;

/// Parses a whole query.
pub fn parse_query(text: &str) -> Result<Expr> {
    let mut parser = Parser {
        tokens: lex(text),
        index: 0,
        no_ctor_record: false,
        depth: 0,
        too_deep: false,
        conditions: HashMap::new(),
    };
    let expr = parser.expr()?;
    if parser.peek() != &Tok::Eof {
        return parser.error("the end of the query");
    }
    Ok(expr)
}

/// A condition that has been read: the condition and whether the outfix form follows, or the error, with the index of
/// the token after it and whether the nesting limit was passed.
type ReadCondition = (Result<(Expr, bool)>, usize, bool);

struct Parser {
    tokens: Vec<Token>,
    index: usize,
    /// Whether a constructor followed by `{` is read without the record, as in Rust's conditions. This is only set
    /// when the condition of a braced `if` or `match` is read again, after reading it in full failed.
    no_ctor_record: bool,
    /// How deeply the expressions and patterns being read are nested, which keeps the recursion bounded.
    depth: u32,
    /// Whether the nesting limit has been passed. Reading a condition again cannot help then, so it is not tried,
    /// which would otherwise double the work at every level of a deeply nested query.
    too_deep: bool,
    /// The conditions that have been read, by where they start and the state they were read in, with where they end.
    /// A condition that is read a second time, as the inner conditions of one that is read again are, takes its result
    /// from here, so that the work does not double at every level of conditions within conditions.
    conditions: HashMap<(usize, bool, u32), ReadCondition>,
}

impl Parser {
    fn peek(&self) -> &Tok {
        &self.tokens[self.index].tok
    }

    fn peek_at(&self, ahead: usize) -> &Tok {
        let last = self.tokens.len() - 1;
        &self.tokens[(self.index + ahead).min(last)].tok
    }

    fn pos(&self) -> Pos {
        self.tokens[self.index].pos
    }

    fn bump(&mut self) {
        if self.index < self.tokens.len() - 1 {
            self.index += 1;
        }
    }

    fn is_punct(&self, p: &str) -> bool {
        matches!(self.peek(), Tok::Punct(q) if *q == p)
    }

    fn is_keyword(&self, k: &str) -> bool {
        matches!(self.peek(), Tok::Keyword(q) if *q == k)
    }

    fn eat_punct(&mut self, p: &str) -> bool {
        let found = self.is_punct(p);
        if found {
            self.bump();
        }
        found
    }

    fn eat_keyword(&mut self, k: &str) -> bool {
        let found = self.is_keyword(k);
        if found {
            self.bump();
        }
        found
    }

    fn expect_punct(&mut self, p: &str) -> Result<()> {
        if self.eat_punct(p) {
            Ok(())
        } else {
            self.error(&format!("'{}'", p))
        }
    }

    fn expect_keyword(&mut self, k: &str) -> Result<()> {
        if self.eat_keyword(k) {
            Ok(())
        } else {
            self.error(&format!("'{}'", k))
        }
    }

    fn error<T>(&self, expected: &str) -> Result<T> {
        let found = match self.peek() {
            Tok::Eof => "the end of the query".to_string(),
            Tok::Error(message) => return Err(Diagnostic::new(QUERY_NAME, self.pos(), message.clone())),
            Tok::Invalid(c) => format!("'{}'", c),
            Tok::TypeName(s) | Tok::Name(s) => format!("'{}'", s),
            Tok::Keyword(k) => format!("'{}'", k),
            Tok::Sigil(s) => format!("'@{}'", s),
            Tok::Nat(s) | Tok::Float(s) => format!("'{}'", s),
            Tok::Str(_) => "a string".to_string(),
            Tok::Punct(p) => format!("'{}'", p),
        };
        Err(Diagnostic::new(
            QUERY_NAME,
            self.pos(),
            format!("Expected {} but found {}", expected, found),
        ))
    }

    fn fail<T>(&self, pos: Pos, message: impl Into<String>) -> Result<T> {
        Err(Diagnostic::new(QUERY_NAME, pos, message))
    }

    /// Enters one more level of nesting, refusing a query that is too deeply nested to read safely.
    fn deeper(&mut self) -> Result<()> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            self.too_deep = true;
            return self.fail(
                self.pos(),
                format!("The query is nested more than {} levels deep", MAX_DEPTH),
            );
        }
        Ok(())
    }

    /// Reads a part of the query that is inside brackets, where the restriction of a condition is lifted.
    fn bracketed<T>(&mut self, read: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        let saved = std::mem::replace(&mut self.no_ctor_record, false);
        let result = read(self);
        self.no_ctor_record = saved;
        result
    }

    /// An expression, which may be an assignment. Whether an assignment is allowed where it is found is checked after
    /// parsing.
    fn expr(&mut self) -> Result<Expr> {
        self.deeper()?;
        let target = self.or_expr()?;
        let op = match self.peek() {
            Tok::Punct("=") => Some(None),
            Tok::Punct("+=") => Some(Some(BinOp::Add)),
            Tok::Punct("-=") => Some(Some(BinOp::Sub)),
            Tok::Punct("*=") => Some(Some(BinOp::Mul)),
            Tok::Punct("/=") => Some(Some(BinOp::Div)),
            Tok::Punct("%=") => Some(Some(BinOp::Rem)),
            _ => None,
        };
        let result = match op {
            None => target,
            Some(op) => {
                let pos = self.pos();
                self.bump();
                let value = self.or_expr()?;
                Expr {
                    pos,
                    kind: ExprKind::Assign {
                        target: Box::new(target),
                        op,
                        value: Box::new(value),
                    },
                }
            }
        };
        self.depth -= 1;
        Ok(result)
    }

    /// A chain of binary operators. The chain is read in a loop, but it makes a tree as deep as it is long, which the
    /// later passes recurse over, so each link counts as a level of nesting, as do those of `postfix` and `cast`.
    fn binary(&mut self, ops: &[(&str, BinOp)], next: fn(&mut Self) -> Result<Expr>, chain: bool) -> Result<Expr> {
        let start = self.depth;
        let mut left = next(self)?;
        loop {
            let found = ops.iter().find(|(p, _)| self.is_punct(p)).map(|(_, op)| *op);
            let Some(op) = found else {
                self.depth = start;
                return Ok(left);
            };
            self.deeper()?;
            let pos = self.pos();
            self.bump();
            let right = next(self)?;
            left = Expr {
                pos,
                kind: ExprKind::Binary(op, Box::new(left), Box::new(right)),
            };
            if !chain {
                // Comparisons do not chain, as in Rust: `a < b < c` is an error.
                if ops.iter().any(|(p, _)| self.is_punct(p)) {
                    return self.fail(self.pos(), "Comparisons cannot be chained; use && between them");
                }
                self.depth = start;
                return Ok(left);
            }
        }
    }

    fn or_expr(&mut self) -> Result<Expr> {
        self.binary(&[("||", BinOp::Or)], Self::and_expr, true)
    }

    fn and_expr(&mut self) -> Result<Expr> {
        self.binary(&[("&&", BinOp::And)], Self::comparison, true)
    }

    fn comparison(&mut self) -> Result<Expr> {
        let ops = [
            ("==", BinOp::Eq),
            ("!=", BinOp::Ne),
            ("<=", BinOp::Le),
            (">=", BinOp::Ge),
            ("<", BinOp::Lt),
            (">", BinOp::Gt),
        ];
        self.binary(&ops, Self::additive, false)
    }

    fn additive(&mut self) -> Result<Expr> {
        self.binary(&[("+", BinOp::Add), ("-", BinOp::Sub)], Self::multiplicative, true)
    }

    fn multiplicative(&mut self) -> Result<Expr> {
        self.binary(
            &[("*", BinOp::Mul), ("/", BinOp::Div), ("%", BinOp::Rem)],
            Self::cast,
            true,
        )
    }

    fn cast(&mut self) -> Result<Expr> {
        let start = self.depth;
        let mut expr = self.unary()?;
        while self.is_keyword("as") {
            self.deeper()?;
            let pos = self.pos();
            self.bump();
            let kind = match self.peek() {
                Tok::TypeName(name) if name == "Nat" => NumKind::Nat,
                Tok::TypeName(name) if name == "Int" => NumKind::Int,
                Tok::TypeName(name) if name == "Float" => NumKind::Float,
                Tok::Name(name) if matches!(name.as_str(), "f64" | "f32" | "u64" | "i64" | "u32" | "i32" | "usize") => {
                    return self.fail(
                        self.pos(),
                        format!("Casts name a Constructo type: Nat, Int or Float, not {}", name),
                    )
                }
                _ => return self.error("Nat, Int or Float after 'as'"),
            };
            self.bump();
            expr = Expr {
                pos,
                kind: ExprKind::Cast(Box::new(expr), kind),
            };
        }
        self.depth = start;
        Ok(expr)
    }

    /// Whether the current token is a sign that touches the number after it, which makes them one literal.
    fn at_signed_number(&self) -> bool {
        let sign = &self.tokens[self.index];
        let next = &self.tokens[(self.index + 1).min(self.tokens.len() - 1)];
        matches!(sign.tok, Tok::Punct("+") | Tok::Punct("-"))
            && matches!(next.tok, Tok::Nat(_) | Tok::Float(_))
            && sign.end == next.pos
    }

    fn unary(&mut self) -> Result<Expr> {
        let pos = self.pos();
        let op = if self.is_punct("!") {
            UnOp::Not
        } else if self.is_punct("-") && !self.at_signed_number() {
            UnOp::Neg
        } else {
            return self.postfix();
        };
        self.bump();
        self.deeper()?;
        let operand = self.unary()?;
        self.depth -= 1;
        Ok(Expr {
            pos,
            kind: ExprKind::Unary(op, Box::new(operand)),
        })
    }

    fn postfix(&mut self) -> Result<Expr> {
        let start = self.depth;
        let mut expr = self.primary()?;
        loop {
            if self.is_punct(".") || self.is_punct("[") {
                self.deeper()?;
            }
            if self.is_punct(".") {
                self.bump();
                let pos = self.pos();
                match self.peek().clone() {
                    Tok::Name(name) => {
                        self.bump();
                        if self.is_punct("(") {
                            let args = self.call_args()?;
                            expr = Expr {
                                pos,
                                kind: ExprKind::Method {
                                    receiver: Box::new(expr),
                                    name,
                                    args,
                                },
                            };
                        } else {
                            expr = Expr {
                                pos,
                                kind: ExprKind::Field(Box::new(expr), name),
                            };
                        }
                    }
                    Tok::Nat(digits) => {
                        self.bump();
                        let Ok(n) = digits.parse::<usize>() else {
                            return self.fail(pos, format!("{} is too large to be a position", digits));
                        };
                        expr = Expr {
                            pos,
                            kind: ExprKind::Position(Box::new(expr), n),
                        };
                    }
                    _ => return self.error("a field, a position or a method after '.'"),
                }
            } else if self.is_punct("[") {
                let pos = self.pos();
                self.bump();
                let index = self.bracketed(|p| p.expr())?;
                self.expect_punct("]")?;
                expr = Expr {
                    pos,
                    kind: ExprKind::Index(Box::new(expr), Box::new(index)),
                };
            } else {
                self.depth = start;
                return Ok(expr);
            }
        }
    }

    /// The arguments of a method call, from the `(`.
    fn call_args(&mut self) -> Result<Vec<Expr>> {
        self.expect_punct("(")?;
        self.bracketed(|p| {
            let mut args = Vec::new();
            while !p.is_punct(")") {
                args.push(p.expr()?);
                if !p.eat_punct(",") {
                    break;
                }
            }
            p.expect_punct(")")?;
            Ok(args)
        })
    }

    /// Whether the tokens from the current one are a qualifier: names separated by dots and ending in `::`.
    fn at_qualifier(&self) -> bool {
        let mut ahead = 0;
        loop {
            if !matches!(self.peek_at(ahead), Tok::Name(_) | Tok::TypeName(_)) {
                return false;
            }
            match self.peek_at(ahead + 1) {
                Tok::Punct(".") => ahead += 2,
                Tok::Punct("::") => return true,
                _ => return false,
            }
        }
    }

    /// A constructor name, possibly qualified, as `shapes::Circle` or `acme.shapes::Circle`.
    fn ctor_name(&mut self) -> Result<String> {
        let mut text = String::new();
        while self.at_qualifier() {
            loop {
                match self.peek().clone() {
                    Tok::Name(part) | Tok::TypeName(part) => text.push_str(&part),
                    _ => unreachable!("at_qualifier has checked the tokens"),
                }
                self.bump();
                if self.eat_punct(".") {
                    text.push('.');
                } else {
                    self.expect_punct("::")?;
                    text.push_str("::");
                    break;
                }
            }
        }
        match self.peek().clone() {
            Tok::TypeName(name) => {
                self.bump();
                text.push_str(&name);
                Ok(text)
            }
            _ => self.error("a constructor name"),
        }
    }

    fn literal(&mut self) -> Option<Lit> {
        let signed = self.at_signed_number();
        let sign = if signed {
            let sign = if self.is_punct("-") { "-" } else { "+" };
            self.bump();
            sign
        } else {
            ""
        };
        let lit = match self.peek().clone() {
            Tok::Nat(digits) if signed => Lit::Int(format!("{}{}", sign, digits)),
            Tok::Nat(digits) => Lit::Nat(digits),
            Tok::Float(text) => Lit::Float(format!("{}{}", sign, text)),
            Tok::Str(text) => Lit::Str(text),
            _ => return None,
        };
        self.bump();
        Some(lit)
    }

    fn primary(&mut self) -> Result<Expr> {
        let pos = self.pos();
        if let Some(lit) = self.literal() {
            return Ok(Expr {
                pos,
                kind: ExprKind::Lit(lit),
            });
        }
        let kind = match self.peek().clone() {
            Tok::Sigil(word) if word == "values" => {
                self.bump();
                ExprKind::Values
            }
            Tok::Sigil(word) => return self.fail(pos, format!("@{} is not known; the stream is @values", word)),
            Tok::Name(name) if name == "matches" && self.peek_at(1) == &Tok::Punct("!") => {
                self.bump();
                self.bump();
                self.expect_punct("(")?;
                let (subject, pattern) = self.bracketed(|p| {
                    let subject = p.expr()?;
                    p.expect_punct(",")?;
                    let pattern = p.pattern(true)?;
                    p.eat_punct(",");
                    p.expect_punct(")")?;
                    Ok((subject, pattern))
                })?;
                ExprKind::Matches(Box::new(subject), pattern)
            }
            Tok::Name(_) | Tok::TypeName(_) if self.at_qualifier() || matches!(self.peek(), Tok::TypeName(_)) => {
                let name = self.ctor_name()?;
                let body = self.ctor_body()?;
                ExprKind::Ctor { name, body }
            }
            Tok::Name(name) => {
                self.bump();
                ExprKind::Var(name)
            }
            Tok::Punct("(") => {
                self.bump();
                let inner = self.bracketed(|p| p.expr())?;
                self.expect_punct(")")?;
                return Ok(inner);
            }
            Tok::Punct("{") => {
                self.bump();
                let (fields, base) = self.bracketed(|p| p.record_inits(false))?;
                ExprKind::Record { fields, base }
            }
            Tok::Punct("[") => {
                self.bump();
                ExprKind::Array(self.bracketed(|p| p.items("]"))?)
            }
            Tok::Punct("|") => ExprKind::Closure(Rc::new(self.closure()?)),
            Tok::Punct("||") => {
                return self.fail(pos, "A closure here takes one parameter, as in |v| ...");
            }
            // A condition or scrutinee can itself start with `if` or `match`, which reaches here without passing
            // through `expr`, so the nesting is counted here as well.
            Tok::Keyword("if") => {
                self.deeper()?;
                let kind = self.if_expr();
                self.depth -= 1;
                kind?
            }
            Tok::Keyword("match") => {
                self.deeper()?;
                let kind = self.match_expr();
                self.depth -= 1;
                kind?
            }
            _ => return self.error("an expression"),
        };
        Ok(Expr { pos, kind })
    }

    /// The expressions of an array or a tuple, after the opening bracket and up to and including `close`.
    fn items(&mut self, close: &str) -> Result<Vec<Expr>> {
        let mut items = Vec::new();
        while !self.is_punct(close) {
            items.push(self.expr()?);
            if !self.eat_punct(",") {
                break;
            }
        }
        self.expect_punct(close)?;
        Ok(items)
    }

    /// The body of a constructor, if one follows.
    fn ctor_body(&mut self) -> Result<Option<CtorBody>> {
        if self.is_punct("{") && !self.no_ctor_record {
            self.bump();
            let (fields, base) = self.bracketed(|p| p.record_inits(true))?;
            Ok(Some(CtorBody::Record { fields, base }))
        } else if self.is_punct("(") {
            self.bump();
            Ok(Some(CtorBody::Tuple(self.bracketed(|p| p.tuple_inits())?)))
        } else if self.is_punct("[") {
            self.bump();
            Ok(Some(CtorBody::Array(self.bracketed(|p| p.items("]"))?)))
        } else {
            Ok(None)
        }
    }

    /// The fields of a record, after the `{` and up to and including the `}`, with an optional `..base` at the end.
    /// Field shorthand, `{ x }` for `{ x: x }`, is allowed only after a constructor name, as in Rust.
    fn record_inits(&mut self, shorthand: bool) -> Result<(Vec<FieldInit>, Option<Box<Expr>>)> {
        let mut fields: Vec<FieldInit> = Vec::new();
        let mut base = None;
        while !self.is_punct("}") {
            if self.eat_punct("..") {
                base = Some(Box::new(self.expr()?));
                self.eat_punct(",");
                break;
            }
            let pos = self.pos();
            let Tok::Name(name) = self.peek().clone() else {
                return self.error("a field name");
            };
            self.bump();
            if fields.iter().any(|f| f.name == name) {
                return self.fail(pos, format!("The field {} is given more than once", name));
            }
            let value = if self.eat_punct(":") {
                self.expr()?
            } else if shorthand {
                Expr {
                    pos,
                    kind: ExprKind::Var(name.clone()),
                }
            } else if self.is_punct(",") || self.is_punct("}") {
                return self.fail(
                    pos,
                    format!(
                        "A bare record names its fields: write {} for the value, or {{ {}: {} }} for a record",
                        name, name, name
                    ),
                );
            } else {
                return self.error("':' after the field name");
            };
            fields.push(FieldInit { name, pos, value });
            if !self.eat_punct(",") {
                break;
            }
        }
        self.expect_punct("}")?;
        Ok((fields, base))
    }

    /// The arguments of a tuple, after the `(` and up to and including the `)`. Positional arguments come first.
    fn tuple_inits(&mut self) -> Result<Vec<ArgInit>> {
        let mut args: Vec<ArgInit> = Vec::new();
        while !self.is_punct(")") {
            let pos = self.pos();
            let named = matches!(self.peek(), Tok::Name(_)) && self.peek_at(1) == &Tok::Punct(":");
            let name = if named {
                let Tok::Name(name) = self.peek().clone() else {
                    unreachable!("checked above")
                };
                self.bump();
                self.bump();
                Some(name)
            } else {
                if args.iter().any(|a| a.name.is_some()) {
                    return self.fail(pos, "A positional argument cannot follow a named one");
                }
                None
            };
            args.push(ArgInit {
                name,
                pos,
                value: self.expr()?,
            });
            if !self.eat_punct(",") {
                break;
            }
        }
        self.expect_punct(")")?;
        Ok(args)
    }

    /// `|pattern, ...| body`. The parameters cannot be alternatives, which would need parentheses, as in Rust.
    fn closure(&mut self) -> Result<Closure> {
        let pos = self.pos();
        self.expect_punct("|")?;
        let mut params = Vec::new();
        while !self.is_punct("|") {
            params.push(self.bracketed(|p| p.pattern(false))?);
            if !self.eat_punct(",") {
                break;
            }
        }
        self.expect_punct("|")?;
        let body = self.bracketed(|p| p.expr())?;
        Ok(Closure { pos, params, body })
    }

    /// Reads the condition of a braced or outfix form, and says which form follows. The condition is read in full,
    /// constructor bodies included. If what follows is neither `outfix` nor `{`, it is read again without the record
    /// of a constructor, which a braced form's `{` would otherwise be taken for, as in `if v == None { x } else ...`.
    fn condition(&mut self, outfix: &str) -> Result<(Expr, bool)> {
        let key = (self.index, self.no_ctor_record, self.depth);
        if let Some((result, end, too_deep)) = self.conditions.get(&key) {
            self.index = *end;
            self.too_deep |= *too_deep;
            return result.clone();
        }
        let result = self.read_condition(outfix);
        self.conditions.insert(key, (result.clone(), self.index, self.too_deep));
        result
    }

    /// Reads a condition, as `condition` describes, without looking for it among those already read.
    fn read_condition(&mut self, outfix: &str) -> Result<(Expr, bool)> {
        let start = self.index;
        let depth = self.depth;
        let attempt = self.or_expr();
        if self.too_deep {
            return attempt.map(|cond| (cond, false));
        }
        if let Ok(cond) = attempt {
            if self.is_keyword(outfix) {
                self.bump();
                return Ok((cond, true));
            }
            if self.is_punct("{") {
                return Ok((cond, false));
            }
        }
        self.index = start;
        self.depth = depth;
        let saved = std::mem::replace(&mut self.no_ctor_record, true);
        let cond = self.or_expr();
        self.no_ctor_record = saved;
        let cond = cond?;
        if self.is_keyword(outfix) {
            self.bump();
            return Ok((cond, true));
        }
        if !self.is_punct("{") {
            return self.error(&format!("'{}' or '{{'", outfix));
        }
        Ok((cond, false))
    }

    /// `if` in its outfix form, `if c then a elseif c then b else d endif`, or Rust's, `if c { a } else { d }`.
    fn if_expr(&mut self) -> Result<ExprKind> {
        self.expect_keyword("if")?;
        let (cond, outfix) = self.condition("then")?;
        let mut branches = Vec::new();
        if outfix {
            let then = self.bracketed(|p| p.expr())?;
            branches.push((cond, then));
            loop {
                if self.eat_keyword("elseif") {
                    let (cond, outfix) = self.condition("then")?;
                    if !outfix {
                        return self.error("'then'; this 'if' is in the outfix form");
                    }
                    let then = self.bracketed(|p| p.expr())?;
                    branches.push((cond, then));
                } else if self.eat_keyword("else") {
                    let otherwise = self.bracketed(|p| p.expr())?;
                    self.expect_keyword("endif")?;
                    return Ok(ExprKind::If {
                        branches,
                        otherwise: Box::new(otherwise),
                    });
                } else if self.is_keyword("endif") {
                    return self.fail(self.pos(), "An 'if' needs an 'else', since it must give a value");
                } else {
                    return self.error("'elseif', 'else' or 'endif'");
                }
            }
        }
        let then = self.braced_branch()?;
        branches.push((cond, then));
        loop {
            if !self.eat_keyword("else") {
                return self.fail(self.pos(), "An 'if' needs an 'else', since it must give a value");
            }
            if self.eat_keyword("if") {
                let (cond, outfix) = self.condition("then")?;
                if outfix {
                    return self.error("'{'; this 'if' is in Rust's braced form");
                }
                let then = self.braced_branch()?;
                branches.push((cond, then));
            } else {
                let otherwise = self.braced_branch()?;
                return Ok(ExprKind::If {
                    branches,
                    otherwise: Box::new(otherwise),
                });
            }
        }
    }

    /// The branch of a braced `if`, `{ expr }`, which holds one expression, not statements.
    fn braced_branch(&mut self) -> Result<Expr> {
        self.expect_punct("{")?;
        let expr = self.bracketed(|p| p.expr())?;
        if self.is_punct(";") {
            return self.fail(self.pos(), "A branch holds one expression, and there are no statements");
        }
        self.expect_punct("}")?;
        Ok(expr)
    }

    /// `match` in its outfix form, `match e with arms endmatch`, or Rust's, `match e { arms }`.
    fn match_expr(&mut self) -> Result<ExprKind> {
        self.expect_keyword("match")?;
        let (scrutinee, outfix) = self.condition("with")?;
        if !outfix {
            self.expect_punct("{")?;
        }
        let close_is = |p: &Self| {
            if outfix {
                p.is_keyword("endmatch")
            } else {
                p.is_punct("}")
            }
        };
        let arms = self.bracketed(|p| {
            let mut arms = Vec::new();
            while !close_is(p) {
                let pattern = p.pattern(true)?;
                let guard = if p.eat_keyword("if") { Some(p.expr()?) } else { None };
                p.expect_punct("=>")?;
                let body = p.expr()?;
                arms.push(Arm { pattern, guard, body });
                if !p.eat_punct(",") {
                    break;
                }
            }
            Ok(arms)
        })?;
        if outfix {
            self.expect_keyword("endmatch")?;
        } else {
            self.expect_punct("}")?;
        }
        if arms.is_empty() {
            return self.fail(self.pos(), "A match needs at least one arm");
        }
        Ok(ExprKind::Match {
            scrutinee: Box::new(scrutinee),
            arms,
        })
    }

    /// A pattern. With `alternatives`, `p | q` is allowed at the top, as in a `match` arm but not in a closure's
    /// parameters, where `|` ends the list.
    fn pattern(&mut self, alternatives: bool) -> Result<Pattern> {
        self.deeper()?;
        let first = self.single_pattern()?;
        let result = if alternatives && self.is_punct("|") {
            let pos = first.pos;
            let mut all = vec![first];
            while self.eat_punct("|") {
                all.push(self.single_pattern()?);
            }
            Pattern {
                pos,
                kind: PatternKind::Or(all),
            }
        } else {
            first
        };
        self.depth -= 1;
        Ok(result)
    }

    fn single_pattern(&mut self) -> Result<Pattern> {
        let pos = self.pos();
        if let Some(lit) = self.literal() {
            return Ok(Pattern {
                pos,
                kind: PatternKind::Lit(lit),
            });
        }
        let kind = match self.peek().clone() {
            Tok::Keyword("_") => {
                self.bump();
                PatternKind::Wild
            }
            Tok::Name(_) | Tok::TypeName(_) if self.at_qualifier() || matches!(self.peek(), Tok::TypeName(_)) => {
                let name = self.ctor_name()?;
                let body = if self.eat_punct("{") {
                    let (fields, rest) = self.field_patterns()?;
                    Some(PatBody::Record { fields, rest })
                } else if self.eat_punct("(") {
                    let (args, rest) = self.arg_patterns()?;
                    Some(PatBody::Tuple { args, rest })
                } else if self.eat_punct("[") {
                    let (items, rest) = self.item_patterns()?;
                    Some(PatBody::Array { items, rest })
                } else {
                    None
                };
                PatternKind::Ctor { name, body }
            }
            // The consts of the prelude are values to compare with, not names to bind, as `true` is in Rust.
            Tok::Name(name) if matches!(name.as_str(), "true" | "false" | "null") => {
                self.bump();
                PatternKind::Ctor { name, body: None }
            }
            Tok::Name(name) => {
                self.bump();
                if self.eat_punct("@") {
                    self.deeper()?;
                    let inner = self.single_pattern();
                    self.depth -= 1;
                    PatternKind::At(name, Box::new(inner?))
                } else {
                    PatternKind::Bind(name)
                }
            }
            Tok::Punct("{") => {
                self.bump();
                let (fields, rest) = self.field_patterns()?;
                PatternKind::Record { fields, rest }
            }
            Tok::Punct("[") => {
                self.bump();
                let (items, rest) = self.item_patterns()?;
                PatternKind::Array { items, rest }
            }
            Tok::Punct("(") => {
                self.bump();
                let inner = self.pattern(true)?;
                self.expect_punct(")")?;
                return Ok(inner);
            }
            _ => return self.error("a pattern"),
        };
        Ok(Pattern { pos, kind })
    }

    /// The fields of a record pattern, after the `{` and up to and including the `}`. `x` is shorthand for `x: x`.
    fn field_patterns(&mut self) -> Result<(Vec<FieldPat>, bool)> {
        let mut fields = Vec::new();
        let mut rest = false;
        while !self.is_punct("}") {
            if self.eat_punct("..") {
                rest = true;
                break;
            }
            let pos = self.pos();
            let Tok::Name(name) = self.peek().clone() else {
                return self.error("a field name or '..'");
            };
            self.bump();
            let pattern = if self.eat_punct(":") {
                self.pattern(true)?
            } else {
                Pattern {
                    pos,
                    kind: PatternKind::Bind(name.clone()),
                }
            };
            fields.push(FieldPat { name, pos, pattern });
            if !self.eat_punct(",") {
                break;
            }
        }
        self.expect_punct("}")?;
        Ok((fields, rest))
    }

    /// The arguments of a tuple pattern, after the `(` and up to and including the `)`.
    fn arg_patterns(&mut self) -> Result<(Vec<ArgPat>, bool)> {
        let mut args = Vec::new();
        let mut rest = false;
        while !self.is_punct(")") {
            if self.eat_punct("..") {
                rest = true;
                break;
            }
            let pos = self.pos();
            let named = matches!(self.peek(), Tok::Name(_)) && self.peek_at(1) == &Tok::Punct(":");
            let name = if named {
                let Tok::Name(name) = self.peek().clone() else {
                    unreachable!("checked above")
                };
                self.bump();
                self.bump();
                Some(name)
            } else {
                None
            };
            args.push(ArgPat {
                name,
                pos,
                pattern: self.pattern(true)?,
            });
            if !self.eat_punct(",") {
                break;
            }
        }
        self.expect_punct(")")?;
        Ok((args, rest))
    }

    /// The elements of an array pattern, after the `[` and up to and including the `]`. A `..` may only come last.
    fn item_patterns(&mut self) -> Result<(Vec<Pattern>, bool)> {
        let mut items = Vec::new();
        let mut rest = false;
        while !self.is_punct("]") {
            if self.eat_punct("..") {
                rest = true;
                break;
            }
            items.push(self.pattern(true)?);
            if !self.eat_punct(",") {
                break;
            }
        }
        self.expect_punct("]")?;
        Ok((items, rest))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parses(text: &str) -> Expr {
        parse_query(text).unwrap_or_else(|e| panic!("{} did not parse: {}", text, e))
    }

    fn error(text: &str) -> String {
        match parse_query(text) {
            Ok(e) => panic!("{} parsed as {:?}", text, e),
            Err(d) => d.message,
        }
    }

    #[test]
    fn parses_a_method_chain() {
        let e = parses("@values.filter(|m| m.y > 0.0).map(|m| m.x)");
        let ExprKind::Method { name, receiver, .. } = e.kind else {
            panic!()
        };
        assert_eq!(name, "map");
        assert!(matches!(receiver.kind, ExprKind::Method { ref name, .. } if name == "filter"));
    }

    #[test]
    fn a_sign_that_touches_a_number_makes_a_literal() {
        assert!(matches!(parses("-1").kind, ExprKind::Lit(Lit::Int(ref t)) if t == "-1"));
        assert!(matches!(parses("n - 1").kind, ExprKind::Binary(BinOp::Sub, ..)));
        assert!(matches!(parses("n -1").kind, ExprKind::Binary(BinOp::Sub, ..)));
        assert!(matches!(parses("- x").kind, ExprKind::Unary(UnOp::Neg, _)));
    }

    #[test]
    fn as_binds_tighter_than_comparison() {
        let e = parses("m.y as Float > 0.0");
        let ExprKind::Binary(BinOp::Gt, left, _) = e.kind else {
            panic!()
        };
        assert!(matches!(left.kind, ExprKind::Cast(_, NumKind::Float)));
        assert!(error("x as f64").contains("Constructo type"));
    }

    #[test]
    fn field_shorthand_only_after_a_constructor() {
        parses("|m| Point { x, y }");
        assert_eq!(
            error("|m| { x }"),
            "A bare record names its fields: write x for the value, or { x: x } for a record"
        );
        parses("{ x: 1, ..r }");
    }

    #[test]
    fn if_has_two_forms() {
        parses("if a then 1 elseif b then 2 else 3 endif");
        parses("if a { 1 } else if b { 2 } else { 3 }");
        parses("if c then { x: 1 } else { x: 0 } endif");
        parses("if c { { x: 1 } } else { { x: 0 } }");
        assert!(error("if a then 1 endif").contains("needs an 'else'"));
        assert!(error("if a { 1 }").contains("needs an 'else'"));
    }

    #[test]
    fn a_braced_condition_is_read_again_without_a_constructor_record() {
        let e = parses("if v == None { x } else { y }");
        let ExprKind::If { branches, .. } = e.kind else {
            panic!()
        };
        assert!(matches!(branches[0].1.kind, ExprKind::Var(ref x) if x == "x"));
        // Read in full, the record belongs to the constructor when the branch follows it.
        let e = parses("if v == P { x } { 1 } else { 2 }");
        let ExprKind::If { branches, .. } = e.kind else {
            panic!()
        };
        assert!(matches!(branches[0].0.kind, ExprKind::Binary(BinOp::Eq, _, ref r)
            if matches!(r.kind, ExprKind::Ctor { body: Some(_), .. })));
        // The outfix form needs no second reading.
        parses("if v == P { x } then 1 else 2 endif");
    }

    #[test]
    fn match_has_two_forms() {
        parses("match s with Circle { r } => r, Rect { w, h } if w == h => w, Quit | Pause => 0.0, endmatch");
        parses("match s { Circle { r } => r, _ => 0.0 }");
        parses("match v == None { true => 1, false => 2 }");
    }

    #[test]
    fn patterns() {
        parses("@values.filter_match(|Move { x, .. }| x)");
        parses("matches!(v, Quit)");
        parses("match v with Record(a, right: b, ..) => a, n @ [1, ..] => n, _ => 0 endmatch");
        parses("match v with shapes::Circle { r } => r, true => 1, endmatch");
    }

    #[test]
    fn qualified_constructors_and_positions() {
        let e = parses("acme.shapes::Circle { r: 1.0 }");
        assert!(matches!(e.kind, ExprKind::Ctor { ref name, .. } if name == "acme.shapes::Circle"));
        let e = parses("v.0.1");
        assert!(matches!(e.kind, ExprKind::Position(_, 1)));
    }

    #[test]
    fn assignment_and_update() {
        parses("c.update(|c| c.server.tls.port = 443)");
        parses("v.update(|v| v.count += 1)");
    }

    #[test]
    fn comparisons_do_not_chain() {
        assert!(error("a < b < c").contains("cannot be chained"));
    }

    #[test]
    fn errors_say_what_was_expected() {
        assert_eq!(
            error("@values."),
            "Expected a field, a position or a method after '.' but found the end of the query"
        );
        assert!(error("@input").contains("@values"));
    }
}
