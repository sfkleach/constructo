//! What the codec that `constructo codegen` generates calls and needs no trait for: the error of reading or writing a
//! value, the setting of how names are written, the parts of a record and a tuple in the order of their declaration,
//! the numbers, and the builders of a tree.
//!
//! The traits and the tables of names and consts are generated with each codec, in the program's own crate, so that
//! they can be implemented for a type of any crate. What is here is the same for every codec. Its messages are those of
//! `constructo check`, where the error is one that `check` finds.

use crate::ast::{Argument, Member, TaggedBody, Value, ValueKind};
use crate::diagnostics::Pos;
use crate::parser::parse_values;
use crate::values_text::render;
use std::fmt;

/// How a codec writes the name of a constructor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Names {
    /// As the project that the code was generated for writes it: bare, else through a group, else in full.
    #[default]
    Project,
    /// In full, as `com.example.shapes::Circle`, which any project reads.
    Full,
}

/// A step of the path from the value that was read or written to the value that an error is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// A field of a record, or a parameter of a tuple, by name.
    Field(String),
    /// A parameter of a tuple that has no name, by its position from 0.
    Position(usize),
    /// An element of an array, by its index from 0.
    Index(usize),
}

/// An error of reading or of writing a value.
///
/// It has the message, and the path in the value, by field and index, which is added to as the error passes out of each
/// field and element, so that it is built only when there is an error. An error of reading has the position in the
/// text. An error in the value of a const has the position of the reference to it, and the const is named.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    message: String,
    pos: Option<Pos>,
    /// The steps from the innermost out, which is the order in which they are added.
    path: Vec<Step>,
    /// The consts that the error is inside, from the outermost in.
    consts: Vec<String>,
}

impl Error {
    /// An error at a position in the text that is read.
    pub fn at(pos: Pos, message: impl Into<String>) -> Error {
        Error {
            message: message.into(),
            pos: Some(pos),
            path: Vec::new(),
            consts: Vec::new(),
        }
    }

    /// An error that has no position, as an error of writing has none.
    pub fn new(message: impl Into<String>) -> Error {
        Error {
            message: message.into(),
            pos: None,
            path: Vec::new(),
            consts: Vec::new(),
        }
    }

    /// The error with a step in front of its path, as it passes out of the value that the step leads to.
    pub fn step(mut self, step: Step) -> Error {
        self.path.push(step);
        self
    }

    /// The error inside the value of a const, as it passes out of the reference to the const, at `pos`. The position
    /// in the text of the const's value means nothing to the reader of the message, so it is replaced by that of the
    /// reference.
    pub fn in_const(mut self, name: &str, pos: Pos) -> Error {
        self.pos = Some(pos);
        self.consts.insert(0, name.to_string());
        self
    }

    /// The message, without the path, the consts or the position.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The position in the text, if the error is one of reading.
    pub fn pos(&self) -> Option<Pos> {
        self.pos
    }

    /// The path in the value, from the outside in, as in `shapes[0].colour`, which is empty for the value itself.
    pub fn path(&self) -> String {
        let mut out = String::new();
        for step in self.path.iter().rev() {
            match step {
                Step::Field(name) => {
                    if !out.is_empty() {
                        out.push('.');
                    }
                    out.push_str(name);
                }
                Step::Position(at) => {
                    if !out.is_empty() {
                        out.push('.');
                    }
                    out.push_str(&at.to_string());
                }
                Step::Index(at) => out.push_str(&format!("[{}]", at)),
            }
        }
        out
    }

    /// The consts that the error is inside, from the outermost in.
    pub fn consts(&self) -> &[String] {
        &self.consts
    }
}

/// Shown as `shapes[0].colour: in the value of const c: MESSAGE (line 3, col 12)`, with each part only if there is one.
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let path = self.path();
        if !path.is_empty() {
            write!(f, "{}: ", path)?;
        }
        for name in &self.consts {
            write!(f, "in the value of const {}: ", name)?;
        }
        write!(f, "{}", self.message)?;
        if let Some(pos) = self.pos {
            write!(f, " ({})", pos)?;
        }
        Ok(())
    }
}

impl std::error::Error for Error {}

/// The values of a text.
pub fn parse(text: &str) -> Result<Vec<Value>, Error> {
    parse_values(text).map_err(|e| Error::at(e.pos, e.message))
}

/// The one value of a text, which is an error at the start of the text if it has more or fewer.
pub fn parse_one(text: &str) -> Result<Value, Error> {
    let mut values = parse(text)?;
    match values.len() {
        1 => Ok(values.remove(0)),
        n => Err(Error::at(
            Pos { line: 1, col: 1 },
            format!("Expected one value but found {}", n),
        )),
    }
}

/// The text of the values, each ended by `;` and a new line, as `convert` writes them.
pub fn write_text(values: &[Value]) -> String {
    values.iter().map(|value| render(value, false)).collect()
}

/// The depth of a value inside one at `depth`, which is an error if it is more than `max`. Each value is a level, a
/// scalar included, as the parser counts them, so a value that is read is never deeper than one that the parser reads,
/// and one that is written can be read back.
pub fn enter(depth: u32, max: u32, pos: Option<Pos>) -> Result<u32, Error> {
    if depth >= max {
        let message = "Nesting is too deep";
        return Err(match pos {
            Some(pos) => Error::at(pos, message),
            None => Error::new(message),
        });
    }
    Ok(depth + 1)
}

/// What a value is, for a message, as `check` says it: `Nat`, `Int`, `Float`, `String`, `an array` or `a record`. A
/// tagged value is its name as it is written, which a codec can give with its type.
pub fn found(value: &Value) -> String {
    match &value.kind {
        ValueKind::Nat(_) => "Nat".to_string(),
        ValueKind::Int(_) => "Int".to_string(),
        ValueKind::Float(_) => "Float".to_string(),
        ValueKind::Str(_) => "String".to_string(),
        ValueKind::Array(_) => "an array".to_string(),
        ValueKind::Record(_) => "a record".to_string(),
        ValueKind::Tagged { name, .. } => name.clone(),
    }
}

/// The error for a value that is not of the type that is expected: `Expected Nat but found String`.
pub fn expected(what: &str, value: &Value) -> Error {
    Error::at(value.pos, format!("Expected {} but found {}", what, found(value)))
}

/// The members of a record, or of a constructor with a record, in the order of `names`, which are the fields. Every
/// field is required and no other is allowed, and the errors are `check`'s, in its order: a duplicate or an unknown
/// field at the member, then a missing one at the record. `owner` is the constructor, or `record` for a record
/// without a name.
pub fn fields<'a, const N: usize>(
    members: &'a [Member],
    names: [&str; N],
    owner: &str,
    pos: Pos,
) -> Result<[&'a Value; N], Error> {
    let mut slots: [Option<&'a Value>; N] = [None; N];
    for (index, member) in members.iter().enumerate() {
        if members[..index].iter().any(|m| m.name == member.name) {
            return Err(Error::at(member.pos, format!("Duplicate field {}", member.name)));
        }
        match names.iter().position(|name| *name == member.name) {
            Some(at) => slots[at] = Some(&member.value),
            None => {
                return Err(Error::at(
                    member.pos,
                    format!("Unknown field {} in {}", member.name, owner),
                ))
            }
        }
    }
    if let Some(at) = slots.iter().position(Option::is_none) {
        return Err(Error::at(pos, format!("Missing field {} in {}", names[at], owner)));
    }
    Ok(slots.map(|slot| slot.expect("every field has been found")))
}

/// The arguments of a constructor with a tuple, in the order of its parameters, which are given by their names, or
/// `None` for a parameter that has none. The arguments without a name bind the parameters in order, and those with a
/// name bind the parameter of that name, which may be one that could have been bound in order. A named parameter that
/// is not bound is `None`, and takes the default of its type; an unnamed one is an error. The errors are `check`'s, in
/// its order.
pub fn arguments<'a, const N: usize>(
    args: &'a [Argument],
    params: [Option<&str>; N],
    owner: &str,
    pos: Pos,
) -> Result<[Option<&'a Value>; N], Error> {
    let count = || format!("Constructor {} takes {} value(s) but has {}", owner, N, args.len());
    let mut slots: [Option<&'a Value>; N] = [None; N];
    for (index, arg) in args.iter().enumerate() {
        let target = match &arg.name {
            None if index < N => index,
            None => return Err(Error::at(arg.pos, count())),
            Some(name) => match params.iter().position(|p| *p == Some(name.as_str())) {
                Some(target) => target,
                None => return Err(Error::at(arg.pos, format!("Unknown argument {} in {}", name, owner))),
            },
        };
        if slots[target].is_some() {
            let message = match params[target] {
                Some(name) => format!("Argument {} is given more than once in {}", name, owner),
                None => format!("Argument {} is given more than once in {}", target + 1, owner),
            };
            return Err(Error::at(arg.pos, message));
        }
        slots[target] = Some(&arg.value);
    }
    if (0..N).any(|at| slots[at].is_none() && params[at].is_none()) {
        return Err(Error::at(pos, count()));
    }
    Ok(slots)
}

/// A `Nat` from its text. A sign makes a number an `Int`, which is never a `Nat`, as in `check`, and a number that is
/// too large for a `u64` is an error at it.
pub fn read_nat(value: &Value) -> Result<u64, Error> {
    match &value.kind {
        ValueKind::Nat(text) => text.parse().map_err(|_| does_not_fit(value, text, "Nat")),
        _ => Err(expected("Nat", value)),
    }
}

/// An `Int` from its text, or from that of a `Nat` if `widening` is set. A number that is too large for an `i64` is an
/// error at it.
pub fn read_int(value: &Value, widening: bool) -> Result<i64, Error> {
    match &value.kind {
        ValueKind::Int(text) => text.parse().map_err(|_| does_not_fit(value, text, "Int")),
        ValueKind::Nat(text) if widening => text.parse().map_err(|_| does_not_fit(value, text, "Int")),
        _ => Err(expected("Int", value)),
    }
}

/// A `Float` from its text, or from that of a `Nat` or an `Int` if `widening` is set: the nearest `f64`, which is the
/// one loss that a mapping is allowed. A number too small to be held other than as zero is zero, and one too large
/// for an `f64`, such as `1.0e400`, is an error at it, since making it infinite would change a finite value into one
/// that is not. There is no negative zero, so `-0.0` is `0.0`.
pub fn read_float(value: &Value, widening: bool) -> Result<f64, Error> {
    let text = match &value.kind {
        ValueKind::Float(text) => text,
        ValueKind::Nat(text) | ValueKind::Int(text) if widening => text,
        _ => return Err(expected("Float", value)),
    };
    match text.parse::<f64>() {
        Ok(number) if number.is_finite() => Ok(if number == 0.0 { 0.0 } else { number }),
        _ => Err(does_not_fit(value, text, "Float")),
    }
}

fn does_not_fit(value: &Value, text: &str, what: &str) -> Error {
    Error::at(value.pos, format!("The number {} does not fit {}", text, what))
}

/// A string from its value.
pub fn read_string(value: &Value) -> Result<String, Error> {
    match &value.kind {
        ValueKind::Str(text) => Ok(text.clone()),
        _ => Err(expected("String", value)),
    }
}

// The builders of a tree, for writing. A value that a program builds has no position in a text, so each is given the
// first, which `render` does not write.

fn built(kind: ValueKind) -> Value {
    Value {
        pos: Pos { line: 1, col: 1 },
        kind,
    }
}

/// A `Nat`.
pub fn nat(number: u64) -> Value {
    built(ValueKind::Nat(number.to_string()))
}

/// An `Int`, with its sign, which a number that is not negative has as `+`, as `convert` writes it.
pub fn int(number: i64) -> Value {
    let text = if number < 0 {
        number.to_string()
    } else {
        format!("+{}", number)
    };
    built(ValueKind::Int(text))
}

/// A `Float`, as the shortest text that reads back as the same `f64`. Rust's own shortest text is used, with `.0` put
/// in where it has no point, as in `1.0e300`, and a zero is written as `0.0`, since there is no negative zero. A value
/// that is not finite has no text yet, and is an error.
pub fn float(number: f64) -> Result<Value, Error> {
    if !number.is_finite() {
        return Err(Error::new(format!(
            "The Float {} cannot be written, since Constructo has no text for it yet",
            number
        )));
    }
    if number == 0.0 {
        return Ok(built(ValueKind::Float("0.0".to_string())));
    }
    let text = format!("{:?}", number);
    let text = match text.split_once('e') {
        Some((digits, exponent)) if !digits.contains('.') => format!("{}.0e{}", digits, exponent),
        _ if !text.contains('.') => format!("{}.0", text),
        _ => text,
    };
    Ok(built(ValueKind::Float(text)))
}

/// A `String`.
pub fn string(text: &str) -> Value {
    built(ValueKind::Str(text.to_string()))
}

/// An array.
pub fn array(items: Vec<Value>) -> Value {
    built(ValueKind::Array(items))
}

/// A record without a name, with its members in the order given.
pub fn record(members: Vec<(&str, Value)>) -> Value {
    built(ValueKind::Record(make_members(members)))
}

/// A constructor with no body, or a reference to a const.
pub fn unit(name: &str) -> Value {
    built(ValueKind::Tagged {
        name: name.to_string(),
        body: None,
    })
}

/// A constructor with a record.
pub fn tagged_record(name: &str, members: Vec<(&str, Value)>) -> Value {
    tagged(name, TaggedBody::Record(make_members(members)))
}

/// A constructor with a tuple, each argument with its name if it is written by name.
pub fn tagged_tuple(name: &str, args: Vec<(Option<&str>, Value)>) -> Value {
    let args = args
        .into_iter()
        .map(|(name, value)| Argument {
            name: name.map(str::to_string),
            pos: value.pos,
            value,
        })
        .collect();
    tagged(name, TaggedBody::Tuple(args))
}

/// A constructor with an array.
pub fn tagged_array(name: &str, items: Vec<Value>) -> Value {
    tagged(name, TaggedBody::Array(items))
}

fn tagged(name: &str, body: TaggedBody) -> Value {
    built(ValueKind::Tagged {
        name: name.to_string(),
        body: Some(body),
    })
}

fn make_members(members: Vec<(&str, Value)>) -> Vec<Member> {
    members
        .into_iter()
        .map(|(name, value)| Member {
            name: name.to_string(),
            pos: value.pos,
            value,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::limits::MAX_DEPTH;

    fn one(text: &str) -> Value {
        parse_one(text).unwrap()
    }

    fn written(value: &Value) -> String {
        write_text(std::slice::from_ref(value))
            .trim_end_matches(";\n")
            .to_string()
    }

    fn members(value: &Value) -> &[Member] {
        match &value.kind {
            ValueKind::Record(members) => members,
            ValueKind::Tagged {
                body: Some(TaggedBody::Record(members)),
                ..
            } => members,
            other => panic!("not a record: {:?}", other),
        }
    }

    fn args(value: &Value) -> &[Argument] {
        match &value.kind {
            ValueKind::Tagged {
                body: Some(TaggedBody::Tuple(args)),
                ..
            } => args,
            other => panic!("not a tuple: {:?}", other),
        }
    }

    #[test]
    fn an_error_shows_its_path_its_consts_and_its_position() {
        let pos = Pos { line: 3, col: 12 };
        let error = Error::at(Pos { line: 1, col: 5 }, "Expected Nat but found String")
            .step(Step::Field("colour".into()))
            .in_const("inner", Pos { line: 1, col: 2 })
            .step(Step::Index(0))
            .in_const("outer", pos)
            .step(Step::Field("shapes".into()));
        assert_eq!(
            error.to_string(),
            "shapes[0].colour: in the value of const outer: in the value of const inner: Expected Nat but found String \
             (line 3, col 12)"
        );
        assert_eq!(error.pos(), Some(pos));
        assert_eq!(error.consts(), ["outer", "inner"]);
        assert_eq!(error.message(), "Expected Nat but found String");
        let written = Error::new("The Float NaN cannot be written").step(Step::Position(1));
        assert_eq!(written.to_string(), "1: The Float NaN cannot be written");
        assert_eq!(
            Error::new("x")
                .step(Step::Position(0))
                .step(Step::Field("a".into()))
                .path(),
            "a.0"
        );
    }

    #[test]
    fn a_text_has_one_value_or_it_is_an_error() {
        assert_eq!(written(&one(" Dot ")), "Dot");
        assert_eq!(
            parse_one("1; 2").unwrap_err().to_string(),
            "Expected one value but found 2 (line 1, col 1)"
        );
        assert_eq!(
            parse_one("Dot(").unwrap_err().pos(),
            parse_values("Dot(").unwrap_err().pos.into()
        );
    }

    #[test]
    fn the_depth_that_is_entered_is_counted_against_the_limit() {
        assert_eq!(enter(0, 2, None), Ok(1));
        assert_eq!(enter(1, 2, None), Ok(2));
        assert_eq!(enter(2, 2, None).unwrap_err().message(), "Nesting is too deep");
        let pos = Pos { line: 2, col: 3 };
        assert_eq!(enter(2, 2, Some(pos)).unwrap_err().pos(), Some(pos));
    }

    /// How deep the value is, counted as `enter` counts it: each value is a level.
    fn height(value: &Value) -> u32 {
        let inner = match &value.kind {
            ValueKind::Array(items) => items.iter().map(height).max(),
            ValueKind::Record(members) => members.iter().map(|m| height(&m.value)).max(),
            ValueKind::Tagged { body, .. } => match body {
                Some(TaggedBody::Record(members)) => members.iter().map(|m| height(&m.value)).max(),
                Some(TaggedBody::Tuple(args)) => args.iter().map(|a| height(&a.value)).max(),
                Some(TaggedBody::Array(items)) => items.iter().map(height).max(),
                None => None,
            },
            _ => None,
        };
        1 + inner.unwrap_or(0)
    }

    #[test]
    fn the_parser_reads_a_value_exactly_as_deep_as_the_limit_of_a_codec() {
        // Every kind of nesting, so that what a codec writes within its limit the parser reads back.
        let wrappers: [(&str, &str); 5] = [("[", "]"), ("{ a: ", " }"), ("A(", ")"), ("A { a: ", " }"), ("A[", "]")];
        for (open, close) in wrappers {
            let nested = |levels: u32| format!("{}1{}", open.repeat(levels as usize), close.repeat(levels as usize));
            let deepest = parse_one(&nested(MAX_DEPTH - 1)).unwrap();
            assert_eq!(height(&deepest), MAX_DEPTH, "{}", open);
            assert!(parse_one(&nested(MAX_DEPTH)).is_err(), "{}", open);
        }
    }

    #[test]
    fn the_fields_are_given_in_the_order_of_the_declaration() {
        let value = one("Box { height: 2.0, width: 1.0 }");
        let [width, height] = fields(members(&value), ["width", "height"], "Box", value.pos).unwrap();
        assert_eq!((written(width), written(height)), ("1.0".into(), "2.0".into()));
        let error = |text: &str| {
            let value = one(text);
            fields(members(&value), ["width", "height"], "Box", value.pos)
                .unwrap_err()
                .to_string()
        };
        assert_eq!(
            error("Box { width: 1.0 }"),
            "Missing field height in Box (line 1, col 1)"
        );
        assert_eq!(
            error("Box { width: 1.0, height: 1.0, depth: 1.0 }"),
            "Unknown field depth in Box (line 1, col 32)"
        );
        assert_eq!(
            error("Box { width: 1.0, width: 1.0 }"),
            "Duplicate field width (line 1, col 19)"
        );
        let empty = one("{}");
        assert!(fields(members(&empty), [], "record", empty.pos).is_ok());
    }

    #[test]
    fn the_arguments_bind_in_order_then_by_name() {
        // Image(String, String, hOffset: Nat, vOffset: Nat, alignment: String), as in the typed examples.
        let params = [None, None, Some("hOffset"), Some("vOffset"), Some("alignment")];
        let bind = |text: &str| {
            let value = one(text);
            arguments(args(&value), params, "Image", value.pos)
                .map(|slots| slots.map(|slot| slot.map(written)))
                .map_err(|e| e.to_string())
        };
        let given = |list: [&str; 5]| -> [Option<String>; 5] { list.map(|s| (!s.is_empty()).then(|| s.to_string())) };
        assert_eq!(
            bind(r#"Image("a", "b", alignment: "left", hOffset: 1, vOffset: 2)"#),
            Ok(given([r#""a""#, r#""b""#, "1", "2", r#""left""#]))
        );
        assert_eq!(
            bind(r#"Image("a", "b", 1)"#),
            Ok(given([r#""a""#, r#""b""#, "1", "", ""]))
        );
        assert_eq!(
            bind(r#"Image("a", "b", 1, 2, "left", hOffset: 3)"#),
            Err("Argument hOffset is given more than once in Image (line 1, col 31)".into())
        );
        assert_eq!(
            bind(r#"Image("a", "b", colour: "red")"#),
            Err("Unknown argument colour in Image (line 1, col 17)".into())
        );
        assert_eq!(
            bind(r#"Image("a")"#),
            Err("Constructor Image takes 5 value(s) but has 1 (line 1, col 1)".into())
        );
        assert_eq!(
            bind(r#"Image("a", "b", 1, 2, "c", 3)"#),
            Err("Constructor Image takes 5 value(s) but has 6 (line 1, col 28)".into())
        );
        let pair = one("Pair(1, 1)");
        let error = arguments(args(&pair), [None], "Pair", pair.pos).unwrap_err();
        assert_eq!(error.message(), "Constructor Pair takes 1 value(s) but has 2");
    }

    #[test]
    fn a_whole_number_is_read_if_it_fits_and_a_sign_makes_it_an_int() {
        assert_eq!(read_nat(&one("18446744073709551615")), Ok(u64::MAX));
        assert_eq!(read_nat(&one("007")), Ok(7));
        assert_eq!(
            read_nat(&one("18446744073709551616")).unwrap_err().message(),
            "The number 18446744073709551616 does not fit Nat"
        );
        assert_eq!(
            read_nat(&one("+1")).unwrap_err().message(),
            "Expected Nat but found Int"
        );
        assert_eq!(
            read_nat(&one("1.0")).unwrap_err().message(),
            "Expected Nat but found Float"
        );
        assert_eq!(
            read_nat(&one("\"1\"")).unwrap_err().message(),
            "Expected Nat but found String"
        );
        assert_eq!(read_int(&one("-9223372036854775808"), true), Ok(i64::MIN));
        assert_eq!(read_int(&one("+9223372036854775807"), true), Ok(i64::MAX));
        assert_eq!(read_int(&one("-0"), true), Ok(0));
        assert_eq!(read_int(&one("5"), true), Ok(5));
        assert_eq!(
            read_int(&one("5"), false).unwrap_err().message(),
            "Expected Int but found Nat"
        );
        assert_eq!(
            read_int(&one("9223372036854775808"), true).unwrap_err().message(),
            "The number 9223372036854775808 does not fit Int"
        );
        assert_eq!(
            read_int(&one("-9223372036854775809"), true).unwrap_err().message(),
            "The number -9223372036854775809 does not fit Int"
        );
    }

    #[test]
    fn a_float_is_the_nearest_f64_and_one_too_large_is_refused() {
        assert_eq!(read_float(&one("0.1"), true), Ok(0.1));
        assert_eq!(read_float(&one("1.5E+2"), true), Ok(150.0));
        assert_eq!(read_float(&one("-2.5e3"), true), Ok(-2500.0));
        assert_eq!(read_float(&one("1.50"), true), Ok(1.5));
        let zero = read_float(&one("-0.0"), true).unwrap();
        assert!(zero == 0.0 && zero.is_sign_positive());
        assert_eq!(read_float(&one("1.0e-400"), true), Ok(0.0));
        assert_eq!(read_float(&one("4.9e-324"), true), Ok(f64::from_bits(1)));
        assert_eq!(
            read_float(&one("1.0e400"), true).unwrap_err().message(),
            "The number 1.0e400 does not fit Float"
        );
        let huge = "9".repeat(400);
        assert_eq!(
            read_float(&one(&huge), true).unwrap_err().message(),
            format!("The number {} does not fit Float", huge)
        );
        assert_eq!(read_float(&one("3"), true), Ok(3.0));
        assert_eq!(read_float(&one("-3"), true), Ok(-3.0));
        assert_eq!(
            read_float(&one("3"), false).unwrap_err().message(),
            "Expected Float but found Nat"
        );
        assert_eq!(
            read_float(&one("-3"), false).unwrap_err().message(),
            "Expected Float but found Int"
        );
    }

    #[test]
    fn a_number_is_written_as_convert_writes_it() {
        assert_eq!(written(&nat(u64::MAX)), "18446744073709551615");
        assert_eq!(written(&int(0)), "+0");
        assert_eq!(written(&int(5)), "+5");
        assert_eq!(written(&int(i64::MIN)), "-9223372036854775808");
    }

    #[test]
    fn a_float_is_written_as_the_shortest_text_that_reads_back() {
        let cases: [(f64, &str); 10] = [
            (0.1, "0.1"),
            (1.5, "1.5"),
            (100.0, "100.0"),
            (-2.5, "-2.5"),
            (0.0, "0.0"),
            (-0.0, "0.0"),
            (1e300, "1.0e300"),
            (1.5e300, "1.5e300"),
            (1e-7, "1.0e-7"),
            (f64::MAX, "1.7976931348623157e308"),
        ];
        for (number, text) in cases {
            let value = float(number).unwrap();
            assert_eq!(written(&value), text);
            // What is written is a Float that the parser reads, and it reads back as the same number.
            let back = read_float(&one(text), false).unwrap();
            assert_eq!(
                back.to_bits(),
                if number == 0.0 { 0 } else { number.to_bits() },
                "{}",
                text
            );
        }
        for number in [
            f64::MIN_POSITIVE,
            f64::from_bits(1),
            1e16,
            123456789.125,
            std::f64::consts::PI,
        ] {
            let text = written(&float(number).unwrap());
            assert_eq!(read_float(&one(&text), false), Ok(number), "{}", text);
        }
        for number in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let error = float(number).unwrap_err();
            assert_eq!(error.pos(), None);
            assert!(error.message().starts_with("The Float "), "{}", error);
        }
    }

    #[test]
    fn the_builders_make_the_tree_that_the_parser_makes() {
        let value = tagged_tuple(
            "Image",
            vec![
                (None, string("a\"b")),
                (Some("hOffset"), nat(1)),
                (
                    Some("tags"),
                    array(vec![unit("None"), tagged_array("Items", vec![int(-1)])]),
                ),
                (
                    Some("at"),
                    tagged_record("Point", vec![("x", float(1.0).unwrap()), ("y", record(vec![]))]),
                ),
            ],
        );
        let text = written(&value);
        assert_eq!(
            text,
            r#"Image("a\"b", hOffset: 1, tags: [None, Items[-1]], at: Point { x: 1.0, y: {} })"#
        );
        assert_eq!(written(&one(&text)), text);
    }
}
