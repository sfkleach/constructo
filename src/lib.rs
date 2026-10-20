//! Library for processing Constructo data-types and data-values files.
//!
//! The `constructo` command-line tool is a thin layer over this crate.

pub mod ast;
pub mod diagnostics;
pub mod lexer;
pub mod parser;
pub mod typecheck;
pub mod validate;

use diagnostics::Diagnostic;
use parser::{parse_file, Parsed};
use typecheck::{check_value, Env};
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
    /// Whether the values were checked against the types, which is not so if that was disabled or if the types had
    /// errors.
    pub type_checked: bool,
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
    parse_file(&source.text).1.err().map(|e| syntax_diagnostic(source, e))
}

/// The options that change how sources are checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckOptions {
    /// Check values against the types, rather than only for being well-formed.
    pub type_check: bool,
    /// Let a Nat fit where an Int or Float is expected, and an Int where a Float is expected.
    pub widening: bool,
    /// Carry on with the next top-level value after a type error, instead of stopping.
    pub continue_after_error: bool,
}

impl Default for CheckOptions {
    fn default() -> Self {
        CheckOptions {
            type_check: true,
            widening: true,
            continue_after_error: false,
        }
    }
}

/// Checks a set of sources with the default options. See `check_sources_with`.
pub fn check_sources(sources: &[Source]) -> Report {
    check_sources_with(sources, &CheckOptions::default())
}

/// Checks a set of sources, each of which is either a types file or a values file.
///
/// The kind of each source is decided by its first token. The sources are parsed in order and checking stops at the
/// first syntax error, which is the only diagnostic reported. The post-parse checks then only run once every source has
/// parsed, because a file that fails to parse may have contained the declarations that other files refer to, and
/// checking without them would report spurious unknown names. The types files share one namespace and the post-parse
/// checks report every error they find.
///
/// Unless disabled, the values are then checked against the types. A values file sees only the types files that come
/// before it. That step is skipped if the types have errors, since the declarations cannot be relied on. The first type
/// error stops the run, unless `continue_after_error` is set, in which case there is one error per top-level value.
pub fn check_sources_with(sources: &[Source], options: &CheckOptions) -> Report {
    let mut kinds = vec![None; sources.len()];
    let mut parsed_sources = Vec::new();
    for (index, source) in sources.iter().enumerate() {
        let (kind, parsed) = parse_file(&source.text);
        kinds[index] = Some(kind);
        match parsed {
            Ok(parsed) => parsed_sources.push(parsed),
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
                    type_checked: false,
                };
            }
        }
    }
    let mut outcomes = vec![Outcome::Ok; sources.len()];
    let mut diagnostics = Vec::new();
    // Values files take no part in validation, so each parsed types file is kept with the index of its source.
    let mut indexes = Vec::new();
    let mut files = Vec::new();
    for (index, parsed) in parsed_sources.iter().enumerate() {
        if let Parsed::Types(decls) = parsed {
            indexes.push(index);
            files.push(ParsedFile {
                name: sources[index].name.clone(),
                decls: decls.clone(),
            });
        }
    }
    for (file, diagnostic) in validate(&files) {
        outcomes[indexes[file]] = Outcome::Failed;
        diagnostics.push(diagnostic);
    }
    let type_checked = diagnostics.is_empty() && options.type_check;
    if type_checked {
        type_check_values(sources, &parsed_sources, options, &mut outcomes, &mut diagnostics);
    }
    Report {
        diagnostics,
        outcomes,
        kinds,
        type_checked,
    }
}

fn type_check_values(
    sources: &[Source],
    parsed_sources: &[Parsed],
    options: &CheckOptions,
    outcomes: &mut [Outcome],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut visible = Vec::new();
    let mut stopped = false;
    for (index, parsed) in parsed_sources.iter().enumerate() {
        match parsed {
            Parsed::Types(decls) => visible.extend(decls),
            Parsed::Values(_) if stopped => outcomes[index] = Outcome::NotChecked,
            Parsed::Values(values) => {
                let env = Env::new(visible.iter().copied());
                for value in values {
                    if let Err(e) = check_value(&env, options.widening, value) {
                        diagnostics.push(Diagnostic::new(&sources[index].name, e.pos, e.message));
                        outcomes[index] = Outcome::Failed;
                        if !options.continue_after_error {
                            stopped = true;
                            break;
                        }
                    }
                }
            }
        }
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
    fn syntax_error_accepts_a_well_formed_values_file() {
        let source = Source {
            name: "f".to_string(),
            text: "Circle { radius: 3.0 }; Dot".to_string(),
        };
        assert_eq!(syntax_error(&source), None);
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
        let report = check_sources(&sources(&["enum A = X;", "X; X;", "enum B = Y(A);"]));
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
        let options = CheckOptions {
            type_check: false,
            ..CheckOptions::default()
        };
        assert!(check_sources_with(&sources(&["Circle { radius: 3.0 }"]), &options).is_ok());
    }

    #[test]
    fn values_are_checked_against_the_types_before_them() {
        let report = check_sources(&sources(&["enum A = X;", "Unrelated(1, 2)"]));
        assert_eq!(
            report.diagnostics.iter().map(|d| d.to_string()).collect::<Vec<_>>(),
            vec!["ERROR: Unknown constructor Unrelated (f1, line 1, col 1)"]
        );
        assert_eq!(report.outcomes, vec![Outcome::Ok, Outcome::Failed]);
        // The types come after the values, so they are not in scope.
        assert!(!check_sources(&sources(&["X", "enum A = X;"])).is_ok());
        assert!(check_sources(&sources(&["enum A = X;", "X"])).is_ok());
    }

    #[test]
    fn no_type_check_leaves_only_the_well_formed_check() {
        let options = CheckOptions {
            type_check: false,
            ..CheckOptions::default()
        };
        assert!(check_sources_with(&sources(&["enum A = X;", "Unrelated(1, 2)"]), &options).is_ok());
    }

    #[test]
    fn a_type_error_stops_the_run_unless_continuing() {
        let texts = ["enum A = X;", "Y; Z; X", "W", "enum B = V;"];
        let report = check_sources(&sources(&texts));
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(
            report.outcomes,
            vec![Outcome::Ok, Outcome::Failed, Outcome::NotChecked, Outcome::Ok]
        );
        let options = CheckOptions {
            continue_after_error: true,
            ..CheckOptions::default()
        };
        let report = check_sources_with(&sources(&texts), &options);
        assert_eq!(
            report.diagnostics.iter().map(|d| d.to_string()).collect::<Vec<_>>(),
            vec![
                "ERROR: Unknown constructor Y (f1, line 1, col 1)",
                "ERROR: Unknown constructor Z (f1, line 1, col 4)",
                "ERROR: Unknown constructor W (f2, line 1, col 1)",
            ]
        );
    }

    #[test]
    fn types_with_errors_suppress_the_type_check() {
        let report = check_sources(&sources(&["enum A = X(Missing);", "Nope"]));
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.outcomes, vec![Outcome::Failed, Outcome::Ok]);
    }

    #[test]
    fn reports_a_values_syntax_error_with_the_file_and_location() {
        let report = check_sources(&sources(&["enum A = X;", "X;\nB { x: 1 y: 2 }", "enum C = Z;"]));
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
