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

/// What happened to one source during a check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The source parsed and has no validity errors.
    Ok,
    /// The source has a syntax error or at least one validity error.
    Failed,
    /// Checking stopped before the source could be checked.
    NotChecked,
}

/// The result of checking a set of sources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub diagnostics: Vec<Diagnostic>,
    /// One outcome for each source, in the order the sources were given.
    pub outcomes: Vec<Outcome>,
}

impl Report {
    pub fn is_ok(&self) -> bool {
        self.diagnostics.is_empty()
    }
}

/// Checks the syntax and validity of a set of types files, which share one namespace.
///
/// The files are parsed in order and checking stops at the first syntax error, which is the only diagnostic
/// reported. The post-parse checks then only run once every file has parsed, because a file that fails to parse
/// may have contained the declarations that other files refer to, and checking without them would report
/// spurious unknown names. The post-parse checks report every error they find.
pub fn check_sources(sources: &[Source]) -> Report {
    let mut parsed = Vec::new();
    for (index, source) in sources.iter().enumerate() {
        match parser::parse(&source.text) {
            Ok(decls) => parsed.push(ParsedFile {
                name: source.name.clone(),
                decls,
            }),
            Err(e) => {
                let mut diagnostic = Diagnostic::new(&source.name, e.pos, e.message);
                if let Some(name) = e.enum_name {
                    diagnostic = diagnostic.with_note(format!("in enum {}", name));
                }
                let mut outcomes = vec![Outcome::NotChecked; sources.len()];
                outcomes[index] = Outcome::Failed;
                return Report {
                    diagnostics: vec![diagnostic],
                    outcomes,
                };
            }
        }
    }
    let mut outcomes = vec![Outcome::Ok; sources.len()];
    let mut diagnostics = Vec::new();
    for (file, diagnostic) in validate(&parsed) {
        outcomes[file] = Outcome::Failed;
        diagnostics.push(diagnostic);
    }
    Report { diagnostics, outcomes }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prelude_is_valid_on_its_own() {
        assert!(check_sources(&[prelude_source()]).is_ok());
    }

    #[test]
    fn the_prelude_provides_bool_and_option() {
        let user = Source {
            name: "user".to_string(),
            text: "enum Flags = Flags { on: Bool, name: Option<String> };".to_string(),
        };
        assert_eq!(check_sources(&[user]).diagnostics.len(), 2);
        let user = Source {
            name: "user".to_string(),
            text: "enum Flags = Flags { on: Bool, name: Option<String> };".to_string(),
        };
        assert!(check_sources(&[prelude_source(), user]).is_ok());
    }

    #[test]
    fn redeclaring_a_prelude_name_is_a_duplicate() {
        let user = Source {
            name: "user".to_string(),
            text: "enum Bool = Yes, True;".to_string(),
        };
        let errors: Vec<String> = check_sources(&[prelude_source(), user])
            .diagnostics
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

    #[test]
    fn stops_at_the_first_syntax_error_and_names_the_enum() {
        let sources: Vec<Source> = ["enum A = X;", "enum B = Y", "enum C = ;"]
            .iter()
            .enumerate()
            .map(|(i, text)| Source {
                name: format!("f{}", i),
                text: text.to_string(),
            })
            .collect();
        let report = check_sources(&sources);
        assert_eq!(
            report.diagnostics.iter().map(|d| d.to_string()).collect::<Vec<_>>(),
            vec!["ERROR: Expected comma or semi-colon (f1, line 1, col 11); in enum B"]
        );
        assert_eq!(
            report.outcomes,
            vec![Outcome::NotChecked, Outcome::Failed, Outcome::NotChecked]
        );
    }

    #[test]
    fn reports_an_outcome_for_each_source() {
        let sources: Vec<Source> = ["enum A = X;", "enum B = X;", "enum C = Z;"]
            .iter()
            .enumerate()
            .map(|(i, text)| Source {
                name: format!("f{}", i),
                text: text.to_string(),
            })
            .collect();
        let report = check_sources(&sources);
        assert_eq!(report.outcomes, vec![Outcome::Ok, Outcome::Failed, Outcome::Ok]);
    }
}
