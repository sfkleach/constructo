//! The syntax trees for data-type declarations and data values.

use crate::diagnostics::Pos;

/// The declarations of a types file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TypesFile {
    pub enums: Vec<EnumDecl>,
    pub consts: Vec<ConstDecl>,
}

/// `const name<Params>: Type = value;`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstDecl {
    pub name: String,
    pub pos: Pos,
    pub params: Vec<TypeParam>,
    pub ty: Type,
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
    /// A tuple such as `Write(String)`, possibly empty as in `Ping()`.
    Tuple(Vec<Type>),
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
    /// A tuple, possibly empty as in `Ping()`.
    Tuple(Vec<Value>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub name: String,
    pub pos: Pos,
    pub value: Value,
}
