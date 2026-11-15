//! Queries: Rust-like expressions over `@values`, the lazy stream of the input values, which extract data and
//! transform it. See `docs/decisions/0044-queries-are-rust-like-expressions-over-a-stream-of-values.md` and the task,
//! `docs/tasks/2026-11-15-query-subcommand/query-subcommand.md`, which settles the details.

pub mod check;
pub mod eval;
pub mod lexer;
pub mod number;
pub mod parser;
pub mod syntax;

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
            }
            Err(diagnostic) => {
                failed = true;
                return Some(Err(Failure::Error(diagnostic)));
            }
        }
    }))
}

/// The values of a source. A syntax error is an error, and so is a types file, until queries are typed. A file with
/// nothing in it, which the parser takes for a types file, has no values.
fn values_in(source: &Source, experimental: Experimental) -> Result<Vec<Value>, Diagnostic> {
    match parse_file(&source.text, experimental).1 {
        Err(e) => Err(syntax_diagnostic(source, e)),
        Ok(Parsed::Types(file)) if file.enums.is_empty() && file.consts.is_empty() => Ok(Vec::new()),
        Ok(Parsed::Types(_)) => {
            let message = format!("{} is a types file, and queries are untyped for now", source.name);
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
    let mut values = Some(values);
    let result = eval(expr, &Env::default(), &mut values).map_err(RunError::Failed)?;
    match result {
        Val::One(item) => out(&item.value).map_err(RunError::Write),
        Val::Many(stream) => {
            for item in stream {
                let item = item.map_err(RunError::Failed)?;
                out(&item.value).map_err(RunError::Write)?;
            }
            Ok(())
        }
    }
}

/// Why a run stopped early.
#[derive(Debug)]
pub enum RunError {
    Failed(Failure),
    Write(std::io::Error),
}

#[cfg(test)]
mod tests;
