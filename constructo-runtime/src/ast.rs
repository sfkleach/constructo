//! The syntax trees for data-type declarations and data values.

use crate::diagnostics::Pos;
use std::fmt;

/// The declarations of a types file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TypesFile {
    /// The enums, in the order of the file.
    pub enums: Vec<EnumDecl>,
    /// The consts, in the order of the file.
    pub consts: Vec<ConstDecl>,
}

/// `const name<Params>: Type = value;`, in which the parameters and the type may be left out together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstDecl {
    /// The name of the const.
    pub name: String,
    /// The position of the name.
    pub pos: Pos,
    /// The type parameters, which are empty if there are none.
    pub params: Vec<TypeParam>,
    /// The declared type, or `None` if it is to be inferred from the value.
    pub ty: Option<Type>,
    /// The value of the const.
    pub value: Value,
}

/// `enum Name<Params> = Variants;`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumDecl {
    /// The name of the enum.
    pub name: String,
    /// The position of the name.
    pub pos: Pos,
    /// The type parameters, which are empty if there are none.
    pub params: Vec<TypeParam>,
    /// The variants, in the order of the file.
    pub variants: Vec<Variant>,
}

/// A type parameter of an enum or a const, such as the `T` of `enum List<T>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeParam {
    /// The name of the parameter.
    pub name: String,
    /// The position of the name.
    pub pos: Pos,
    /// True for a parameter written with a leading underscore, which is allowed to be unused.
    pub underscore: bool,
}

/// A variant of an enum, which is a constructor and what it takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variant {
    /// The name of the constructor.
    pub name: String,
    /// The position of the name.
    pub pos: Pos,
    /// What the constructor takes.
    pub body: VariantBody,
}

/// What the constructor of a variant takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VariantBody {
    /// A bare constructor such as `Quit`.
    Unit,
    /// A record such as `Move { x: Int, y: Int }`.
    Record(Vec<Field>),
    /// A tuple such as `Write(String)` or `Image(String, hOffset: Nat)`, possibly empty as in `Ping()`.
    Tuple(Vec<Parameter>),
    /// An array of one element type, as in `Users[User]`. It is a different form from `Users([User])`.
    Array(Type),
}

/// A parameter of a tuple constructor. Unnamed parameters come first and can only be bound by position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parameter {
    /// The name of the parameter, or `None` if it is unnamed.
    pub name: Option<String>,
    /// The position of the parameter.
    pub pos: Pos,
    /// The type of the parameter.
    pub ty: Type,
}

/// A field of a record type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    /// The name of the field.
    pub name: String,
    /// The position of the name.
    pub pos: Pos,
    /// The type of the field.
    pub ty: Type,
}

/// A type, as it is written in a declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    /// A named type with its arguments, such as `Nat` or `List<Int>`.
    Ref {
        /// The name of the type, as it is written.
        name: String,
        /// The position of the name.
        pos: Pos,
        /// The type arguments, which are empty if there are none.
        args: Vec<Type>,
    },
    /// An array of one element type, such as `[Nat]`.
    Array(Box<Type>),
    /// A record type without a name, such as `{ x: Int }`.
    Record(Vec<Field>),
}

/// A data value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Value {
    /// The position of the start of the value.
    pub pos: Pos,
    /// What the value is.
    pub kind: ValueKind,
}

/// The kinds of a data value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueKind {
    /// The literals keep their text, so that checking them against a type can decide how to interpret them.
    /// An unsigned integer literal, such as `42`.
    Nat(String),
    /// A signed integer literal, such as `+8` or `-1`.
    Int(String),
    /// A floating-point literal, such as `3.0` or `-2.5e3`.
    Float(String),
    /// A string literal, with its escapes decoded.
    Str(String),
    /// An array, such as `[1, 2]`.
    Array(Vec<Value>),
    /// A record without a constructor, such as `{ x: 1 }`.
    Record(Vec<Member>),
    /// A constructor, optionally followed by a record, a tuple or an array, as in `Quit`, `Move { x: 1 }`,
    /// `Write("hi")` or `Users[u1, u2]`.
    /// A name that starts with a lower-case letter or `_` is a reference to a const, which takes no body, and any other
    /// name is a constructor.
    Tagged {
        /// The name, as it is written, which may be qualified.
        name: String,
        /// What follows the name, or `None` if nothing does.
        body: Option<TaggedBody>,
    },
}

/// What follows the name of a constructor in a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaggedBody {
    /// A record, such as the `{ x: 1 }` of `Move { x: 1 }`.
    Record(Vec<Member>),
    /// A tuple, possibly empty as in `Ping()`. Positional arguments come before any named ones.
    Tuple(Vec<Argument>),
    /// An array, possibly empty as in `Users[]`. It has no names, so every element is an ordinary value.
    Array(Vec<Value>),
}

/// An argument of a tuple constructor, which is either positional or given by the name of a parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Argument {
    /// The name of the parameter, or `None` if the argument is positional.
    pub name: Option<String>,
    /// The position of the argument.
    pub pos: Pos,
    /// The value of the argument.
    pub value: Value,
}

/// A member of a record value, `name: value`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    /// The name of the member.
    pub name: String,
    /// The position of the name.
    pub pos: Pos,
    /// The value of the member.
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
                    Some(TaggedBody::Array(items)) => items.iter().for_each(|item| item.names(out)),
                    None => {}
                }
            }
            ValueKind::Nat(_) | ValueKind::Int(_) | ValueKind::Float(_) | ValueKind::Str(_) => {}
        }
    }
}
