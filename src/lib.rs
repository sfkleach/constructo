//! Library for processing Constructo data-types and data-values files.
//!
//! The `constructo` command-line tool is a thin layer over this crate.

pub mod ast;
pub mod diagnostics;
pub mod lexer;
pub mod parser;
pub mod validate;

use diagnostics::Diagnostic;
use parser::{parse_file, Parsed};
use validate::{validate, ParsedFile};

pub use parser::SourceKind;

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
    /// The kind of each source, or `None` for a source that was not reached because checking stopped earlier.
    pub kinds: Vec<Option<SourceKind>>,
}

impl Report {
    pub fn is_ok(&self) -> bool {
        self.diagnostics.is_empty()
    }
}

fn syntax_diagnostic(source: &Source, e: parser::SyntaxError) -> Diagnostic {
    let diagnostic = Diagnostic::new(&source.name, e.pos, e.message);
    match e.enum_name {
        Some(name) => diagnostic.with_note(format!("in enum {}", name)),
        None => diagnostic,
    }
}

/// Returns the syntax error in a source, if it has one.
///
/// This lets a caller that reads its inputs one at a time stop reading at the first syntax error, before any later
/// input could fail for some unrelated reason.
pub fn syntax_error(source: &Source) -> Option<Diagnostic> {
    parser::parse(&source.text).err().map(|e| syntax_diagnostic(source, e))
}

/// Checks a set of sources, each of which is either a types file or a values file.
///
/// The kind of each source is decided by its first token. The sources are parsed in order and checking stops at the
/// first syntax error, which is the only diagnostic reported. The post-parse checks then only run once every source has
/// parsed, because a file that fails to parse may have contained the declarations that other files refer to, and
/// checking without them would report spurious unknown names. The types files share one namespace and the post-parse
/// checks report every error they find. A values file is only checked for being well-formed: it is not yet checked
/// against the types.
pub fn check_sources(sources: &[Source]) -> Report {
    let mut kinds = vec![None; sources.len()];
    // Each parsed types file is kept with the index of its source, because values files take no part in validation.
    let mut types_files = Vec::new();
    for (index, source) in sources.iter().enumerate() {
        let (kind, parsed) = parse_file(&source.text);
        kinds[index] = Some(kind);
        match parsed {
            Ok(Parsed::Types(decls)) => types_files.push((
                index,
                ParsedFile {
                    name: source.name.clone(),
                    decls,
                },
            )),
            Ok(Parsed::Values(_)) => {}
            Err(e) => {
                let mut outcomes = vec![Outcome::NotChecked; sources.len()];
                outcomes[index] = Outcome::Failed;
                // Only the failing source and the ones before it were reached.
                for later in &mut kinds[index + 1..] {
                    *later = None;
                }
                return Report {
                    diagnostics: vec![syntax_diagnostic(source, e)],
                    outcomes,
                    kinds,
                };
            }
        }
    }
    let mut outcomes = vec![Outcome::Ok; sources.len()];
    let mut diagnostics = Vec::new();
    let (indexes, files): (Vec<usize>, Vec<ParsedFile>) = types_files.into_iter().unzip();
    for (file, diagnostic) in validate(&files) {
        outcomes[indexes[file]] = Outcome::Failed;
        diagnostics.push(diagnostic);
    }
    Report {
        diagnostics,
        outcomes,
        kinds,
    }
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
    fn syntax_error_reports_only_syntax_problems() {
        let source = |text: &str| Source {
            name: "f".to_string(),
            text: text.to_string(),
        };
        assert_eq!(syntax_error(&source("enum A = X(Missing);")), None);
        let e = syntax_error(&source("enum A = X")).unwrap();
        assert_eq!(
            e.to_string(),
            "ERROR: Expected comma or semi-colon (f, line 1, col 11); in enum A"
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

    fn sources(texts: &[&str]) -> Vec<Source> {
        texts
            .iter()
            .enumerate()
            .map(|(i, text)| Source {
                name: format!("f{}", i),
                text: text.to_string(),
            })
            .collect()
    }

    #[test]
    fn checks_types_and_values_files_in_one_run() {
        let report = check_sources(&sources(&["enum A = X;", "A; X { y: 1 };", "enum B = Y(A);"]));
        assert!(report.is_ok());
        assert_eq!(report.outcomes, vec![Outcome::Ok; 3]);
        assert_eq!(
            report.kinds,
            vec![
                Some(SourceKind::Types),
                Some(SourceKind::Values),
                Some(SourceKind::Types)
            ]
        );
    }

    #[test]
    fn a_values_file_is_checked_without_any_types() {
        assert!(check_sources(&sources(&["Circle { radius: 3.0 }"])).is_ok());
    }

    #[test]
    fn values_are_not_yet_checked_against_the_types() {
        assert!(check_sources(&sources(&["enum A = X;", "Unrelated(1, 2)"])).is_ok());
    }

    #[test]
    fn reports_a_values_syntax_error_with_the_file_and_location() {
        let report = check_sources(&sources(&["enum A = X;", "A;\nB { x: 1 y: 2 }", "enum C = Z;"]));
        assert_eq!(
            report.diagnostics.iter().map(|d| d.to_string()).collect::<Vec<_>>(),
            vec!["ERROR: Expected comma or '}' (f1, line 2, col 10)"]
        );
        assert_eq!(
            report.outcomes,
            vec![Outcome::NotChecked, Outcome::Failed, Outcome::NotChecked]
        );
        assert_eq!(
            report.kinds,
            vec![Some(SourceKind::Types), Some(SourceKind::Values), None]
        );
    }

    #[test]
    fn validity_errors_are_attributed_to_the_right_source_when_values_are_mixed_in() {
        let report = check_sources(&sources(&["1; 2", "enum A = X(Missing);"]));
        assert_eq!(report.outcomes, vec![Outcome::Ok, Outcome::Failed]);
    }
}
