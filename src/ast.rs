//! The syntax tree for data-type declarations.

use crate::diagnostics::Pos;

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
