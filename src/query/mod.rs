//! Queries: Rust-like expressions over `@values`, the lazy stream of the input values, which extract data and
//! transform it. See `docs/decisions/0044-queries-are-rust-like-expressions-over-a-stream-of-values.md` and the task,
//! `docs/tasks/done/2026-11-15-query-subcommand/query-subcommand.md`, which settles the details.

pub mod check;
pub mod eval;
pub mod lexer;
pub mod number;
pub mod parser;
pub mod syntax;
pub mod typed_check;
pub mod types;

use crate::ast::Value;
use crate::diagnostics::Diagnostic;
use crate::diagnostics::Pos;
use crate::parser::Experimental;
use crate::parser::{parse_file, Parsed};
use crate::syntax_diagnostic;
use crate::Source;
use eval::{eval, Env, Failure, Item, Stream, Val};
use std::rc::Rc;
use syntax::Expr;

pub use eval::Failure as QueryFailure;

/// Reads a query and checks what it fixes, before any data is read.
pub fn prepare(text: &str) -> Result<Expr, Diagnostic> {
    let expr = parser::parse_query(text)?;
    check::check_query(&expr)?;
    Ok(expr)
}

/// The values of the sources, in order, as a lazy stream. Each source is only read, and parsed, when the stream
/// reaches it, so that `@values.take(1)` stops reading early. A source is given by a function, so that reading it is
/// also left until it is needed; its error is a message about how the input is found.
pub fn stream_of(
    sources: impl Iterator<Item = Box<dyn FnOnce() -> Result<Source, String>>> + 'static,
    experimental: Experimental,
) -> Stream {
    let mut sources = sources;
    let mut current: std::vec::IntoIter<Value> = Vec::new().into_iter();
    let mut src: Rc<str> = Rc::from("");
    let mut file = 0;
    let mut next_file = 0;
    let mut failed = false;
    Box::new(std::iter::from_fn(move || loop {
        if failed {
            return None;
        }
        if let Some(value) = current.next() {
            return Some(Ok(Item {
                value,
                src: src.clone(),
                lit: false,
                ty: None,
                file,
            }));
        }
        let read = sources.next()?;
        let source = match read() {
            Ok(source) => source,
            Err(message) => {
                failed = true;
                return Some(Err(Failure::Input(message)));
            }
        };
        match values_in(&source, experimental) {
            Ok(values) => {
                src = Rc::from(source.name.as_str());
                current = values.into_iter();
                file = next_file;
                next_file += 1;
            }
            Err(diagnostic) => {
                failed = true;
                return Some(Err(Failure::Error(diagnostic)));
            }
        }
    }))
}

/// The values of a source. A syntax error is an error, and so is a types file, which an untyped query is not given: a
/// query with types files is typed, and reads them first. A file with nothing in it, which the parser takes for a types
/// file, has no values.
fn values_in(source: &Source, experimental: Experimental) -> Result<Vec<Value>, Diagnostic> {
    match parse_file(&source.text, experimental).1 {
        Err(e) => Err(syntax_diagnostic(source, e)),
        Ok(Parsed::Types(file)) if file.enums.is_empty() && file.consts.is_empty() => Ok(Vec::new()),
        Ok(Parsed::Types(_)) => {
            let message = format!(
                "{} is a types file, which must come before the values files",
                source.name
            );
            Err(Diagnostic::new(&source.name, Pos { line: 1, col: 1 }, message))
        }
        Ok(Parsed::Values(found)) => Ok(found),
    }
}

/// Runs a query that `prepare` accepted over a stream of values, passing each value of the result to `out` as it is
/// made: each item if the result is an iterator, or the one value otherwise.
pub fn run(
    expr: &Expr,
    values: Stream,
    out: &mut dyn FnMut(&Value) -> Result<(), std::io::Error>,
) -> Result<(), RunError> {
    eval::with_typing(None, || run_items(expr, values, &mut |item: &Item| out(&item.value)))
}

fn run_items(
    expr: &Expr,
    values: Stream,
    out: &mut dyn FnMut(&Item) -> Result<(), std::io::Error>,
) -> Result<(), RunError> {
    let mut values = Some(values);
    let result = eval(expr, &Env::default(), &mut values).map_err(RunError::Failed)?;
    match result {
        Val::One(item) => out(&item).map_err(RunError::Write),
        Val::Many(stream) => {
            for item in stream {
                let item = item.map_err(RunError::Failed)?;
                out(&item).map_err(RunError::Write)?;
            }
            Ok(())
        }
    }
}

/// Why a run stopped early.
#[derive(Debug)]
pub enum RunError {
    Failed(Failure),
    /// A mistake in a typed query that the types found before any value was read: exit code 2.
    Query(Diagnostic),
    Write(std::io::Error),
}

#[cfg(test)]
mod tests;

/// Visits the constructor names of a query in the order in which they are written, in expressions and in patterns, so
/// that a typed query can resolve them and then put the resolved names back in the same order.
fn visit_names(expr: &mut Expr, visit: &mut dyn FnMut(&mut String, crate::diagnostics::Pos)) {
    use syntax::*;
    fn pattern(p: &mut Pattern, visit: &mut dyn FnMut(&mut String, crate::diagnostics::Pos)) {
        let pos = p.pos;
        match &mut p.kind {
            PatternKind::Wild | PatternKind::Bind(_) | PatternKind::Lit(_) => {}
            PatternKind::At(_, inner) => pattern(inner, visit),
            PatternKind::Or(all) => all.iter_mut().for_each(|p| pattern(p, visit)),
            PatternKind::Ctor { name, body } => {
                // The consts of the prelude, as `true` is in a pattern, are compared by their names.
                if !crate::convert::is_const_name(name) {
                    visit(name, pos);
                }
                match body {
                    None => {}
                    Some(PatBody::Record { fields, .. }) => {
                        fields.iter_mut().for_each(|f| pattern(&mut f.pattern, visit))
                    }
                    Some(PatBody::Tuple { args, .. }) => args.iter_mut().for_each(|a| pattern(&mut a.pattern, visit)),
                    Some(PatBody::Array { items, .. }) => items.iter_mut().for_each(|p| pattern(p, visit)),
                }
            }
            PatternKind::Record { fields, .. } => fields.iter_mut().for_each(|f| pattern(&mut f.pattern, visit)),
            PatternKind::Array { items, .. } => items.iter_mut().for_each(|p| pattern(p, visit)),
        }
    }
    let pos = expr.pos;
    match &mut expr.kind {
        ExprKind::Values | ExprKind::Var(_) | ExprKind::Lit(_) => {}
        ExprKind::Ctor { name, body } => {
            visit(name, pos);
            match body {
                None => {}
                Some(CtorBody::Record { fields, base }) => {
                    fields.iter_mut().for_each(|f| visit_names(&mut f.value, visit));
                    if let Some(base) = base {
                        visit_names(base, visit);
                    }
                }
                Some(CtorBody::Tuple(args)) => args.iter_mut().for_each(|a| visit_names(&mut a.value, visit)),
                Some(CtorBody::Array(items)) => items.iter_mut().for_each(|e| visit_names(e, visit)),
            }
        }
        ExprKind::Record { fields, base } => {
            fields.iter_mut().for_each(|f| visit_names(&mut f.value, visit));
            if let Some(base) = base {
                visit_names(base, visit);
            }
        }
        ExprKind::Array(items) => items.iter_mut().for_each(|e| visit_names(e, visit)),
        ExprKind::Field(of, _) | ExprKind::Position(of, _) | ExprKind::Cast(of, _) | ExprKind::Unary(_, of) => {
            visit_names(of, visit)
        }
        ExprKind::Index(of, index) => {
            visit_names(of, visit);
            visit_names(index, visit);
        }
        ExprKind::Method { receiver, args, .. } => {
            visit_names(receiver, visit);
            args.iter_mut().for_each(|a| visit_names(a, visit));
        }
        ExprKind::Closure(closure) => {
            let closure = Rc::make_mut(closure);
            closure.params.iter_mut().for_each(|p| pattern(p, visit));
            visit_names(&mut closure.body, visit);
        }
        ExprKind::Binary(_, a, b) => {
            visit_names(a, visit);
            visit_names(b, visit);
        }
        ExprKind::If { branches, otherwise } => {
            for (cond, then) in branches {
                visit_names(cond, visit);
                visit_names(then, visit);
            }
            visit_names(otherwise, visit);
        }
        ExprKind::Match { scrutinee, arms } => {
            visit_names(scrutinee, visit);
            for arm in arms {
                pattern(&mut arm.pattern, visit);
                if let Some(guard) = &mut arm.guard {
                    visit_names(guard, visit);
                }
                visit_names(&mut arm.body, visit);
            }
        }
        ExprKind::Matches(subject, p) => {
            visit_names(subject, visit);
            pattern(p, visit);
        }
        ExprKind::Assign { target, value, .. } => {
            visit_names(target, visit);
            visit_names(value, visit);
        }
    }
}

/// The constructor names of a query, with their positions, in the order in which they are written.
pub fn constructor_names(expr: &Expr) -> Vec<(String, crate::diagnostics::Pos)> {
    let mut copy = expr.clone();
    let mut names = Vec::new();
    visit_names(&mut copy, &mut |name, pos| names.push((name.clone(), pos)));
    names
}

/// Runs a typed query over inputs that have been checked. The constructor names of the query are replaced by their
/// resolved names first, and each value of the result is passed to `out` with its names shown as they are written in
/// the source that it is in.
pub fn run_typed(
    expr: &Expr,
    inputs: types::QueryInputs,
    out: &mut dyn FnMut(&Value) -> Result<(), std::io::Error>,
) -> Result<(), RunError> {
    let mut expr = expr.clone();
    let mut resolved = inputs.names.into_iter();
    visit_names(&mut expr, &mut |name, _| {
        if let Some(next) = resolved.next() {
            *name = next;
        }
    });
    typed_check::check_typed(&expr, &inputs.types, inputs.target.as_ref()).map_err(RunError::Query)?;
    let typing = Rc::new(eval::Typing {
        types: inputs.types,
        names_source: inputs.names_source,
    });
    let target = inputs.target;
    let source_names: Vec<Rc<str>> = inputs.source_names.iter().map(|n| Rc::from(n.as_str())).collect();
    let items: Vec<Item> = inputs
        .values
        .into_iter()
        .map(|(file, value)| {
            let ty = target.clone().or_else(|| typing.types.type_of(&value));
            Item {
                value,
                src: source_names[file].clone(),
                lit: false,
                ty,
                file,
            }
        })
        .collect();
    let stream: Stream = Box::new(items.into_iter().map(Ok));
    let show = inputs.show;
    eval::with_typing(Some(typing), || {
        run_items(&expr, stream, &mut |item: &Item| {
            let shown = show_names(&item.value, &|name| show(item.file, name));
            out(&shown)
        })
    })
}

/// A value with each of its names shown by `show`.
fn show_names(value: &Value, show: &dyn Fn(&str) -> String) -> Value {
    use crate::ast::{Argument, Member, TaggedBody, ValueKind};
    let all = |items: &[Value]| items.iter().map(|v| show_names(v, show)).collect::<Vec<_>>();
    let members = |ms: &[Member]| {
        ms.iter()
            .map(|m| Member {
                name: m.name.clone(),
                pos: m.pos,
                value: show_names(&m.value, show),
            })
            .collect::<Vec<_>>()
    };
    let kind = match &value.kind {
        ValueKind::Array(items) => ValueKind::Array(all(items)),
        ValueKind::Record(ms) => ValueKind::Record(members(ms)),
        ValueKind::Tagged { name, body } => ValueKind::Tagged {
            name: show(name),
            body: body.as_ref().map(|b| match b {
                TaggedBody::Record(ms) => TaggedBody::Record(members(ms)),
                TaggedBody::Array(items) => TaggedBody::Array(all(items)),
                TaggedBody::Tuple(args) => TaggedBody::Tuple(
                    args.iter()
                        .map(|a| Argument {
                            name: a.name.clone(),
                            pos: a.pos,
                            value: show_names(&a.value, show),
                        })
                        .collect(),
                ),
            }),
        },
        other => other.clone(),
    };
    Value { pos: value.pos, kind }
}
