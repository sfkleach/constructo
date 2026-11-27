//! A stand-in for `constructo-runtime`: the syntax layer of `constructo`, and helpers that need no trait. It has no
//! traits of its own, since those are generated with each codec.

pub use constructo::ast::{Argument, Member, TaggedBody, Value, ValueKind};
pub use constructo::diagnostics::Pos;

use std::fmt;

/// An error of reading or writing, at a position in the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub pos: Pos,
    pub message: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.pos == NOWHERE {
            write!(f, "{}", self.message)
        } else {
            write!(f, "{} {}", self.pos, self.message)
        }
    }
}

impl std::error::Error for Error {}

/// The position of a value that was built by a program and not read.
pub const NOWHERE: Pos = Pos { line: 0, col: 0 };

/// The position of the start of a text.
pub const START: Pos = Pos { line: 1, col: 1 };

pub fn at(pos: Pos, message: impl Into<String>) -> Error {
    Error {
        pos,
        message: message.into(),
    }
}

/// The limits of a codec. The parser has its own nesting limit, which is not repeated here.
#[derive(Debug, Clone)]
pub struct Limits {
    /// How deep a const may refer to another before the reading is refused.
    pub const_depth: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { const_depth: 64 }
    }
}

/// Every value of a values text.
pub fn parse(text: &str) -> Result<Vec<Value>, Error> {
    constructo::parser::parse_values(text).map_err(|e| at(e.pos, e.message))
}

/// The one value of a text, such as the value of a const.
pub fn parse_one(text: &str) -> Result<Value, Error> {
    let mut values = parse(text)?;
    match values.len() {
        1 => Ok(values.remove(0)),
        n => Err(at(START, format!("Expected one value but found {n}"))),
    }
}

/// The text of a value, ended by `;` and a new line.
pub fn render(value: &Value) -> String {
    constructo::values_text::render(value, false)
}

/// The local part of a full name, `Circle` of `com.example.shapes::Circle`, for messages.
pub fn bare(name: &str) -> &str {
    name.rsplit("::").next().unwrap_or(name)
}

/// A name that starts with a lower-case letter or `_` is a const.
pub fn is_const_name(name: &str) -> bool {
    let local = name.rsplit("::").next().unwrap_or(name);
    local.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
}

/// What a value is, for a message.
pub fn found(value: &Value) -> String {
    match &value.kind {
        ValueKind::Nat(t) | ValueKind::Int(t) | ValueKind::Float(t) => format!("the number {t}"),
        ValueKind::Str(_) => "a string".to_string(),
        ValueKind::Array(_) => "an array".to_string(),
        ValueKind::Record(_) => "a record".to_string(),
        ValueKind::Tagged { name, .. } => bare(name).to_string(),
    }
}

pub fn expected(what: &str, value: &Value) -> Error {
    at(value.pos, format!("Expected {what} but found {}", found(value)))
}

/// A constructor that is written with the wrong body, as `Circle(1.0)`.
pub fn takes(name: &str, body: &str, value: &Value) -> Error {
    at(value.pos, format!("Constructor {} takes {body}", bare(name)))
}

/// The fields of a record, in the order of `names`. Every field is required, and no other is allowed.
pub fn fields<'a, const N: usize>(
    members: &'a [Member],
    names: [&str; N],
    owner: &str,
    pos: Pos,
) -> Result<[&'a Value; N], Error> {
    let mut slots: [Option<&'a Value>; N] = [None; N];
    for member in members {
        let Some(i) = names.iter().position(|n| *n == member.name) else {
            return Err(at(
                member.pos,
                format!("Unknown field {} in {}", member.name, bare(owner)),
            ));
        };
        if slots[i].replace(&member.value).is_some() {
            return Err(at(member.pos, format!("Duplicate field {}", member.name)));
        }
    }
    let mut out: [&'a Value; N] = [&PLACEHOLDER; N];
    for (i, slot) in slots.iter().enumerate() {
        match slot {
            Some(value) => out[i] = value,
            None => return Err(at(pos, format!("Missing field {} in {}", names[i], bare(owner)))),
        }
    }
    Ok(out)
}

static PLACEHOLDER: Value = Value {
    pos: NOWHERE,
    kind: ValueKind::Array(Vec::new()),
};

/// The arguments of a tuple constructor, in the order of its parameters. Positional arguments bind a prefix, and named
/// ones follow. A parameter with a name may be left out, and is then `None`; one without a name may not.
pub fn arguments<'a, const N: usize>(
    args: &'a [Argument],
    params: [Option<&str>; N],
    owner: &str,
    pos: Pos,
) -> Result<[Option<&'a Value>; N], Error> {
    let mut slots: [Option<&'a Value>; N] = [None; N];
    let mut next = 0;
    for arg in args {
        let i = match &arg.name {
            None => {
                next += 1;
                next - 1
            }
            Some(name) => match params.iter().position(|p| *p == Some(name.as_str())) {
                Some(i) => i,
                None => return Err(at(arg.pos, format!("Unknown argument {name} in {}", bare(owner)))),
            },
        };
        if i >= N {
            return Err(at(
                pos,
                format!("Constructor {} takes {N} value(s) but has {}", bare(owner), args.len()),
            ));
        }
        if slots[i].replace(&arg.value).is_some() {
            let name = params[i].unwrap_or("?");
            return Err(at(
                arg.pos,
                format!("Argument {name} is given more than once in {}", bare(owner)),
            ));
        }
    }
    for (i, param) in params.iter().enumerate() {
        if param.is_none() && slots[i].is_none() {
            return Err(at(
                pos,
                format!("Constructor {} takes {N} value(s) but has {}", bare(owner), args.len()),
            ));
        }
    }
    Ok(slots)
}

/// What a rewrite of a name gives: another name, or a whole value in place of a reference to a const.
pub enum Rewrite {
    Name(String),
    Value(Value),
}

/// A copy of a value, with each name, of a constructor or a const, rewritten.
pub fn rewrite(value: &Value, f: &mut dyn FnMut(&str, Pos) -> Result<Rewrite, Error>) -> Result<Value, Error> {
    let kind = match &value.kind {
        ValueKind::Array(items) => ValueKind::Array(items.iter().map(|v| rewrite(v, f)).collect::<Result<_, _>>()?),
        ValueKind::Record(members) => ValueKind::Record(rewrite_members(members, f)?),
        ValueKind::Tagged { name, body } => {
            let name = match f(name, value.pos)? {
                Rewrite::Name(name) => name,
                Rewrite::Value(v) => return Ok(v),
            };
            let body = match body {
                None => None,
                Some(TaggedBody::Record(members)) => Some(TaggedBody::Record(rewrite_members(members, f)?)),
                Some(TaggedBody::Array(items)) => Some(TaggedBody::Array(
                    items.iter().map(|v| rewrite(v, f)).collect::<Result<_, _>>()?,
                )),
                Some(TaggedBody::Tuple(args)) => Some(TaggedBody::Tuple(
                    args.iter()
                        .map(|a| {
                            Ok(Argument {
                                name: a.name.clone(),
                                pos: a.pos,
                                value: rewrite(&a.value, f)?,
                            })
                        })
                        .collect::<Result<_, Error>>()?,
                )),
            };
            ValueKind::Tagged { name, body }
        }
        other => other.clone(),
    };
    Ok(Value { pos: value.pos, kind })
}

fn rewrite_members(
    members: &[Member],
    f: &mut dyn FnMut(&str, Pos) -> Result<Rewrite, Error>,
) -> Result<Vec<Member>, Error> {
    members
        .iter()
        .map(|m| {
            Ok(Member {
                name: m.name.clone(),
                pos: m.pos,
                value: rewrite(&m.value, f)?,
            })
        })
        .collect()
}

/// A value with every position set to `pos`, which is where a const was referred to.
pub fn relocate(value: Value, pos: Pos) -> Value {
    rewrite(&value, &mut |name, _| Ok(Rewrite::Name(name.to_string())))
        .map(|v| set_pos(v, pos))
        .unwrap_or(value)
}

fn set_pos(mut value: Value, pos: Pos) -> Value {
    value.pos = pos;
    value.kind = match value.kind {
        ValueKind::Array(items) => ValueKind::Array(items.into_iter().map(|v| set_pos(v, pos)).collect()),
        ValueKind::Record(members) => ValueKind::Record(
            members
                .into_iter()
                .map(|m| Member {
                    name: m.name,
                    pos,
                    value: set_pos(m.value, pos),
                })
                .collect(),
        ),
        ValueKind::Tagged { name, body } => ValueKind::Tagged {
            name,
            body: body.map(|b| match b {
                TaggedBody::Record(ms) => TaggedBody::Record(
                    ms.into_iter()
                        .map(|m| Member {
                            name: m.name,
                            pos,
                            value: set_pos(m.value, pos),
                        })
                        .collect(),
                ),
                TaggedBody::Array(items) => TaggedBody::Array(items.into_iter().map(|v| set_pos(v, pos)).collect()),
                TaggedBody::Tuple(args) => TaggedBody::Tuple(
                    args.into_iter()
                        .map(|a| Argument {
                            name: a.name,
                            pos,
                            value: set_pos(a.value, pos),
                        })
                        .collect(),
                ),
            }),
        },
        other => other,
    };
    value
}

// Builders, for writing.

fn value(kind: ValueKind) -> Value {
    Value { pos: NOWHERE, kind }
}

pub fn nat(text: String) -> Value {
    value(ValueKind::Nat(text))
}

pub fn int(text: String) -> Value {
    value(ValueKind::Int(text))
}

pub fn float(text: String) -> Value {
    value(ValueKind::Float(text))
}

pub fn string(text: &str) -> Value {
    value(ValueKind::Str(text.to_string()))
}

pub fn array(items: Vec<Value>) -> Value {
    value(ValueKind::Array(items))
}

pub fn record(members: Vec<(&str, Value)>) -> Value {
    value(ValueKind::Record(make_members(members)))
}

pub fn unit(name: &str) -> Value {
    value(ValueKind::Tagged {
        name: name.to_string(),
        body: None,
    })
}

pub fn tagged_record(name: &str, members: Vec<(&str, Value)>) -> Value {
    value(ValueKind::Tagged {
        name: name.to_string(),
        body: Some(TaggedBody::Record(make_members(members))),
    })
}

pub fn tagged_tuple(name: &str, args: Vec<(Option<&str>, Value)>) -> Value {
    let args = args
        .into_iter()
        .map(|(n, v)| Argument {
            name: n.map(str::to_string),
            pos: NOWHERE,
            value: v,
        })
        .collect();
    value(ValueKind::Tagged {
        name: name.to_string(),
        body: Some(TaggedBody::Tuple(args)),
    })
}

pub fn tagged_array(name: &str, items: Vec<Value>) -> Value {
    value(ValueKind::Tagged {
        name: name.to_string(),
        body: Some(TaggedBody::Array(items)),
    })
}

fn make_members(members: Vec<(&str, Value)>) -> Vec<Member> {
    members
        .into_iter()
        .map(|(n, v)| Member {
            name: n.to_string(),
            pos: NOWHERE,
            value: v,
        })
        .collect()
}
