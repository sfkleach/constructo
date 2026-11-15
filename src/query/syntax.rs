//! The syntax tree of a query. See `docs/decisions/0044-queries-are-rust-like-expressions-over-a-stream-of-values.md`
//! and the settled questions of `docs/tasks/done/2026-11-15-query-subcommand/query-subcommand.md`.

use crate::diagnostics::Pos;
use std::rc::Rc;

/// The kinds of number, as `as` names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumKind {
    Nat,
    Int,
    Float,
}

impl NumKind {
    pub fn name(self) -> &'static str {
        match self {
            NumKind::Nat => "Nat",
            NumKind::Int => "Int",
            NumKind::Float => "Float",
        }
    }
}

/// A literal as it is written, so that its text can be kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lit {
    Nat(String),
    Int(String),
    Float(String),
    Str(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

impl BinOp {
    pub fn symbol(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Rem => "%",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::And => "&&",
            BinOp::Or => "||",
        }
    }

    pub fn is_arithmetic(self) -> bool {
        matches!(self, BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem)
    }

    pub fn is_ordering(self) -> bool {
        matches!(self, BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Not,
    Neg,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub pos: Pos,
    pub kind: ExprKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    /// `#values`, the stream of the input values.
    Values,
    /// A lower-case name: a variable that a pattern bound, or a const of the prelude such as `true`.
    Var(String),
    /// A const of the types, possibly qualified, as `origin` or `shapes::origin`. In a typed query, a name that no
    /// pattern binds is a const, other than those of the prelude, which stay `Var`s.
    Const(String),
    Lit(Lit),
    /// A constructor, possibly qualified, with an optional body.
    Ctor {
        name: String,
        body: Option<CtorBody>,
    },
    /// A bare record, with an optional base for struct update: `{ a: 1, ..r }`.
    Record {
        fields: Vec<FieldInit>,
        base: Option<Box<Expr>>,
    },
    Array(Vec<Expr>),
    /// `v.x`, a field of a record or a named argument of a tuple.
    Field(Box<Expr>, String),
    /// `v.0`, an argument of a tuple by position.
    Position(Box<Expr>, usize),
    /// `v[i]`.
    Index(Box<Expr>, Box<Expr>),
    /// `v.name(args)`.
    Method {
        receiver: Box<Expr>,
        name: String,
        args: Vec<Expr>,
    },
    /// A closure, which is only allowed as the argument of a method.
    Closure(Rc<Closure>),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Cast(Box<Expr>, NumKind),
    /// `if` in either form, with the `elseif` (or `else if`) branches flattened.
    If {
        branches: Vec<(Expr, Expr)>,
        otherwise: Box<Expr>,
    },
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<Arm>,
    },
    /// `matches!(e, pattern)`.
    Matches(Box<Expr>, Pattern),
    /// An assignment to a path, `target op= value`, which is only allowed as the body of a closure given to `update`.
    Assign {
        target: Box<Expr>,
        op: Option<BinOp>,
        value: Box<Expr>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum CtorBody {
    Record {
        fields: Vec<FieldInit>,
        base: Option<Box<Expr>>,
    },
    Tuple(Vec<ArgInit>),
    Array(Vec<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldInit {
    pub name: String,
    pub pos: Pos,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArgInit {
    pub name: Option<String>,
    pub pos: Pos,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Closure {
    pub pos: Pos,
    pub params: Vec<Pattern>,
    pub body: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Arm {
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pattern {
    pub pos: Pos,
    pub kind: PatternKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PatternKind {
    /// `_`.
    Wild,
    /// A name, which binds whatever is there.
    Bind(String),
    /// `name @ pattern`.
    At(String, Box<Pattern>),
    Lit(Lit),
    /// A constructor, possibly qualified, with an optional body.
    Ctor {
        name: String,
        body: Option<PatBody>,
    },
    /// A bare record: `{ x, y: p, .. }`.
    Record {
        fields: Vec<FieldPat>,
        rest: bool,
    },
    /// An array: `[a, b, ..]`, or `[a, b, rest @ ..]`, which binds the rest to an array.
    Array {
        items: Vec<Pattern>,
        rest: bool,
        rest_as: Option<String>,
    },
    /// `p | q`.
    Or(Vec<Pattern>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum PatBody {
    Record {
        fields: Vec<FieldPat>,
        rest: bool,
    },
    Tuple {
        args: Vec<ArgPat>,
        rest: bool,
    },
    /// The elements of an array body, as for `PatternKind::Array`.
    Array {
        items: Vec<Pattern>,
        rest: bool,
        rest_as: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldPat {
    pub name: String,
    pub pos: Pos,
    pub pattern: Pattern,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArgPat {
    pub name: Option<String>,
    pub pos: Pos,
    pub pattern: Pattern,
}

impl Pattern {
    /// Whether the pattern matches every value of the right shape without looking at which constructor or literal it
    /// is. Without the types no constructor can be known to be the only one of its enum, so a constructor is always
    /// refutable here.
    pub fn is_irrefutable(&self) -> bool {
        match &self.kind {
            PatternKind::Wild | PatternKind::Bind(_) => true,
            PatternKind::At(_, inner) => inner.is_irrefutable(),
            PatternKind::Record { fields, .. } => fields.iter().all(|f| f.pattern.is_irrefutable()),
            PatternKind::Or(alternatives) => alternatives.iter().any(|p| p.is_irrefutable()),
            PatternKind::Lit(_) | PatternKind::Ctor { .. } | PatternKind::Array { .. } => false,
        }
    }
}
