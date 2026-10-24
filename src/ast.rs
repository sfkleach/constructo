//! The syntax trees for data-type declarations and data values.

use crate::diagnostics::Pos;
use std::fmt;

/// The declarations of a types file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TypesFile {
    pub enums: Vec<EnumDecl>,
    pub consts: Vec<ConstDecl>,
}

/// `const name<Params>: Type = value;`, in which the parameters and the type may be left out together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstDecl {
    pub name: String,
    pub pos: Pos,
    pub params: Vec<TypeParam>,
    /// The declared type, or `None` if it is to be inferred from the value.
    pub ty: Option<Type>,
    pub value: Value,
}

/// `enum Name<Params> = Variants;`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumDecl {
    pub name: String,
    pub pos: Pos,
    pub params: Vec<TypeParam>,
    pub variants: Vec<Variant>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeParam {
    pub name: String,
    pub pos: Pos,
    /// True for a parameter written with a leading underscore, which is allowed to be unused.
    pub underscore: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variant {
    pub name: String,
    pub pos: Pos,
    pub body: VariantBody,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VariantBody {
    /// A bare constructor such as `Quit`.
    Unit,
    Record(Vec<Field>),
    /// A tuple such as `Write(String)` or `Image(String, hOffset: Nat)`, possibly empty as in `Ping()`.
    Tuple(Vec<Parameter>),
}

/// A parameter of a tuple constructor. Unnamed parameters come first and can only be bound by position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parameter {
    pub name: Option<String>,
    pub pos: Pos,
    pub ty: Type,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub pos: Pos,
    pub ty: Type,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Ref { name: String, pos: Pos, args: Vec<Type> },
    Array(Box<Type>),
    Record(Vec<Field>),
}

/// A data value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Value {
    pub pos: Pos,
    pub kind: ValueKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueKind {
    /// The literals keep their text, so that checking them against a type can decide how to interpret them.
    Nat(String),
    Int(String),
    Float(String),
    Str(String),
    Array(Vec<Value>),
    Record(Vec<Member>),
    /// A constructor, optionally followed by a record or a tuple, as in `Quit`, `Move { x: 1 }` or `Write("hi")`.
    /// A bare name with no body may instead be a reference to a const, and a lowercase name always is.
    Tagged {
        name: String,
        body: Option<TaggedBody>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaggedBody {
    Record(Vec<Member>),
    /// A tuple, possibly empty as in `Ping()`. Positional arguments come before any named ones.
    Tuple(Vec<Argument>),
}

/// An argument of a tuple constructor, which is either positional or given by the name of a parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Argument {
    pub name: Option<String>,
    pub pos: Pos,
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub name: String,
    pub pos: Pos,
    pub value: Value,
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Ref { name, args, .. } => {
                write!(f, "{}", name)?;
                if !args.is_empty() {
                    let args: Vec<String> = args.iter().map(|t| t.to_string()).collect();
                    write!(f, "<{}>", args.join(", "))?;
                }
                Ok(())
            }
            Type::Array(element) => write!(f, "[{}]", element),
            Type::Record(fields) if fields.is_empty() => write!(f, "{{}}"),
            Type::Record(fields) => {
                let fields: Vec<String> = fields
                    .iter()
                    .map(|field| format!("{}: {}", field.name, field.ty))
                    .collect();
                write!(f, "{{ {} }}", fields.join(", "))
            }
        }
    }
}

impl Value {
    /// Collects the names that the value uses on their own or as the tag of a body, with their positions. Any of them
    /// may refer to a const, which is for the caller to decide.
    pub fn names<'a>(&'a self, out: &mut Vec<(&'a str, Pos)>) {
        match &self.kind {
            ValueKind::Array(items) => items.iter().for_each(|item| item.names(out)),
            ValueKind::Record(members) => members.iter().for_each(|m| m.value.names(out)),
            ValueKind::Tagged { name, body } => {
                out.push((name, self.pos));
                match body {
                    Some(TaggedBody::Record(members)) => members.iter().for_each(|m| m.value.names(out)),
                    Some(TaggedBody::Tuple(items)) => items.iter().for_each(|item| item.value.names(out)),
                    None => {}
                }
            }
            ValueKind::Nat(_) | ValueKind::Int(_) | ValueKind::Float(_) | ValueKind::Str(_) => {}
        }
    }
}
