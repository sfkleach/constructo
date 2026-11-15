//! Evaluates a query over a lazy stream of values.
//!
//! What the query itself fixes has been checked by `check` before this runs. Here every value is checked as it
//! arrives, by its kind, and a mismatch is an error at that value. See the settled questions of
//! `docs/tasks/2026-11-15-query-subcommand/query-subcommand.md`.

use super::check::Fixed;
use super::number::{self, kind_of};
use super::parser::QUERY_NAME;
use super::syntax::*;
use super::types::{known, Types};
use crate::ast::{Argument, Member, TaggedBody, Value, ValueKind};
use crate::diagnostics::{Diagnostic, Pos};
use crate::typecheck::{bind_arguments, Ty};
use std::cell::RefCell;
use std::rc::Rc;

/// A value with where it came from: the name of its source and, through its position, where it is in that source.
#[derive(Debug, Clone)]
pub struct Item {
    pub value: Value,
    pub src: Rc<str>,
    /// Whether the value is a literal of the query, or made only from literals, which adapts to the kind of number it
    /// meets, as `1` does in `m.count + 1`.
    pub lit: bool,
    /// The type of the value, when the query is typed and the type is known.
    pub ty: Option<Ty>,
    /// The index of the source that the value is in, which says how its names are shown. A value that the query makes
    /// is shown as if it were in the source of the query's names.
    pub file: usize,
}

/// What a typed query knows of the types, while it runs.
pub struct Typing {
    pub types: Types,
    pub names_source: usize,
}

thread_local! {
    static TYPING: RefCell<Option<Rc<Typing>>> = const { RefCell::new(None) };
}

/// Runs `f` with the types that a typed query knows, or with none for an untyped one.
pub fn with_typing<T>(typing: Option<Rc<Typing>>, f: impl FnOnce() -> T) -> T {
    let saved = TYPING.with(|t| std::mem::replace(&mut *t.borrow_mut(), typing));
    let result = f();
    TYPING.with(|t| *t.borrow_mut() = saved);
    result
}

fn typing() -> Option<Rc<Typing>> {
    TYPING.with(|t| t.borrow().clone())
}

/// Why a query stopped.
#[derive(Debug)]
pub enum Failure {
    /// An error in the inputs or at a value: exit code 1.
    Error(Diagnostic),
    /// A problem with how the inputs are found, such as a file that cannot be read: exit code 2. The message has been
    /// made, with no `ERROR:` in front.
    Input(String),
}

pub type Stream = Box<dyn Iterator<Item = Result<Item, Failure>>>;

/// The result of an expression: one value, or a lazy stream of them.
pub enum Val {
    One(Item),
    Many(Stream),
}

/// The names that patterns have bound, innermost first.
#[derive(Clone, Default)]
pub struct Env(Option<Rc<Binding>>);

struct Binding {
    name: String,
    item: Item,
    next: Env,
}

impl Env {
    fn with(&self, name: &str, item: Item) -> Env {
        Env(Some(Rc::new(Binding {
            name: name.to_string(),
            item,
            next: self.clone(),
        })))
    }

    fn get(&self, name: &str) -> Option<&Item> {
        let mut at = &self.0;
        while let Some(binding) = at {
            if binding.name == name {
                return Some(&binding.item);
            }
            at = &binding.next.0;
        }
        None
    }
}

type R<T> = Result<T, Failure>;

fn at_item<T>(item: &Item, message: impl Into<String>) -> R<T> {
    Err(Failure::Error(Diagnostic::new(&item.src, item.value.pos, message)))
}

fn at_query<T>(pos: Pos, message: impl Into<String>) -> R<T> {
    Err(Failure::Error(Diagnostic::new(QUERY_NAME, pos, message)))
}

fn query_src() -> Rc<str> {
    Rc::from(QUERY_NAME)
}

/// Makes an item of a value that the query built.
fn made(kind: ValueKind, pos: Pos, src: &Rc<str>, lit: bool) -> Item {
    Item {
        value: Value { pos, kind },
        src: src.clone(),
        lit,
        ty: None,
        file: typing().map_or(0, |t| t.names_source),
    }
}

/// A part of an item, which comes from the same source.
fn part(of: &Item, value: &Value) -> Item {
    typed_part(of, value, None)
}

/// A part of an item, with its type if that is known.
fn typed_part(of: &Item, value: &Value, ty: Option<Ty>) -> Item {
    Item {
        value: value.clone(),
        src: of.src.clone(),
        lit: of.lit,
        ty,
        file: of.file,
    }
}

fn tagged(name: &str, body: Option<TaggedBody>) -> ValueKind {
    ValueKind::Tagged {
        name: name.to_string(),
        body,
    }
}

fn boolean(b: bool, pos: Pos) -> Item {
    made(tagged(if b { "true" } else { "false" }, None), pos, &query_src(), false)
}

fn some(item: Item) -> Item {
    let pos = item.value.pos;
    let src = item.src.clone();
    let file = item.file;
    // The Option keeps the type of what it holds, so that it is known again when it is taken out.
    let ty = item
        .ty
        .clone()
        .map(|t| Ty::Enum("Option".to_string(), Rc::new(vec![t])));
    let arg = Argument {
        name: None,
        pos,
        value: item.value,
    };
    let mut result = made(tagged("Some", Some(TaggedBody::Tuple(vec![arg]))), pos, &src, false);
    result.file = file;
    result.ty = ty;
    result
}

fn none(pos: Pos) -> Item {
    made(tagged("None", None), pos, &query_src(), false)
}

/// The name of a constructor or const of the prelude, with `true` and `True` made one, and `null` and `None`, since
/// the prelude declares each const as the other.
pub fn canonical(name: &str) -> &str {
    match name.strip_suffix("@constructo.prelude").unwrap_or(name) {
        "True" => "true",
        "False" => "false",
        "null" => "None",
        other => other,
    }
}

/// A name as a message shows it, without the project that a typed query resolved it to.
fn shown(name: &str) -> &str {
    name.split('@').next().unwrap_or(name)
}

/// Describes the kind of a value for a message.
pub fn describe(value: &Value) -> String {
    match &value.kind {
        ValueKind::Nat(_) => "a Nat".to_string(),
        ValueKind::Int(_) => "an Int".to_string(),
        ValueKind::Float(_) => "a Float".to_string(),
        ValueKind::Str(_) => "a String".to_string(),
        ValueKind::Array(_) => "an array".to_string(),
        ValueKind::Record(_) => "a record".to_string(),
        ValueKind::Tagged { name, .. } if matches!(canonical(name), "true" | "false") => "a Bool".to_string(),
        ValueKind::Tagged { name, .. } if crate::convert::is_const_name(name) => format!("the const {}", shown(name)),
        ValueKind::Tagged { name, .. } => format!("the constructor {}", shown(name)),
    }
}

/// Describes an item, with the kind of a number that its type gives it.
fn describe_item(item: &Item) -> String {
    match num(item) {
        Some((NumKind::Int, _)) => "an Int".to_string(),
        Some((kind, _)) => format!("a {}", kind.name()),
        None => describe(&item.value),
    }
}

fn as_bool(item: &Item) -> R<bool> {
    let item = &deref(item)?;
    if let ValueKind::Tagged { name, body: None } = &item.value.kind {
        match canonical(name) {
            "true" => return Ok(true),
            "false" => return Ok(false),
            _ => {}
        }
    }
    at_item(item, format!("Expected a Bool but found {}", describe(&item.value)))
}

fn number(item: &Item) -> Option<&str> {
    num(item).map(|(_, text)| text)
}

/// The kind and text of a number. The kind is the one that the type gives it, if that is known, so that a `5` in a
/// `Float` field is a `Float`, and otherwise the one that its text gives it.
fn num(item: &Item) -> Option<(NumKind, &str)> {
    let text = match &item.value.kind {
        ValueKind::Nat(t) | ValueKind::Int(t) | ValueKind::Float(t) => t.as_str(),
        _ => return None,
    };
    let kind = match &item.ty {
        Some(Ty::Nat) => NumKind::Nat,
        Some(Ty::Int) => NumKind::Int,
        Some(Ty::Float) => NumKind::Float,
        _ => kind_of(text),
    };
    Some((kind, text))
}

fn num_value(text: String, pos: Pos, src: &Rc<str>, lit: bool) -> Item {
    let kind = match kind_of(&text) {
        NumKind::Nat => ValueKind::Nat(text),
        NumKind::Int => ValueKind::Int(text),
        NumKind::Float => ValueKind::Float(text),
    };
    made(kind, pos, src, lit)
}

/// Whether a literal of one kind can stand for a number of another, as a literal adapts in Rust.
fn widens(from: NumKind, to: NumKind) -> bool {
    matches!(
        (from, to),
        (NumKind::Nat, NumKind::Int) | (NumKind::Nat, NumKind::Float) | (NumKind::Int, NumKind::Float)
    )
}

/// Brings two numbers to one kind, adapting a literal to the other's kind. Two values of different kinds are an error
/// at the value that is not a literal, which suggests the cast.
fn unify(a: &Item, b: &Item, verb: &str) -> R<(NumKind, String, String)> {
    let ((ka, ta), (kb, tb)) = (num(a).expect("checked"), num(b).expect("checked"));
    if ka == kb {
        return Ok((ka, ta.to_string(), tb.to_string()));
    }
    if a.lit && widens(ka, kb) {
        let ta = number::cast(ta, kb).map_err(|m| Failure::Error(Diagnostic::new(&a.src, a.value.pos, m)))?;
        return Ok((kb, ta, tb.to_string()));
    }
    if b.lit && widens(kb, ka) {
        let tb = number::cast(tb, ka).map_err(|m| Failure::Error(Diagnostic::new(&b.src, b.value.pos, m)))?;
        return Ok((ka, ta.to_string(), tb));
    }
    let (blamed, mine, other) = if a.lit && !b.lit { (b, kb, ka) } else { (a, ka, kb) };
    let text = number(blamed).expect("checked");
    at_item(
        blamed,
        format!(
            "{} is a {} and is {} a {}; cast it with `as {}`",
            text,
            mine.name(),
            verb,
            other.name(),
            other.name()
        ),
    )
}

fn num_error(item: &Item) -> impl Fn(String) -> Failure + '_ {
    move |m| Failure::Error(Diagnostic::new(&item.src, item.value.pos, m))
}

/// Evaluates an expression. `values` holds the stream of the inputs until `#values` takes it.
pub fn eval(expr: &Expr, env: &Env, values: &mut Option<Stream>) -> R<Val> {
    let pos = expr.pos;
    let one = |item: Item| Ok(Val::One(item));
    match &expr.kind {
        ExprKind::Values => match values.take() {
            Some(stream) => Ok(Val::Many(stream)),
            None => at_query(pos, "#values can only be used once"),
        },
        ExprKind::Var(name) => match env.get(name) {
            Some(item) => one(item.clone()),
            None if matches!(name.as_str(), "true" | "false" | "null") => {
                one(made(tagged(name, None), pos, &query_src(), false))
            }
            None => at_query(pos, format!("{} is not bound", name)),
        },
        // A const of the query is followed as a const in the data is, to its value and its type.
        ExprKind::Const(name) => one(deref(&made(tagged(name, None), pos, &query_src(), false))?),
        ExprKind::Lit(lit) => {
            let kind = match lit {
                Lit::Nat(t) => ValueKind::Nat(t.clone()),
                Lit::Int(t) => ValueKind::Int(t.clone()),
                Lit::Float(t) => ValueKind::Float(t.clone()),
                Lit::Str(t) => ValueKind::Str(t.clone()),
            };
            one(made(kind, pos, &query_src(), true))
        }
        ExprKind::Ctor { name, body } => {
            let body = match body {
                None => None,
                Some(CtorBody::Record { fields, base }) => {
                    let members = record(fields, base.as_deref(), Some(name), env, values)?;
                    Some(TaggedBody::Record(members))
                }
                Some(CtorBody::Tuple(args)) => {
                    let mut out = Vec::new();
                    for a in args {
                        out.push(Argument {
                            name: a.name.clone(),
                            pos: a.pos,
                            value: item(&a.value, env, values)?.value,
                        });
                    }
                    Some(TaggedBody::Tuple(out))
                }
                Some(CtorBody::Array(items)) => Some(TaggedBody::Array(array(items, env, values)?)),
            };
            one(made(tagged(name, body), pos, &query_src(), false))
        }
        ExprKind::Record { fields, base } => {
            let members = record(fields, base.as_deref(), None, env, values)?;
            one(made(ValueKind::Record(members), pos, &query_src(), false))
        }
        ExprKind::Array(items) => {
            let items = array(items, env, values)?;
            one(made(ValueKind::Array(items), pos, &query_src(), false))
        }
        ExprKind::Field(of, name) => {
            let of = item(of, env, values)?;
            one(field(&of, name)?)
        }
        ExprKind::Position(of, n) => {
            let of = item(of, env, values)?;
            one(position(&of, *n)?)
        }
        ExprKind::Index(of, index) => {
            let of = item(of, env, values)?;
            let index = item(index, env, values)?;
            one(element(&of, &index)?)
        }
        ExprKind::Method { receiver, name, args } => {
            let receiver = eval(receiver, env, values)?;
            match receiver {
                Val::Many(stream) => stream_method(stream, name, args, pos, env, values),
                Val::One(item) => value_method(item, name, args, pos, env, values),
            }
        }
        ExprKind::Closure(_) => at_query(pos, "A closure can only be the argument of a method"),
        ExprKind::Unary(op, operand) => {
            let operand = deref(&item(operand, env, values)?)?;
            match op {
                UnOp::Not => one(boolean(!as_bool(&operand)?, pos)),
                UnOp::Neg => {
                    let Some((kind, text)) = num(&operand) else {
                        return at_item(
                            &operand,
                            format!("Expected a number but found {}", describe(&operand.value)),
                        );
                    };
                    // A literal Nat that is negated is an Int literal, as `- 5` is `-5`.
                    let text = if operand.lit && kind == NumKind::Nat {
                        format!("-{}", text)
                    } else {
                        number::negate(text, kind).map_err(num_error(&operand))?
                    };
                    one(num_value(text, operand.value.pos, &operand.src, operand.lit))
                }
            }
        }
        ExprKind::Binary(op, left, right) => binary(*op, left, right, pos, env, values),
        ExprKind::Cast(operand, to) => {
            let operand = deref(&item(operand, env, values)?)?;
            let Some((kind, text)) = num(&operand) else {
                return at_item(
                    &operand,
                    format!("Only a number can be cast, and this is {}", describe(&operand.value)),
                );
            };
            // A cast to the same kind does nothing, so the number keeps its text and its type.
            if kind == *to {
                return one(Item { lit: false, ..operand });
            }
            let text = number::cast(text, *to).map_err(num_error(&operand))?;
            one(num_value(text, operand.value.pos, &operand.src, false))
        }
        ExprKind::If { branches, otherwise } => {
            for (cond, then) in branches {
                if as_bool(&item(cond, env, values)?)? {
                    return branch(then, expr, env, values);
                }
            }
            branch(otherwise, expr, env, values)
        }
        ExprKind::Match { scrutinee, arms } => {
            let subject = item(scrutinee, env, values)?;
            for arm in arms {
                let Some(inner) = bind(&arm.pattern, &subject, env)? else {
                    continue;
                };
                if let Some(guard) = &arm.guard {
                    if !as_bool(&item(guard, &inner, &mut None)?)? {
                        continue;
                    }
                }
                return branch(&arm.body, expr, &inner, values);
            }
            at_item(&subject, "No arm of the match matches this value")
        }
        ExprKind::Matches(subject, pattern) => {
            let subject = item(subject, env, values)?;
            one(boolean(bind(pattern, &subject, env)?.is_some(), pos))
        }
        ExprKind::Assign { .. } => at_query(pos, "An assignment is only allowed in the closure of update"),
    }
}

/// Evaluates the branch of an `if` or a `match`, and checks that what it gives agrees with what the other branches
/// fix, which the check before any data was read could not see.
fn branch(chosen: &Expr, whole: &Expr, env: &Env, values: &mut Option<Stream>) -> R<Val> {
    let result = eval(chosen, env, values)?;
    let Val::One(found) = result else { return Ok(result) };
    let others: Vec<&Expr> = match &whole.kind {
        ExprKind::If { branches, otherwise } => branches
            .iter()
            .map(|(_, b)| b)
            .chain(std::iter::once(&**otherwise))
            .collect(),
        ExprKind::Match { arms, .. } => arms.iter().map(|a| &a.body).collect(),
        _ => Vec::new(),
    };
    let mut common = fixed_of(&found);
    for other in others {
        if let Some(fixed) = super::check::fixed_kind(other) {
            match merge(common, fixed) {
                Some(merged) => common = merged,
                None => {
                    return at_item(
                        &found,
                        format!(
                            "This branch gives {}, but another gives {}, and they have no common type",
                            describe_item(&found),
                            fixed.describe()
                        ),
                    )
                }
            }
        }
    }
    // A literal takes the kind that the branches have in common, as the check found it.
    Ok(Val::One(adapt(found, common)?))
}

/// Evaluates an expression that must give one value.
fn item(expr: &Expr, env: &Env, values: &mut Option<Stream>) -> R<Item> {
    match eval(expr, env, values)? {
        Val::One(item) => Ok(item),
        Val::Many(_) => at_query(
            expr.pos,
            "This is an iterator, which cannot be a value here; use .collect() to make an array of it",
        ),
    }
}

fn array(items: &[Expr], env: &Env, values: &mut Option<Stream>) -> R<Vec<Value>> {
    let mut out: Vec<Item> = Vec::new();
    for e in items {
        out.push(item(e, env, values)?);
    }
    let common = common_kind(&out)?;
    let mut adapted = Vec::new();
    for item in out {
        adapted.push(match common {
            Some(common) => adapt(item, common)?.value,
            None => item.value,
        });
    }
    Ok(adapted)
}

/// Checks that the elements of an array have a common type, as far as their kinds can tell: numbers of one kind (a
/// literal adapting), or all strings, or all records, and so on. Constructors are not told apart without the types.
fn common_kind(items: &[Item]) -> R<Option<Fixed>> {
    let mut common: Option<Fixed> = None;
    for item in items {
        let fixed = fixed_of(item);
        common = Some(match common {
            None => fixed,
            Some(so_far) => match merge(so_far, fixed) {
                Some(merged) => merged,
                None => {
                    return at_item(
                        item,
                        format!(
                            "An array's elements must have a common type, but this is {} and another is {}",
                            describe_item(item),
                            so_far.describe()
                        ),
                    )
                }
            },
        });
    }
    Ok(common)
}

/// What an item fixes about its type, with the kind of a number that its type gives it.
fn fixed_of(item: &Item) -> Fixed {
    match num(item) {
        Some((kind, _)) => Fixed::Num(kind, item.lit),
        None => Fixed::of(&item.value),
    }
}

/// The kind that two have in common, if they have one: a literal number gives way to the kind that it meets, and two
/// literals keep the wider of their kinds.
fn merge(a: Fixed, b: Fixed) -> Option<Fixed> {
    match (a, b) {
        (Fixed::Num(x, lx), Fixed::Num(y, ly)) => {
            if x == y {
                Some(Fixed::Num(x, lx && ly))
            } else if lx && widens(x, y) {
                Some(Fixed::Num(y, ly))
            } else if ly && widens(y, x) {
                Some(Fixed::Num(x, lx))
            } else {
                None
            }
        }
        (a, b) if a == b => Some(a),
        _ => None,
    }
}

/// A literal number converted to the kind that it has in common with others. Anything else is unchanged.
fn adapt(item: Item, common: Fixed) -> R<Item> {
    let Fixed::Num(to, _) = common else { return Ok(item) };
    match num(&item) {
        Some((kind, text)) if item.lit && kind != to => {
            let text = number::cast(text, to).map_err(num_error(&item))?;
            Ok(num_value(text, item.value.pos, &item.src, true))
        }
        _ => Ok(item),
    }
}

/// The members of a record: the base's, if there is one, with the fields that are given replacing them or added.
fn record(
    fields: &[FieldInit],
    base: Option<&Expr>,
    ctor: Option<&String>,
    env: &Env,
    values: &mut Option<Stream>,
) -> R<Vec<Member>> {
    let mut members: Vec<Member> = match base {
        None => Vec::new(),
        Some(base_expr) => {
            let base = item(base_expr, env, values)?;
            match (&base.value.kind, ctor) {
                (ValueKind::Record(ms), None) => ms.clone(),
                (
                    ValueKind::Tagged {
                        name,
                        body: Some(TaggedBody::Record(ms)),
                    },
                    Some(ctor),
                ) if name == ctor => ms.clone(),
                (_, Some(ctor)) => {
                    return at_item(
                        &base,
                        format!(
                            "The base of {} {{ .. }} must be a {} with a record, but this is {}",
                            ctor,
                            ctor,
                            describe(&base.value)
                        ),
                    )
                }
                (_, None) => {
                    return at_item(
                        &base,
                        format!(
                            "The base of {{ .. }} must be a record, but this is {}",
                            describe(&base.value)
                        ),
                    )
                }
            }
        }
    };
    for f in fields {
        let value = item(&f.value, env, values)?.value;
        match members.iter_mut().find(|m| m.name == f.name) {
            Some(m) => m.value = value,
            None => members.push(Member {
                name: f.name.clone(),
                pos: f.pos,
                value,
            }),
        }
    }
    Ok(members)
}

/// Whether a value is a reference to a const other than those of the prelude, which cannot be seen into without the
/// types.
fn deref(item: &Item) -> R<Item> {
    let mut at = item.clone();
    // A const can refer to another, but not to itself, so the chain ends; the limit is a defensive check.
    for _ in 0..crate::limits::MAX_TYPED_NESTING {
        let ValueKind::Tagged { name, body: None } = &at.value.kind else {
            return Ok(at);
        };
        if !crate::convert::is_const_name(name) || matches!(canonical(name), "true" | "false" | "None") {
            return Ok(at);
        }
        let Some(typing) = typing() else {
            return at_item(
                &at,
                format!(
                    "{} is a const, which cannot be seen into without the types; supply the types",
                    shown(name)
                ),
            );
        };
        let Some((value, ty)) = typing.types.constant(name) else {
            return at_item(&at, format!("{} is not a const that the types declare", shown(name)));
        };
        at = Item {
            value: value.clone(),
            src: at.src.clone(),
            lit: false,
            ty: ty.clone(),
            file: at.file,
        };
    }
    at_item(item, "The chain of consts is too long to follow")
}

/// Which parameter of a tuple is wanted: by position, or by name.
#[derive(Clone, Copy)]
enum Param<'a> {
    At(usize),
    Named(&'a str),
}

/// An argument of a tuple constructor. Untyped, only what the text shows can be found: a positional argument by its
/// position, and a named one by its name. Typed, the parameter is found however it was written, and the default is
/// given for one that was left out.
fn argument(of: &Item, ctor: &str, args: &[Argument], param: Param) -> R<Item> {
    if let Some(typing) = typing() {
        if let Some((params, tys)) = typing.types.tuple_params(ctor, of.ty.as_ref()) {
            let index = match param {
                Param::At(n) if n < params.len() => n,
                Param::At(n) => return at_item(of, format!("{} has no parameter {}", shown(ctor), n)),
                Param::Named(name) => match params.iter().position(|p| p.name.as_deref() == Some(name)) {
                    Some(index) => index,
                    None => return at_item(of, format!("{} has no parameter {}", shown(ctor), name)),
                },
            };
            let (targets, _) = bind_arguments(of.value.pos, ctor, &params, args).or_else(|e| at_item(of, e.message))?;
            let ty = Some(tys[index].clone()).filter(known);
            return match targets.iter().position(|t| *t == index) {
                Some(at) => Ok(typed_part(of, &args[at].value, ty)),
                None => {
                    let default = typing
                        .types
                        .default_value(&tys[index], of.value.pos)
                        .or_else(|m| at_item(of, m))?;
                    Ok(typed_part(of, &default, ty))
                }
            };
        }
    }
    let positional: Vec<&Argument> = args.iter().filter(|a| a.name.is_none()).collect();
    match param {
        Param::At(n) => match positional.get(n) {
            Some(a) => Ok(part(of, &a.value)),
            None if args.iter().any(|a| a.name.is_some()) => at_item(
                of,
                format!(
                    "This {} has {} positional argument(s), and without the types it cannot be known which \
                     parameter is {}; supply the types",
                    shown(ctor),
                    positional.len(),
                    n
                ),
            ),
            None => at_item(of, format!("This {} has no argument {}", shown(ctor), n)),
        },
        Param::Named(name) => match args.iter().find(|a| a.name.as_deref() == Some(name)) {
            Some(a) => Ok(part(of, &a.value)),
            None => at_item(
                of,
                format!(
                    "This {} does not give {} by name, and without the types it cannot be known whether it is \
                     given by position or left to its default; supply the types",
                    shown(ctor),
                    name
                ),
            ),
        },
    }
}

/// The type of a field of a record, if the types give it.
fn field_type(of: &Item, name: &str) -> Option<Ty> {
    let typing = typing()?;
    let fields = typing.types.record_fields(&of.value, of.ty.as_ref())?;
    fields
        .into_iter()
        .find(|(n, _)| n == name)
        .map(|(_, ty)| ty)
        .filter(known)
}

/// The type of the elements of an array, if the types give it.
fn element_type(of: &Item) -> Option<Ty> {
    typing()?.types.element_type(&of.value, of.ty.as_ref()).filter(known)
}

fn field(of: &Item, name: &str) -> R<Item> {
    let of = &deref(of)?;
    let members = match &of.value.kind {
        ValueKind::Record(ms) => ms,
        ValueKind::Tagged {
            body: Some(TaggedBody::Record(ms)),
            ..
        } => ms,
        ValueKind::Tagged {
            name: ctor,
            body: Some(TaggedBody::Tuple(args)),
        } => return argument(of, ctor, args, Param::Named(name)),
        ValueKind::Tagged { name: ctor, body: None } => {
            return at_item(of, format!("{} has no body, so it has no field {}", shown(ctor), name))
        }
        _ => {
            return at_item(
                of,
                format!(
                    "{} has no fields, so it has no field {}",
                    capitalised(&describe_item(of)),
                    name
                ),
            )
        }
    };
    match members.iter().find(|m| m.name == name) {
        Some(m) => Ok(typed_part(of, &m.value, field_type(of, name))),
        None => at_item(of, format!("This record has no field {}", name)),
    }
}

fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn position(of: &Item, n: usize) -> R<Item> {
    let of = &deref(of)?;
    let ValueKind::Tagged { name, body } = &of.value.kind else {
        return at_item(
            of,
            format!(
                "{} has no arguments, so it has no argument {}",
                capitalised(&describe_item(of)),
                n
            ),
        );
    };
    let Some(TaggedBody::Tuple(args)) = body else {
        return at_item(of, format!("{} has no tuple, so it has no argument {}", shown(name), n));
    };
    argument(of, name, args, Param::At(n))
}

/// The elements of an array, bare or as a constructor's body.
fn elements(of: &Item) -> Option<&Vec<Value>> {
    match &of.value.kind {
        ValueKind::Array(items) => Some(items),
        ValueKind::Tagged {
            body: Some(TaggedBody::Array(items)),
            ..
        } => Some(items),
        _ => None,
    }
}

fn index_of(index: &Item) -> R<usize> {
    match &index.value.kind {
        ValueKind::Nat(t) => t
            .parse::<usize>()
            .or_else(|_| at_item(index, format!("{} is too large to be an index", t))),
        _ => at_item(
            index,
            format!("An index must be a Nat, and this is {}", describe(&index.value)),
        ),
    }
}

fn element(of: &Item, index: &Item) -> R<Item> {
    let of = &deref(of)?;
    let Some(items) = elements(of) else {
        return at_item(
            of,
            format!(
                "{} is not an array, so it cannot be indexed",
                capitalised(&describe_item(of))
            ),
        );
    };
    let i = index_of(index)?;
    match items.get(i) {
        Some(v) => Ok(typed_part(of, v, element_type(of))),
        None => at_item(
            of,
            format!("The index {} is out of range for an array of {}", i, items.len()),
        ),
    }
}

fn closure_arg<'a>(args: &'a [Expr], at: usize, name: &str, pos: Pos) -> R<&'a Rc<Closure>> {
    match args.get(at).map(|a| &a.kind) {
        Some(ExprKind::Closure(c)) => Ok(c),
        _ => at_query(pos, format!("{} takes a closure", name)),
    }
}

/// Binds an item to a closure's parameter and evaluates the body. `None` if the parameter does not match, which only
/// a refutable pattern can do.
fn apply(closure: &Closure, env: &Env, arg: &Item) -> R<Option<Val>> {
    let param = &closure.params[0];
    match bind(param, arg, env)? {
        None => Ok(None),
        Some(inner) => Ok(Some(eval(&closure.body, &inner, &mut None)?)),
    }
}

/// As `apply`, for a parameter that must always match and a body that must give one value.
fn apply_one(closure: &Closure, env: &Env, arg: &Item) -> R<Item> {
    match apply(closure, env, arg)? {
        Some(Val::One(item)) => Ok(item),
        Some(Val::Many(_)) => at_query(
            closure.body.pos,
            "This is an iterator, which cannot be a value here; use .collect() to make an array of it, or flat_map",
        ),
        None => at_item(arg, "This value does not match the closure's parameter"),
    }
}

fn option(item: &Item) -> R<Option<Item>> {
    let item = &deref(item)?;
    if let ValueKind::Tagged { name, body } = &item.value.kind {
        match (canonical(name), body) {
            ("None", None) => return Ok(None),
            ("Some", Some(TaggedBody::Tuple(args))) if args.len() == 1 && args[0].name.is_none() => {
                // The type of what is in an Option is the argument of its type, when that is known.
                let inner = match &item.ty {
                    Some(Ty::Enum(_, args)) if args.len() == 1 => Some(args[0].clone()).filter(known),
                    _ => None,
                };
                return Ok(Some(typed_part(item, &args[0].value, inner)));
            }
            _ => {}
        }
    }
    at_item(
        item,
        format!(
            "Expected an Option, Some(..) or None, but found {}",
            describe(&item.value)
        ),
    )
}

/// The stream of an iterator, or of an array's elements, as `flat_map` accepts either.
fn into_stream(val: Val) -> R<Stream> {
    match val {
        Val::Many(stream) => Ok(stream),
        Val::One(of) => match elements(&of) {
            Some(items) => {
                let items: Vec<Item> = items.iter().map(|v| typed_part(&of, v, element_type(&of))).collect();
                Ok(Box::new(items.into_iter().map(Ok)))
            }
            None => at_item(
                &of,
                format!("Expected an iterator or an array, but found {}", describe(&of.value)),
            ),
        },
    }
}

fn nat_arg(args: &[Expr], name: &str, pos: Pos, env: &Env, values: &mut Option<Stream>) -> R<usize> {
    let Some(arg) = args.first() else {
        return at_query(pos, format!("{} takes a Nat", name));
    };
    index_of(&item(arg, env, values)?)
}

fn stream_method(
    mut stream: Stream,
    name: &str,
    args: &[Expr],
    pos: Pos,
    env: &Env,
    values: &mut Option<Stream>,
) -> R<Val> {
    let env = env.clone();
    let many = |s: Stream| Ok(Val::Many(s));
    match name {
        "map" => {
            let c = closure_arg(args, 0, name, pos)?.clone();
            many(Box::new(stream.map(move |r| r.and_then(|i| apply_one(&c, &env, &i)))))
        }
        "filter" => {
            let c = closure_arg(args, 0, name, pos)?.clone();
            many(Box::new(stream.filter_map(move |r| {
                match r.and_then(|i| {
                    let keep = as_bool(&apply_one(&c, &env, &i)?)?;
                    Ok(keep.then_some(i))
                }) {
                    Ok(Some(i)) => Some(Ok(i)),
                    Ok(None) => None,
                    Err(e) => Some(Err(e)),
                }
            })))
        }
        "filter_map" => {
            let c = closure_arg(args, 0, name, pos)?.clone();
            many(Box::new(stream.filter_map(
                move |r| match r.and_then(|i| option(&apply_one(&c, &env, &i)?)) {
                    Ok(o) => o.map(Ok),
                    Err(e) => Some(Err(e)),
                },
            )))
        }
        "filter_match" => {
            let c = closure_arg(args, 0, name, pos)?.clone();
            many(Box::new(stream.filter_map(move |r| {
                let result = r.and_then(|i| match apply(&c, &env, &i)? {
                    None => Ok(None),
                    Some(Val::One(item)) => Ok(Some(item)),
                    Some(Val::Many(_)) => at_query(c.body.pos, "This is an iterator, which cannot be a value here"),
                });
                match result {
                    Ok(o) => o.map(Ok),
                    Err(e) => Some(Err(e)),
                }
            })))
        }
        "flat_map" => {
            let c = closure_arg(args, 0, name, pos)?.clone();
            let mut current: Option<Stream> = None;
            let mut failed = false;
            many(Box::new(std::iter::from_fn(move || loop {
                if failed {
                    return None;
                }
                if let Some(inner) = current.as_mut() {
                    match inner.next() {
                        Some(r) => return Some(r),
                        None => current = None,
                    }
                }
                let next = match stream.next()? {
                    Ok(i) => i,
                    Err(e) => {
                        failed = true;
                        return Some(Err(e));
                    }
                };
                let result = match apply(&c, &env, &next) {
                    Ok(Some(val)) => into_stream(val),
                    Ok(None) => at_item(&next, "This value does not match the closure's parameter"),
                    Err(e) => Err(e),
                };
                match result {
                    Ok(s) => current = Some(s),
                    Err(e) => {
                        failed = true;
                        return Some(Err(e));
                    }
                }
            })))
        }
        "take" => {
            let n = nat_arg(args, name, pos, &env, values)?;
            many(Box::new(stream.take(n)))
        }
        "skip" => {
            let n = nat_arg(args, name, pos, &env, values)?;
            many(Box::new(Skip { stream, left: n }))
        }
        "next" => match stream.next() {
            Some(r) => Ok(Val::One(some(r?))),
            None => Ok(Val::One(none(pos))),
        },
        "nth" => {
            let n = nat_arg(args, name, pos, &env, values)?;
            for _ in 0..n {
                if stream.next().transpose()?.is_none() {
                    return Ok(Val::One(none(pos)));
                }
            }
            match stream.next() {
                Some(r) => Ok(Val::One(some(r?))),
                None => Ok(Val::One(none(pos))),
            }
        }
        "last" => {
            let mut last = None;
            for r in stream {
                last = Some(r?);
            }
            Ok(Val::One(last.map(some).unwrap_or_else(|| none(pos))))
        }
        "count" => {
            let mut n: u64 = 0;
            for r in stream {
                r?;
                n += 1;
            }
            Ok(Val::One(made(ValueKind::Nat(n.to_string()), pos, &query_src(), false)))
        }
        "collect" => {
            let mut items = Vec::new();
            for r in stream {
                items.push(r?);
            }
            let common = common_kind(&items)?;
            let src = items.first().map(|i| i.src.clone()).unwrap_or_else(query_src);
            // The array keeps the type of its elements, when every element has one, so that it is known again when
            // they are taken out.
            let element = items
                .first()
                .and_then(|i| i.ty.clone())
                .filter(|_| items.iter().all(|i| i.ty.is_some()));
            let mut values = Vec::new();
            for item in items {
                values.push(match common {
                    Some(common) => adapt(item, common)?.value,
                    None => item.value,
                });
            }
            let mut array = made(ValueKind::Array(values), pos, &src, false);
            array.ty = element.map(|t| Ty::Array(Rc::new(t)));
            Ok(Val::One(array))
        }
        "any" | "all" | "find" => {
            let c = closure_arg(args, 0, name, pos)?.clone();
            for r in stream {
                let i = r?;
                let hit = as_bool(&apply_one(&c, &env, &i)?)?;
                match (name, hit) {
                    ("any", true) => return Ok(Val::One(boolean(true, pos))),
                    ("all", false) => return Ok(Val::One(boolean(false, pos))),
                    ("find", true) => return Ok(Val::One(some(i))),
                    _ => {}
                }
            }
            Ok(Val::One(match name {
                "any" => boolean(false, pos),
                "all" => boolean(true, pos),
                _ => none(pos),
            }))
        }
        _ => at_query(pos, format!("An iterator has no method {}", name)),
    }
}

/// `skip`, which skips lazily, so that an error in a skipped value is still reported.
struct Skip {
    stream: Stream,
    left: usize,
}

impl Iterator for Skip {
    type Item = Result<Item, Failure>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.left > 0 {
            self.left -= 1;
            if let Err(e) = self.stream.next()? {
                return Some(Err(e));
            }
        }
        self.stream.next()
    }
}

fn string(item: &Item) -> Option<&str> {
    match &item.value.kind {
        ValueKind::Str(s) => Some(s),
        _ => None,
    }
}

fn value_method(of: Item, name: &str, args: &[Expr], pos: Pos, env: &Env, values: &mut Option<Stream>) -> R<Val> {
    let one = |i: Item| Ok(Val::One(i));
    let nat = |n: usize| made(ValueKind::Nat(n.to_string()), pos, &query_src(), false);
    let str_value = |s: String| made(ValueKind::Str(s), pos, &query_src(), false);
    let str_arg = |values: &mut Option<Stream>| -> R<String> {
        let Some(arg) = args.first() else {
            return at_query(pos, format!("{} takes a String", name));
        };
        let a = item(arg, env, values)?;
        match string(&a) {
            Some(s) => Ok(s.to_string()),
            None => at_item(
                &a,
                format!("{} takes a String, and this is {}", name, describe(&a.value)),
            ),
        }
    };
    if name == "update" {
        let c = closure_arg(args, 0, name, pos)?;
        return one(update(c, env, of)?);
    }
    let of = deref(&of)?;
    if let Some(text) = string(&of) {
        let text = text.to_string();
        return match name {
            "len" => one(nat(text.len())),
            "is_empty" => one(boolean(text.is_empty(), pos)),
            "contains" => one(boolean(text.contains(str_arg(values)?.as_str()), pos)),
            "starts_with" => one(boolean(text.starts_with(str_arg(values)?.as_str()), pos)),
            "ends_with" => one(boolean(text.ends_with(str_arg(values)?.as_str()), pos)),
            "trim" => one(str_value(text.trim().to_string())),
            "to_lowercase" => one(str_value(text.to_lowercase())),
            "to_uppercase" => one(str_value(text.to_uppercase())),
            "char_strings" => {
                let src = query_src();
                let items: Vec<R<Item>> = text
                    .chars()
                    .map(|c| Ok(made(ValueKind::Str(c.to_string()), pos, &src, false)))
                    .collect();
                Ok(Val::Many(Box::new(items.into_iter())))
            }
            "split" => {
                let sep = str_arg(values)?;
                let src = query_src();
                let items: Vec<R<Item>> = text
                    .split(sep.as_str())
                    .map(|s| Ok(made(ValueKind::Str(s.to_string()), pos, &src, false)))
                    .collect();
                Ok(Val::Many(Box::new(items.into_iter())))
            }
            "chars" => at_query(
                pos,
                "Constructo has no character type yet; use char_strings(), which gives strings",
            ),
            _ => at_query(pos, format!("A String has no method {}", name)),
        };
    }
    if let Some(items) = elements(&of) {
        return match name {
            "len" => one(nat(items.len())),
            "is_empty" => one(boolean(items.is_empty(), pos)),
            "first" => one(items
                .first()
                .map(|v| some(typed_part(&of, v, element_type(&of))))
                .unwrap_or_else(|| none(pos))),
            "last" => one(items
                .last()
                .map(|v| some(typed_part(&of, v, element_type(&of))))
                .unwrap_or_else(|| none(pos))),
            "get" => {
                let i = nat_arg(args, name, pos, env, values)?;
                one(items
                    .get(i)
                    .map(|v| some(typed_part(&of, v, element_type(&of))))
                    .unwrap_or_else(|| none(pos)))
            }
            "iter" => {
                let items: Vec<R<Item>> = items
                    .iter()
                    .map(|v| Ok(typed_part(&of, v, element_type(&of))))
                    .collect();
                Ok(Val::Many(Box::new(items.into_iter())))
            }
            _ => at_query(pos, format!("An array has no method {}", name)),
        };
    }
    if let ValueKind::Tagged { name: ctor, .. } = &of.value.kind {
        if matches!(canonical(ctor), "Some" | "None") {
            let inner = option(&of)?;
            return match name {
                "is_some" => one(boolean(inner.is_some(), pos)),
                "is_none" => one(boolean(inner.is_none(), pos)),
                "unwrap_or" => match inner {
                    Some(i) => one(i),
                    None => match args.first() {
                        Some(a) => one(item(a, env, values)?),
                        None => at_query(pos, "unwrap_or takes a value"),
                    },
                },
                _ => at_query(pos, format!("An Option has no method {}", name)),
            };
        }
    }
    at_item(
        &of,
        format!("{} has no method {}", capitalised(&describe(&of.value)), name),
    )
}

/// One step of the path that an assignment changes.
enum Step {
    Field(String),
    Position(usize),
    Index(usize),
}

/// `value.update(|v| v.path op= new)`: a copy of the value with the place at the path changed.
fn update(closure: &Closure, env: &Env, of: Item) -> R<Item> {
    let ExprKind::Assign { target, op, value } = &closure.body.kind else {
        return at_query(
            closure.body.pos,
            "The closure of update must assign to a path, as in |v| v.x = 1",
        );
    };
    let Some(inner) = bind(&closure.params[0], &of, env)? else {
        return at_item(&of, "This value does not match the closure's parameter");
    };
    // The path is read from the target, innermost step last, down to the parameter's name.
    let mut steps = Vec::new();
    let mut at = &**target;
    loop {
        match &at.kind {
            ExprKind::Field(next, name) => {
                steps.push(Step::Field(name.clone()));
                at = next;
            }
            ExprKind::Position(next, n) => {
                steps.push(Step::Position(*n));
                at = next;
            }
            ExprKind::Index(next, index) => {
                steps.push(Step::Index(index_of(&item(index, &inner, &mut None)?)?));
                at = next;
            }
            ExprKind::Var(_) => break,
            _ => {
                return at_query(
                    at.pos,
                    "An assignment changes a path from the closure's parameter, as in v.a.b[0]",
                )
            }
        }
    }
    steps.reverse();
    let new = item(value, &inner, &mut None)?;
    let new = match op {
        None => new,
        Some(op) => {
            let Val::One(current) = eval(target, &inner, &mut None)? else {
                return at_query(target.pos, "The target of an assignment must be one value");
            };
            arithmetic(*op, &current, &new, closure.body.pos)?
        }
    };
    let mut result = of.clone();
    set_path(&mut result.value, of.ty.clone(), &steps, &new, &of, closure.body.pos)?;
    result.lit = false;
    Ok(result)
}

fn set_path(at: &mut Value, ty: Option<Ty>, steps: &[Step], new: &Item, whole: &Item, pos: Pos) -> R<()> {
    let here = Item {
        value: at.clone(),
        src: whole.src.clone(),
        lit: false,
        ty: ty.clone(),
        file: whole.file,
    };
    let Some((step, rest)) = steps.split_first() else {
        if let Some(ty) = ty.filter(known) {
            if !fits(&ty, &new.value) {
                return at_query(
                    pos,
                    format!(
                        "The new value is {}, which does not fit where it goes, which is of type {}",
                        describe_item(new),
                        ty_text(&ty)
                    ),
                );
            }
        }
        *at = new.value.clone();
        return Ok(());
    };
    // A const is changed by changing a copy of its value, so the reference is replaced by the value.
    let here = deref(&here)?;
    *at = here.value.clone();
    let child_ty: Option<Ty>;
    let slot: &mut Value = match (step, &mut at.kind) {
        (Step::Field(name), ValueKind::Record(ms))
        | (
            Step::Field(name),
            ValueKind::Tagged {
                body: Some(TaggedBody::Record(ms)),
                ..
            },
        ) => {
            child_ty = field_type(&here, name);
            match ms.iter_mut().find(|m| &m.name == name) {
                Some(m) => &mut m.value,
                None => return at_item(&here, format!("This record has no field {}", name)),
            }
        }
        (
            Step::Field(_) | Step::Position(_),
            ValueKind::Tagged {
                name: ctor,
                body: Some(TaggedBody::Tuple(args)),
            },
        ) => {
            let param = match step {
                Step::Field(name) => Param::Named(name),
                Step::Position(n) => Param::At(*n),
                Step::Index(_) => unreachable!("matched above"),
            };
            // Finding the argument reports a parameter that is missing, or one that cannot be found without the
            // types, and gives the type of the parameter.
            let found = argument(&here, ctor, args, param)?;
            child_ty = found.ty.clone();
            let index = tuple_slot(&here, ctor, args, param);
            match index {
                Some(index) => &mut args[index].value,
                None => {
                    // A named parameter that was left out is given, by name, with the value of its default.
                    let Some(name) = param_name(&here, ctor, param) else {
                        return at_item(&here, "This argument cannot be changed");
                    };
                    args.push(Argument {
                        name: Some(name),
                        pos: here.value.pos,
                        value: found.value,
                    });
                    &mut args.last_mut().expect("an argument was just added").value
                }
            }
        }
        (Step::Index(i), ValueKind::Array(items))
        | (
            Step::Index(i),
            ValueKind::Tagged {
                body: Some(TaggedBody::Array(items)),
                ..
            },
        ) => {
            child_ty = element_type(&here);
            let len = items.len();
            match items.get_mut(*i) {
                Some(v) => v,
                None => {
                    return at_item(
                        &here,
                        format!("The index {} is out of range for an array of {}", i, len),
                    )
                }
            }
        }
        (Step::Field(name), _) => {
            return at_item(
                &here,
                format!("{} has no field {}", capitalised(&describe_item(&here)), name),
            )
        }
        (Step::Position(n), _) => {
            return at_item(
                &here,
                format!("{} has no argument {}", capitalised(&describe_item(&here)), n),
            )
        }
        (Step::Index(_), _) => {
            return at_item(
                &here,
                format!(
                    "{} is not an array, so it cannot be indexed",
                    capitalised(&describe_item(&here))
                ),
            )
        }
    };
    set_path(slot, child_ty, rest, new, whole, pos)
}

/// The index of the argument that is for a parameter, if one was given.
fn tuple_slot(of: &Item, ctor: &str, args: &[Argument], param: Param) -> Option<usize> {
    if let Some(typing) = typing() {
        if let Some((params, _)) = typing.types.tuple_params(ctor, of.ty.as_ref()) {
            let index = match param {
                Param::At(n) => n,
                Param::Named(name) => params.iter().position(|p| p.name.as_deref() == Some(name))?,
            };
            let (targets, _) = bind_arguments(of.value.pos, ctor, &params, args).ok()?;
            return targets.iter().position(|t| *t == index);
        }
    }
    match param {
        Param::At(n) => args
            .iter()
            .enumerate()
            .filter(|(_, a)| a.name.is_none())
            .nth(n)
            .map(|(i, _)| i),
        Param::Named(name) => args.iter().position(|a| a.name.as_deref() == Some(name)),
    }
}

/// The name of a parameter, which an argument that is added must be given by.
fn param_name(of: &Item, ctor: &str, param: Param) -> Option<String> {
    match param {
        Param::Named(name) => Some(name.to_string()),
        Param::At(n) => typing()?
            .types
            .tuple_params(ctor, of.ty.as_ref())?
            .0
            .get(n)?
            .name
            .clone(),
    }
}

/// Whether a value fits where a type is expected, as far as its kind can tell, with the widening of numbers.
fn fits(ty: &Ty, value: &Value) -> bool {
    if !known(ty) {
        return true;
    }
    match (ty, &value.kind) {
        (Ty::Nat, ValueKind::Nat(_)) => true,
        (Ty::Int, ValueKind::Int(_) | ValueKind::Nat(_)) => true,
        (Ty::Float, ValueKind::Float(_) | ValueKind::Int(_) | ValueKind::Nat(_)) => true,
        (Ty::Str, ValueKind::Str(_)) => true,
        (Ty::Array(element), ValueKind::Array(items)) => items.iter().all(|item| fits(element, item)),
        (Ty::Record(fields), ValueKind::Record(members)) => {
            members.len() == fields.len()
                && members
                    .iter()
                    .all(|m| fields.iter().any(|(name, t)| *name == m.name && fits(t, &m.value)))
        }
        (Ty::Enum(name, args), ValueKind::Tagged { name: ctor, body }) => {
            match (canonical(shown(name)), canonical(ctor), body) {
                ("Bool", "true" | "false", None) | ("Option", "None", None) => return true,
                ("Option", "Some", Some(TaggedBody::Tuple(items))) if items.len() == 1 && args.len() == 1 => {
                    return fits(&args[0], &items[0].value)
                }
                _ => {}
            }
            let Some(typing) = typing() else { return true };
            let Some((decl, _)) = typing.types.ctor(ctor) else {
                // A const of the right type fits too, which its type would say; it is taken on trust here.
                return crate::convert::is_const_name(ctor);
            };
            if decl.name != *name {
                return false;
            }
            fits_body(&typing.types, ctor, ty, value, body.as_ref())
        }
        _ => false,
    }
}

/// Whether the body of a constructor fits the declaration of the constructor, in the enum type that is expected.
fn fits_body(types: &super::types::Types, ctor: &str, ty: &Ty, value: &Value, body: Option<&TaggedBody>) -> bool {
    match body {
        None => matches!(types.ctor(ctor), Some((_, v)) if matches!(v.body, crate::ast::VariantBody::Unit)),
        Some(TaggedBody::Record(members)) => match types.ctor_fields(ctor, Some(ty)) {
            Some(fields) => {
                members.len() == fields.len()
                    && members
                        .iter()
                        .all(|m| fields.iter().any(|(name, t)| *name == m.name && fits(t, &m.value)))
            }
            None => false,
        },
        Some(TaggedBody::Array(items)) => match types.ctor_element(ctor, Some(ty)) {
            Some(element) => items.iter().all(|item| fits(&element, item)),
            None => false,
        },
        Some(TaggedBody::Tuple(args)) => match types.tuple_params(ctor, Some(ty)) {
            Some((params, tys)) => match bind_arguments(value.pos, ctor, &params, args) {
                Ok((targets, _)) => args.iter().zip(&targets).all(|(a, t)| fits(&tys[*t], &a.value)),
                Err(_) => false,
            },
            None => false,
        },
    }
}

/// A type as a message shows it.
fn ty_text(ty: &Ty) -> String {
    super::types::type_name(ty)
}

fn arithmetic(op: BinOp, a: &Item, b: &Item, pos: Pos) -> R<Item> {
    let (a, b) = (&deref(a)?, &deref(b)?);
    if op == BinOp::Add {
        if let (Some(x), Some(y)) = (string(a), string(b)) {
            return Ok(made(
                ValueKind::Str(format!("{}{}", x, y)),
                pos,
                &query_src(),
                a.lit && b.lit,
            ));
        }
    }
    for operand in [a, b] {
        if number(operand).is_none() {
            return at_item(
                operand,
                format!(
                    "{} needs numbers, or strings for +, and this is {}",
                    op.symbol(),
                    describe(&operand.value)
                ),
            );
        }
    }
    let (kind, x, y) = unify(a, b, "combined with")?;
    let blamed = if a.lit && !b.lit { b } else { a };
    let text = number::arithmetic(op, kind, &x, &y).map_err(num_error(blamed))?;
    Ok(num_value(text, blamed.value.pos, &blamed.src, a.lit && b.lit))
}

fn binary(op: BinOp, left: &Expr, right: &Expr, pos: Pos, env: &Env, values: &mut Option<Stream>) -> R<Val> {
    let a = item(left, env, values)?;
    match op {
        BinOp::And | BinOp::Or => {
            let first = as_bool(&a)?;
            if (op == BinOp::And && !first) || (op == BinOp::Or && first) {
                return Ok(Val::One(boolean(first, pos)));
            }
            let second = as_bool(&item(right, env, values)?)?;
            return Ok(Val::One(boolean(second, pos)));
        }
        _ => {}
    }
    let b = deref(&item(right, env, values)?)?;
    let a = deref(&a)?;
    if op.is_arithmetic() {
        return Ok(Val::One(arithmetic(op, &a, &b, pos)?));
    }
    if op.is_ordering() {
        let ordering = match (string(&a), string(&b)) {
            (Some(x), Some(y)) => x.cmp(y),
            _ => {
                for operand in [&a, &b] {
                    if number(operand).is_none() {
                        return at_item(
                            operand,
                            format!(
                                "{} compares numbers or strings, and this is {}",
                                op.symbol(),
                                describe(&operand.value)
                            ),
                        );
                    }
                }
                let (_, x, y) = unify(&a, &b, "compared with")?;
                number::compare(&x, &y).map_err(num_error(&a))?
            }
        };
        let result = match op {
            BinOp::Lt => ordering.is_lt(),
            BinOp::Le => ordering.is_le(),
            BinOp::Gt => ordering.is_gt(),
            _ => ordering.is_ge(),
        };
        return Ok(Val::One(boolean(result, pos)));
    }
    let same = equal(&a, &b)?;
    Ok(Val::One(boolean(if op == BinOp::Eq { same } else { !same }, pos)))
}

/// Whether two values are equal, structurally. Numbers must be of one kind, a literal adapting, and values of
/// different kinds are an error, as they have no common type.
fn equal(a: &Item, b: &Item) -> R<bool> {
    let a = &deref(a)?;
    let b = &deref(b)?;
    let mismatch = || {
        let (blamed, other) = if a.lit && !b.lit { (b, a) } else { (a, b) };
        at_item(
            blamed,
            format!(
                "{} is compared with {}, and they have no common type",
                capitalised(&describe_item(blamed)),
                describe_item(other)
            ),
        )
    };
    match (&a.value.kind, &b.value.kind) {
        (
            ValueKind::Nat(_) | ValueKind::Int(_) | ValueKind::Float(_),
            ValueKind::Nat(_) | ValueKind::Int(_) | ValueKind::Float(_),
        ) => {
            let (_, p, q) = unify(a, b, "compared with")?;
            Ok(number::compare(&p, &q).map_err(num_error(a))?.is_eq())
        }
        (ValueKind::Str(p), ValueKind::Str(q)) => Ok(p == q),
        (ValueKind::Array(p), ValueKind::Array(q)) => sequence(a, p, b, q),
        (ValueKind::Record(p), ValueKind::Record(q)) => members(a, p, b, q),
        (ValueKind::Tagged { name: n, body: p }, ValueKind::Tagged { name: m, body: q }) => {
            if canonical(n) != canonical(m) {
                return Ok(false);
            }
            match (p, q) {
                (None, None) => Ok(true),
                (Some(TaggedBody::Record(p)), Some(TaggedBody::Record(q))) => members(a, p, b, q),
                (Some(TaggedBody::Array(p)), Some(TaggedBody::Array(q))) => sequence(a, p, b, q),
                (Some(TaggedBody::Tuple(p)), Some(TaggedBody::Tuple(q))) => {
                    if p.len() != q.len() {
                        return Ok(false);
                    }
                    let mut position = 0;
                    for (u, v) in p.iter().zip(q) {
                        if u.name != v.name {
                            return Ok(false);
                        }
                        let param = match &u.name {
                            Some(name) => Param::Named(name),
                            None => {
                                position += 1;
                                Param::At(position - 1)
                            }
                        };
                        let x = typed_part(a, &u.value, param_type(a, n, param));
                        let y = typed_part(b, &v.value, param_type(b, m, param));
                        if !equal(&x, &y)? {
                            return Ok(false);
                        }
                    }
                    Ok(true)
                }
                _ => Ok(false),
            }
        }
        _ => mismatch(),
    }
}

/// The type of a parameter of a tuple constructor, if the types give it.
fn param_type(of: &Item, ctor: &str, param: Param) -> Option<Ty> {
    let (params, tys) = typing()?.types.tuple_params(ctor, of.ty.as_ref())?;
    let index = match param {
        Param::At(n) => n,
        Param::Named(name) => params.iter().position(|p| p.name.as_deref() == Some(name))?,
    };
    tys.get(index).cloned().filter(known)
}

fn sequence(a: &Item, p: &[Value], b: &Item, q: &[Value]) -> R<bool> {
    if p.len() != q.len() {
        return Ok(false);
    }
    let (ta, tb) = (element_type(a), element_type(b));
    for (u, v) in p.iter().zip(q) {
        if !equal(&typed_part(a, u, ta.clone()), &typed_part(b, v, tb.clone()))? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn members(a: &Item, p: &[Member], b: &Item, q: &[Member]) -> R<bool> {
    if p.len() != q.len() {
        return Ok(false);
    }
    for m in p {
        match q.iter().find(|n| n.name == m.name) {
            Some(n) => {
                let x = typed_part(a, &m.value, field_type(a, &m.name));
                let y = typed_part(b, &n.value, field_type(b, &n.name));
                if !equal(&x, &y)? {
                    return Ok(false);
                }
            }
            None => return Ok(false),
        }
    }
    Ok(true)
}

/// Matches an item against a pattern. `Some` with the names bound if it matches, `None` if it does not, and an error
/// if the value is of a kind that the pattern could never match, which is a type error.
pub fn bind(pattern: &Pattern, of: &Item, env: &Env) -> R<Option<Env>> {
    let wrong = |what: &str| {
        at_item(
            of,
            format!(
                "The pattern is {}, but this is {}, so they have no common type",
                what,
                describe(&of.value)
            ),
        )
    };
    match &pattern.kind {
        PatternKind::Wild => Ok(Some(env.clone())),
        PatternKind::Bind(name) => Ok(Some(env.with(name, of.clone()))),
        PatternKind::At(name, inner) => Ok(bind(inner, of, env)?.map(|e| e.with(name, of.clone()))),
        PatternKind::Or(alternatives) => {
            for p in alternatives {
                if let Some(e) = bind(p, of, env)? {
                    return Ok(Some(e));
                }
            }
            Ok(None)
        }
        PatternKind::Lit(lit) => {
            let kind = match lit {
                Lit::Nat(t) => ValueKind::Nat(t.clone()),
                Lit::Int(t) => ValueKind::Int(t.clone()),
                Lit::Float(t) => ValueKind::Float(t.clone()),
                Lit::Str(t) => ValueKind::Str(t.clone()),
            };
            let lit_item = made(kind, pattern.pos, &query_src(), true);
            Ok(equal(of, &lit_item)?.then(|| env.clone()))
        }
        PatternKind::Ctor { name, body } => {
            let of = &deref(of)?;
            let ValueKind::Tagged {
                name: found,
                body: value_body,
            } = &of.value.kind
            else {
                return wrong(&format!("the constructor {}", name));
            };
            if canonical(found) != canonical(name) {
                return Ok(None);
            }
            match (body, value_body) {
                (None, None) => Ok(Some(env.clone())),
                (None, Some(_)) => at_item(
                    of,
                    format!(
                        "{} has a body, which the pattern must match, as in {} {{ .. }} or {}(..)",
                        found, name, name
                    ),
                ),
                (Some(_), None) => at_item(of, format!("{} has no body, but the pattern gives one", found)),
                (Some(PatBody::Record { fields, rest }), Some(TaggedBody::Record(ms))) => {
                    bind_members(fields, *rest, ms, of, env)
                }
                (Some(PatBody::Array { items, rest }), Some(TaggedBody::Array(vs))) => {
                    bind_items(items, *rest, vs, of, env)
                }
                (Some(PatBody::Tuple { args, rest }), Some(TaggedBody::Tuple(vs))) => {
                    bind_args(args, *rest, vs, found, of, env)
                }
                _ => at_item(
                    of,
                    format!("This {} has a different kind of body from the pattern", found),
                ),
            }
        }
        PatternKind::Record { fields, rest } => match &deref(of)?.value.kind {
            ValueKind::Record(ms) => bind_members(fields, *rest, ms, of, env),
            _ => wrong("a record"),
        },
        PatternKind::Array { items, rest } => match &deref(of)?.value.kind {
            ValueKind::Array(vs) => bind_items(items, *rest, vs, of, env),
            _ => wrong("an array"),
        },
    }
}

fn bind_members(fields: &[FieldPat], rest: bool, ms: &[Member], of: &Item, env: &Env) -> R<Option<Env>> {
    if !rest {
        if let Some(extra) = ms.iter().find(|m| !fields.iter().any(|f| f.name == m.name)) {
            return at_item(
                of,
                format!(
                    "This has the field {}, which the pattern does not mention; add .. to ignore it",
                    extra.name
                ),
            );
        }
    }
    let mut env = env.clone();
    for f in fields {
        let Some(m) = ms.iter().find(|m| m.name == f.name) else {
            return at_item(of, format!("This has no field {}", f.name));
        };
        match bind(&f.pattern, &typed_part(of, &m.value, field_type(of, &f.name)), &env)? {
            Some(e) => env = e,
            None => return Ok(None),
        }
    }
    Ok(Some(env))
}

fn bind_items(items: &[Pattern], rest: bool, vs: &[Value], of: &Item, env: &Env) -> R<Option<Env>> {
    if vs.len() < items.len() || (!rest && vs.len() != items.len()) {
        return Ok(None);
    }
    let ty = element_type(of);
    let mut env = env.clone();
    for (p, v) in items.iter().zip(vs) {
        match bind(p, &typed_part(of, v, ty.clone()), &env)? {
            Some(e) => env = e,
            None => return Ok(None),
        }
    }
    Ok(Some(env))
}

fn bind_args(args: &[ArgPat], rest: bool, vs: &[Argument], ctor: &str, of: &Item, env: &Env) -> R<Option<Env>> {
    // Typed, the pattern is against the parameters, however the arguments were written; untyped, against the
    // arguments as they are written.
    let typed_count = typing().and_then(|t| t.types.tuple_params(ctor, of.ty.as_ref()).map(|(p, _)| p.len()));
    if !rest {
        let mentioned = args.len();
        let all = match typed_count {
            Some(count) => mentioned == count,
            None => {
                let positional = vs.iter().filter(|a| a.name.is_none()).count();
                let pattern_positional = args.iter().filter(|a| a.name.is_none()).count();
                pattern_positional == positional && mentioned == vs.len()
            }
        };
        if !all {
            return at_item(
                of,
                format!(
                    "The pattern does not mention every argument of this {}; add .. to ignore the rest",
                    shown(ctor)
                ),
            );
        }
    }
    let mut env = env.clone();
    let mut next_position = 0;
    for a in args {
        let param = match &a.name {
            None => {
                next_position += 1;
                Param::At(next_position - 1)
            }
            Some(name) => Param::Named(name),
        };
        let value = argument(of, ctor, vs, param)?;
        match bind(&a.pattern, &value, &env)? {
            Some(e) => env = e,
            None => return Ok(None),
        }
    }
    Ok(Some(env))
}
