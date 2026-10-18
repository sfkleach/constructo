//! Library for processing Constructo data-types and data-values files.
//!
//! The `constructo` command-line tool is a thin layer over this crate.

pub mod ast;
pub mod diagnostics;
pub mod lexer;
pub mod parser;
pub mod validate;

use diagnostics::Diagnostic;
use validate::{validate, ParsedFile};

/// The name and text of one input to be checked.
pub struct Source {
    pub name: String,
    pub text: String,
}

/// Checks the syntax and validity of a set of types files, which share one namespace.
///
/// Each file reports at most one syntax error. The post-parse checks only run when every file
/// parsed, because a file that fails to parse may have contained the declarations that other
/// files refer to, and checking without them would report spurious unknown names.
pub fn check_sources(sources: &[Source]) -> Vec<Diagnostic> {
    let mut parsed = Vec::new();
    let mut errors = Vec::new();
    for source in sources {
        match parser::parse(&source.text) {
            Ok(decls) => parsed.push(ParsedFile {
                name: source.name.clone(),
                decls,
            }),
            Err(e) => errors.push(Diagnostic::new(&source.name, e.pos, e.message)),
        }
    }
    if errors.is_empty() {
        validate(&parsed)
    } else {
        errors
    }
}
