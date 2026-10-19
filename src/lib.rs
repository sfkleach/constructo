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

/// The standard prelude: declarations that are available to every types file unless the prelude is disabled.
pub const PRELUDE: &str = "enum Bool = True, False;\nenum Option<T> = None, Some(T);\n";

/// The name under which the prelude appears in diagnostics.
pub const PRELUDE_NAME: &str = "<prelude>";

/// Returns the prelude as a source, to be checked ahead of the user's inputs.
pub fn prelude_source() -> Source {
    Source {
        name: PRELUDE_NAME.to_string(),
        text: PRELUDE.to_string(),
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prelude_is_valid_on_its_own() {
        assert!(check_sources(&[prelude_source()]).is_empty());
    }

    #[test]
    fn the_prelude_provides_bool_and_option() {
        let user = Source {
            name: "user".to_string(),
            text: "enum Flags = Flags { on: Bool, name: Option<String> };".to_string(),
        };
        assert_eq!(check_sources(&[user]).len(), 2);
        let user = Source {
            name: "user".to_string(),
            text: "enum Flags = Flags { on: Bool, name: Option<String> };".to_string(),
        };
        assert!(check_sources(&[prelude_source(), user]).is_empty());
    }

    #[test]
    fn redeclaring_a_prelude_name_is_a_duplicate() {
        let user = Source {
            name: "user".to_string(),
            text: "enum Bool = Yes, True;".to_string(),
        };
        let errors: Vec<String> = check_sources(&[prelude_source(), user])
            .iter()
            .map(|d| d.to_string())
            .collect();
        assert_eq!(
            errors,
            vec![
                "ERROR: Duplicate type Bool (user, line 1, col 6); first declared at <prelude>, line 1, col 6",
                "ERROR: Duplicate constructor True (user, line 1, col 18); first declared at <prelude>, line 1, col 13",
            ]
        );
    }
}
