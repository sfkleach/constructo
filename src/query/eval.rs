//! Evaluates a query over a lazy stream of values.
//!
//! What the query itself fixes has been checked by `check` before this runs. Here every value is checked as it
//! arrives, by its kind, and a mismatch is an error at that value. See the settled questions of
//! `docs/tasks/2026-11-15-query-subcommand/query-subcommand.md`.

use super::number::{self, kind_of};
use super::parser::QUERY_NAME;
use super::syntax::*;
use crate::ast::{Argument, Member, TaggedBody, Value, ValueKind};
use crate::diagnostics::{Diagnostic, Pos};
use std::rc::Rc;

/// A value with where it came from: the name of its source and, through its position, where it is in that source.
#[derive(Debug, Clone)]
pub struct Item {
    pub value: Value,
    pub src: Rc<str>,
    /// Whether the value is a literal of the query, or made only from literals, which adapts to the kind of number it
    /// meets, as `1` does in `m.count + 1`.
    pub lit: bool,
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
    }
}

/// A part of an item, which comes from the same source.
fn part(of: &Item, value: &Value) -> Item {
    Item {
        value: value.clone(),
        src: of.src.clone(),
        lit: of.lit,
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
    let arg = Argument {
        name: None,
        pos,
        value: item.value,
    };
    made(tagged("Some", Some(TaggedBody::Tuple(vec![arg]))), pos, &src, false)
}

fn none(pos: Pos) -> Item {
    made(tagged("None", None), pos, &query_src(), false)
}

/// The name of a constructor or const of the prelude, with `true` and `True` made one, and `null` and `None`, since
/// the prelude declares each const as the other.
pub fn canonical(name: &str) -> &str {
    match name {
        "True" => "true",
        "False" => "false",
        "null" => "None",
        other => other,
    }
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
        ValueKind::Tagged { name, .. } if crate::convert::is_const_name(name) => format!("the const {}", name),
        ValueKind::Tagged { name, .. } => format!("the constructor {}", name),
    }
}

fn as_bool(item: &Item) -> R<bool> {
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
    match &item.value.kind {
        ValueKind::Nat(t) | ValueKind::Int(t) | ValueKind::Float(t) => Some(t),
        _ => None,
    }
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
    let (ta, tb) = (number(a).expect("checked"), number(b).expect("checked"));
    let (ka, kb) = (kind_of(ta), kind_of(tb));
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

/// Evaluates an expression. `values` holds the stream of the inputs until `@values` takes it.
pub fn eval(expr: &Expr, env: &Env, values: &mut Option<Stream>) -> R<Val> {
    let pos = expr.pos;
    let one = |item: Item| Ok(Val::One(item));
    match &expr.kind {
        ExprKind::Values => match values.take() {
            Some(stream) => Ok(Val::Many(stream)),
            None => at_query(pos, "@values can only be used once"),
        },
        ExprKind::Var(name) => match env.get(name) {
            Some(item) => one(item.clone()),
            None if matches!(name.as_str(), "true" | "false" | "null") => {
                one(made(tagged(name, None), pos, &query_src(), false))
            }
            None => at_query(pos, format!("{} is not bound", name)),
        },
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
            let operand = item(operand, env, values)?;
            match op {
                UnOp::Not => one(boolean(!as_bool(&operand)?, pos)),
                UnOp::Neg => {
                    let Some(text) = number(&operand) else {
                        return at_item(
                            &operand,
                            format!("Expected a number but found {}", describe(&operand.value)),
                        );
                    };
                    // A literal Nat that is negated is an Int literal, as `- 5` is `-5`.
                    let text = if operand.lit && kind_of(text) == NumKind::Nat {
                        format!("-{}", text)
                    } else {
                        number::negate(text).map_err(num_error(&operand))?
                    };
                    one(num_value(text, operand.value.pos, &operand.src, operand.lit))
                }
            }
        }
        ExprKind::Binary(op, left, right) => binary(*op, left, right, pos, env, values),
        ExprKind::Cast(operand, to) => {
            let operand = item(operand, env, values)?;
            let Some(text) = number(&operand) else {
                return at_item(
                    &operand,
                    format!("Only a number can be cast, and this is {}", describe(&operand.value)),
                );
            };
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
    if let Val::One(found) = &result {
        let others: Vec<&Expr> = match &whole.kind {
            ExprKind::If { branches, otherwise } => branches
                .iter()
                .map(|(_, b)| b)
                .chain(std::iter::once(&**otherwise))
                .collect(),
            ExprKind::Match { arms, .. } => arms.iter().map(|a| &a.body).collect(),
            _ => Vec::new(),
        };
        for other in others {
            if let Some(fixed) = super::check::fixed_kind(other) {
                if !fixed.admits(&found.value, found.lit) {
                    return at_item(
                        found,
                        format!(
                            "This branch gives {}, but another gives {}, and they have no common type",
                            describe(&found.value),
                            fixed.describe()
                        ),
                    );
                }
            }
        }
    }
    Ok(result)
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
    common_kind(&out)?;
    Ok(out.into_iter().map(|i| i.value).collect())
}

/// Checks that the elements of an array have a common type, as far as their kinds can tell: numbers of one kind (a
/// literal adapting), or all strings, or all records, and so on. Constructors are not told apart without the types.
fn common_kind(items: &[Item]) -> R<()> {
    let Some(first) = items.iter().find(|i| !i.lit).or(items.first()) else {
        return Ok(());
    };
    let fixed = super::check::Fixed::of(&first.value);
    for item in items {
        if !fixed.admits(&item.value, item.lit)
            && !(first.lit && super::check::Fixed::of(&item.value).admits(&first.value, true))
        {
            return at_item(
                item,
                format!(
                    "An array's elements must have a common type, but this is {} and another is {}",
                    describe(&item.value),
                    fixed.describe()
                ),
            );
        }
    }
    Ok(())
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
fn const_reference(item: &Item) -> R<()> {
    if let ValueKind::Tagged { name, body: None } = &item.value.kind {
        if crate::convert::is_const_name(name) && !matches!(name.as_str(), "true" | "false" | "null") {
            return at_item(
                item,
                format!(
                    "{} is a const, which cannot be seen into without the types; supply the types",
                    name
                ),
            );
        }
    }
    Ok(())
}

fn field(of: &Item, name: &str) -> R<Item> {
    const_reference(of)?;
    let members = match &of.value.kind {
        ValueKind::Record(ms) => ms,
        ValueKind::Tagged {
            body: Some(TaggedBody::Record(ms)),
            ..
        } => ms,
        ValueKind::Tagged {
            name: ctor,
            body: Some(TaggedBody::Tuple(args)),
        } => {
            return match args.iter().find(|a| a.name.as_deref() == Some(name)) {
                Some(a) => Ok(part(of, &a.value)),
                None => at_item(
                    of,
                    format!(
                        "This {} does not give {} by name, and without the types it cannot be known whether it is \
                         given by position or left to its default; supply the types",
                        ctor, name
                    ),
                ),
            };
        }
        ValueKind::Tagged { name: ctor, body: None } => {
            return at_item(of, format!("{} has no body, so it has no field {}", ctor, name))
        }
        _ => {
            return at_item(
                of,
                format!(
                    "{} has no fields, so it has no field {}",
                    capitalised(&describe(&of.value)),
                    name
                ),
            )
        }
    };
    match members.iter().find(|m| m.name == name) {
        Some(m) => Ok(part(of, &m.value)),
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
    const_reference(of)?;
    let ValueKind::Tagged { name, body } = &of.value.kind else {
        return at_item(
            of,
            format!(
                "{} has no arguments, so it has no argument {}",
                capitalised(&describe(&of.value)),
                n
            ),
        );
    };
    let Some(TaggedBody::Tuple(args)) = body else {
        return at_item(of, format!("{} has no tuple, so it has no argument {}", name, n));
    };
    let positional: Vec<&Argument> = args.iter().filter(|a| a.name.is_none()).collect();
    match positional.get(n) {
        Some(a) => Ok(part(of, &a.value)),
        None if args.iter().any(|a| a.name.is_some()) => at_item(
            of,
            format!(
                "This {} has {} positional argument(s), and without the types it cannot be known which parameter \
                 is {}; supply the types",
                name,
                positional.len(),
                n
            ),
        ),
        None => at_item(of, format!("This {} has no argument {}", name, n)),
    }
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
    const_reference(of)?;
    let Some(items) = elements(of) else {
        return at_item(
            of,
            format!(
                "{} is not an array, so it cannot be indexed",
                capitalised(&describe(&of.value))
            ),
        );
    };
    let i = index_of(index)?;
    match items.get(i) {
        Some(v) => Ok(part(of, v)),
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
    if let ValueKind::Tagged { name, body } = &item.value.kind {
        match (canonical(name), body) {
            ("None", None) => return Ok(None),
            ("Some", Some(TaggedBody::Tuple(args))) if args.len() == 1 && args[0].name.is_none() => {
                return Ok(Some(part(item, &args[0].value)))
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
                let items: Vec<Item> = items.iter().map(|v| part(&of, v)).collect();
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
            common_kind(&items)?;
            let src = items.first().map(|i| i.src.clone()).unwrap_or_else(query_src);
            let values = items.into_iter().map(|i| i.value).collect();
            Ok(Val::One(made(ValueKind::Array(values), pos, &src, false)))
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
    const_reference(&of)?;
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
            "first" => one(items.first().map(|v| some(part(&of, v))).unwrap_or_else(|| none(pos))),
            "last" => one(items.last().map(|v| some(part(&of, v))).unwrap_or_else(|| none(pos))),
            "get" => {
                let i = nat_arg(args, name, pos, env, values)?;
                one(items.get(i).map(|v| some(part(&of, v))).unwrap_or_else(|| none(pos)))
            }
            "iter" => {
                let items: Vec<R<Item>> = items.iter().map(|v| Ok(part(&of, v))).collect();
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
            let current = eval(target, &inner, &mut None)?;
            let Val::One(current) = current else {
                unreachable!("a path gives one value")
            };
            arithmetic(*op, &current, &new, closure.body.pos)?
        }
    };
    let mut result = of.clone();
    set_path(&mut result.value, &steps, new.value, &of)?;
    result.lit = false;
    Ok(result)
}

fn set_path(at: &mut Value, steps: &[Step], new: Value, whole: &Item) -> R<()> {
    let Some((step, rest)) = steps.split_first() else {
        *at = new;
        return Ok(());
    };
    let here = Item {
        value: at.clone(),
        src: whole.src.clone(),
        lit: false,
    };
    let slot: &mut Value = match (step, &mut at.kind) {
        (Step::Field(name), ValueKind::Record(ms))
        | (
            Step::Field(name),
            ValueKind::Tagged {
                body: Some(TaggedBody::Record(ms)),
                ..
            },
        ) => match ms.iter_mut().find(|m| &m.name == name) {
            Some(m) => &mut m.value,
            None => return at_item(&here, format!("This record has no field {}", name)),
        },
        (
            Step::Field(name),
            ValueKind::Tagged {
                body: Some(TaggedBody::Tuple(args)),
                ..
            },
        ) => match args.iter_mut().find(|a| a.name.as_deref() == Some(name)) {
            Some(a) => &mut a.value,
            None => {
                field(&here, name)?;
                unreachable!("field fails when the argument is not given by name")
            }
        },
        (
            Step::Position(n),
            ValueKind::Tagged {
                body: Some(TaggedBody::Tuple(args)),
                ..
            },
        ) => match args.iter_mut().filter(|a| a.name.is_none()).nth(*n) {
            Some(a) => &mut a.value,
            None => {
                position(&here, *n)?;
                unreachable!("position fails when there is no such argument")
            }
        },
        (Step::Index(i), ValueKind::Array(items))
        | (
            Step::Index(i),
            ValueKind::Tagged {
                body: Some(TaggedBody::Array(items)),
                ..
            },
        ) => {
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
            field(&here, name)?;
            unreachable!("field fails on a value without fields")
        }
        (Step::Position(n), _) => {
            position(&here, *n)?;
            unreachable!("position fails on a value without a tuple")
        }
        (Step::Index(_), _) => {
            return at_item(
                &here,
                format!(
                    "{} is not an array, so it cannot be indexed",
                    capitalised(&describe(&here.value))
                ),
            )
        }
    };
    set_path(slot, rest, new, whole)
}

fn arithmetic(op: BinOp, a: &Item, b: &Item, pos: Pos) -> R<Item> {
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
    let b = item(right, env, values)?;
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
                number::compare(&x, &y)
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
    let same = equal(&a, &a.value, &b, &b.value)?;
    Ok(Val::One(boolean(if op == BinOp::Eq { same } else { !same }, pos)))
}

/// Whether two values are equal, structurally. Numbers must be of one kind, a literal adapting, and values of
/// different kinds are an error, as they have no common type.
fn equal(a: &Item, x: &Value, b: &Item, y: &Value) -> R<bool> {
    let ia = Item {
        value: x.clone(),
        src: a.src.clone(),
        lit: a.lit,
    };
    let ib = Item {
        value: y.clone(),
        src: b.src.clone(),
        lit: b.lit,
    };
    let mismatch = || {
        let blamed = if ia.lit && !ib.lit { &ib } else { &ia };
        let other = if ia.lit && !ib.lit { &ia } else { &ib };
        at_item(
            blamed,
            format!(
                "{} is compared with {}, and they have no common type",
                capitalised(&describe(&blamed.value)),
                describe(&other.value)
            ),
        )
    };
    match (&x.kind, &y.kind) {
        (
            ValueKind::Nat(_) | ValueKind::Int(_) | ValueKind::Float(_),
            ValueKind::Nat(_) | ValueKind::Int(_) | ValueKind::Float(_),
        ) => {
            let (_, p, q) = unify(&ia, &ib, "compared with")?;
            Ok(number::compare(&p, &q).is_eq())
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
                    for (u, v) in p.iter().zip(q) {
                        if u.name != v.name || !equal(a, &u.value, b, &v.value)? {
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

fn sequence(a: &Item, p: &[Value], b: &Item, q: &[Value]) -> R<bool> {
    if p.len() != q.len() {
        return Ok(false);
    }
    for (u, v) in p.iter().zip(q) {
        if !equal(a, u, b, v)? {
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
                if !equal(a, &m.value, b, &n.value)? {
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
            Ok(equal(of, &of.value, &lit_item, &lit_item.value)?.then(|| env.clone()))
        }
        PatternKind::Ctor { name, body } => {
            const_reference(of)?;
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
        PatternKind::Record { fields, rest } => match &of.value.kind {
            ValueKind::Record(ms) => bind_members(fields, *rest, ms, of, env),
            _ => wrong("a record"),
        },
        PatternKind::Array { items, rest } => match &of.value.kind {
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
        match bind(&f.pattern, &part(of, &m.value), &env)? {
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
    let mut env = env.clone();
    for (p, v) in items.iter().zip(vs) {
        match bind(p, &part(of, v), &env)? {
            Some(e) => env = e,
            None => return Ok(None),
        }
    }
    Ok(Some(env))
}

fn bind_args(args: &[ArgPat], rest: bool, vs: &[Argument], ctor: &str, of: &Item, env: &Env) -> R<Option<Env>> {
    let positional: Vec<&Argument> = vs.iter().filter(|a| a.name.is_none()).collect();
    let pattern_positional = args.iter().filter(|a| a.name.is_none()).count();
    if !rest && (pattern_positional != positional.len() || args.len() != vs.len()) {
        return at_item(
            of,
            format!(
                "The pattern does not mention every argument of this {}; add .. to ignore the rest",
                ctor
            ),
        );
    }
    let mut env = env.clone();
    let mut next_position = 0;
    for a in args {
        let value = match &a.name {
            None => {
                let found = positional.get(next_position).map(|a| &a.value);
                next_position += 1;
                match found {
                    Some(v) => v,
                    None => return at_item(of, format!("This {} has no argument {}", ctor, next_position - 1)),
                }
            }
            Some(name) => match vs.iter().find(|v| v.name.as_deref() == Some(name)) {
                Some(v) => &v.value,
                None => {
                    return at_item(
                        of,
                        format!(
                            "This {} does not give {} by name, and without the types it cannot be known whether it \
                             is given by position or left to its default; supply the types",
                            ctor, name
                        ),
                    )
                }
            },
        };
        match bind(&a.pattern, &part(of, value), &env)? {
            Some(e) => env = e,
            None => return Ok(None),
        }
    }
    Ok(Some(env))
}
