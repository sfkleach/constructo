//! Library for processing Constructo data-types and data-values files.
//!
//! The `constructo` command-line tool is a thin layer over this crate.

pub mod ast;
pub mod config;
pub mod convert;
pub mod diagnostics;
pub mod digest;
pub mod exact;
pub mod fetch;
pub mod json;
pub mod lexer;
pub mod lock;
pub mod parser;
pub mod project;
pub mod resolve;
#[cfg(test)]
mod test_support;
pub mod typecheck;
pub mod typed;
pub mod validate;
pub mod values_text;
pub mod verify;

use ast::{ConstDecl, TypesFile};
use convert::Json;
use diagnostics::{Diagnostic, Pos};
use parser::{parse_file, Parsed};
use typecheck::{check_consts, check_value, check_value_typed, Env};
use typed::encode_typed;
use validate::{validate, ParsedFile};

pub use config::Config;
pub use parser::{Experimental, SourceKind};
pub use project::{InputFile, Project, ProjectSet, PRELUDE_PROJECT};
pub use resolve::{ImportInfo, Layout, ProjectInfo};
pub use typecheck::{DEFAULT_INFER_LIMIT, DEFAULT_VALUE_LIMIT};

/// The standard prelude: declarations that are available to every types file unless the prelude is disabled.
pub const PRELUDE: &str = "enum Bool = False, True;
enum Option<T> = None, Some(T);
const null<T>: Option<T> = None;
const true: Bool = True;
const false: Bool = False;
";

/// The name under which the prelude appears in diagnostics.
pub const PRELUDE_NAME: &str = "constructo:/prelude/prelude.types";

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
    /// The type of each const, when the values were type-checked, with the index of the source it was declared in.
    pub consts: Vec<ConstInfo>,
    /// The projects, by index in the layout, that have an error in their project-file which is found in the check, such
    /// as a name that an import hides and the imported project does not declare. These errors are not in a source, so
    /// a project can fail with every one of its files passing, or with no files.
    pub failed_projects: Vec<usize>,
}

/// A const and its type, for the report on what was checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstInfo {
    pub source: usize,
    /// The name, followed by the type parameters if there are any, e.g. `none<T>`.
    pub name: String,
    pub ty: String,
    /// True if the type was inferred from the value, and false if the const declared it.
    pub inferred: bool,
}

impl Report {
    pub fn is_ok(&self) -> bool {
        self.diagnostics.is_empty()
    }
}

pub(crate) fn syntax_diagnostic(source: &Source, e: parser::SyntaxError) -> Diagnostic {
    let diagnostic = Diagnostic::new(&source.name, e.pos, e.message);
    match e.context {
        Some(context) => diagnostic.with_note(format!("in {}", context)),
        None => diagnostic,
    }
}

/// Returns the syntax error in a source, if it has one.
///
/// This lets a caller that reads its inputs one at a time stop reading at the first syntax error, before any later
/// input could fail for some unrelated reason.
pub fn syntax_error(source: &Source, experimental: Experimental) -> Option<Diagnostic> {
    parse_file(&source.text, experimental)
        .1
        .err()
        .map(|e| syntax_diagnostic(source, e))
}

/// Whether the source is a types file or a values file, which its first token decides. A file that fails to parse is
/// still classified by its first token.
pub fn source_kind(source: &Source, experimental: Experimental) -> SourceKind {
    parse_file(&source.text, experimental).0
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
    /// Let the definition of a const refer to other consts.
    pub allow_refs: bool,
    /// The most nodes that the type inferred for a const may have, so that a value with an exponentially large type is
    /// refused instead of taking all the time and memory there is.
    pub infer_limit: usize,
    /// The most nodes that the default value of an omitted argument may have, so that a default that is huge when
    /// written out is refused.
    pub default_limit: usize,
    /// The experimental features of the syntax that are switched on.
    pub experimental: Experimental,
}

impl Default for CheckOptions {
    fn default() -> Self {
        CheckOptions {
            type_check: true,
            widening: true,
            continue_after_error: false,
            allow_refs: false,
            infer_limit: DEFAULT_INFER_LIMIT,
            default_limit: DEFAULT_VALUE_LIMIT,
            experimental: Experimental::default(),
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
/// Every types file must come before every values file, so a types file after a values file is an error. (A file with
/// no declarations is the exception, since it cannot affect anything.) Unless disabled, the values are then checked
/// against the types. That step is skipped if the types have errors, since the declarations cannot be relied on. The
/// first type error stops the run, unless `continue_after_error` is set, in which case there is one error per
/// top-level value.
pub fn check_sources_with(sources: &[Source], options: &CheckOptions) -> Report {
    check_with_layout(sources, None, options, &mut ValueMode::Check)
}

/// Checks a set of sources that belong to projects, as described by the layout. The checks are those of
/// `check_sources_with`, except that a name is resolved in the project that the file belongs to, so that two projects
/// can both declare a `Shape`. A name in a message is shown as it would be written in the file the message is about.
pub fn check_in_projects(sources: &[Source], layout: &Layout, options: &CheckOptions) -> Report {
    check_with_layout(sources, Some(layout), options, &mut ValueMode::Check)
}

/// Converts the first value, or with `to_array` every value, of the values files among the sources to natural JSON,
/// which needs the types. The sources are checked as `check_in_projects` checks them, with the options, except that a
/// value is only checked if it is converted, and the first type error ends the run. The result is the JSON and the
/// warning that more values follow, or the diagnostics, which are the errors of the checks and are never empty.
pub fn convert_typed(
    sources: &[Source],
    layout: &Layout,
    options: &CheckOptions,
    to_array: bool,
    compact: bool,
) -> Result<convert::Converted, Vec<Diagnostic>> {
    let options = CheckOptions {
        type_check: true,
        continue_after_error: false,
        ..*options
    };
    let mut conversion = Conversion {
        to_array,
        converted: Vec::new(),
        left_out: None,
        sink: None,
        failure: None,
    };
    let report = check_with_layout(
        sources,
        Some(layout),
        &options,
        &mut ValueMode::Convert(&mut conversion),
    );
    if !report.diagnostics.is_empty() {
        return Err(report.diagnostics);
    }
    let json = if to_array {
        Json::Array(conversion.converted)
    } else {
        match conversion.converted.into_iter().next() {
            Some(json) => json,
            None => {
                // The first input that has values, or the last input if none has, which is the file that was given.
                let named = report
                    .kinds
                    .iter()
                    .position(|kind| *kind == Some(SourceKind::Values))
                    .or(sources.len().checked_sub(1));
                let name = named.map_or("", |index| sources[index].name.as_str());
                let diagnostic = Diagnostic::new(name, Pos { line: 1, col: 1 }, "No value to convert");
                return Err(vec![diagnostic]);
            }
        }
    };
    let warnings = conversion
        .left_out
        .map(|(index, pos)| {
            let message = "More values follow; only the first was converted, so use --to-array for all";
            Diagnostic::warning(&sources[index].name, pos, message)
        })
        .into_iter()
        .collect();
    Ok(convert::Converted {
        output: json.write(compact),
        warnings,
    })
}

/// Converts every value of the values files among the sources to a line of natural JSON, with no white space, and
/// writes each to `out` as soon as it is converted, as `convert::convert_lines` does for the exact form. The sources are
/// checked as for `convert_typed`, and every value is converted and so every value is type-checked. An error in the
/// types, or in the syntax or the names of any source, is found before anything is written, and the first error in a
/// value ends the run with the lines before it written. No values give no output.
pub fn convert_typed_lines(
    sources: &[Source],
    layout: &Layout,
    options: &CheckOptions,
    out: &mut dyn std::io::Write,
) -> Result<(), convert::LinesError> {
    let options = CheckOptions {
        type_check: true,
        continue_after_error: false,
        ..*options
    };
    let mut conversion = Conversion {
        to_array: true,
        converted: Vec::new(),
        left_out: None,
        sink: Some(out),
        failure: None,
    };
    let report = check_with_layout(
        sources,
        Some(layout),
        &options,
        &mut ValueMode::Convert(&mut conversion),
    );
    if let Some(e) = conversion.failure {
        return Err(convert::LinesError::Write(e));
    }
    if !report.diagnostics.is_empty() {
        return Err(convert::LinesError::Invalid(report.diagnostics));
    }
    Ok(())
}

/// The layout of sources that were given without a project: they make one project that has no name, which imports the
/// prelude into `local` if the prelude is among the sources. The prelude is known by its name.
pub(crate) fn plain_layout(sources: &[Source]) -> Layout {
    let plain = ProjectInfo {
        name: String::new(),
        imports: Vec::new(),
        builtin: false,
    };
    if !sources.iter().any(|source| source.name == PRELUDE_NAME) {
        return Layout {
            projects: vec![plain],
            owners: vec![0; sources.len()],
            root: 0,
        };
    }
    let prelude = ProjectInfo {
        name: PRELUDE_PROJECT.to_string(),
        imports: Vec::new(),
        builtin: true,
    };
    let plain = ProjectInfo {
        imports: vec![ImportInfo::new(Some("local"), 0)],
        ..plain
    };
    Layout {
        projects: vec![prelude, plain],
        owners: sources
            .iter()
            .map(|source| usize::from(source.name != PRELUDE_NAME))
            .collect(),
        root: 1,
    }
}

/// What a run does with the values once the types have been checked.
enum ValueMode<'m, 'w> {
    /// Check every value.
    Check,
    /// Check only the values that are converted, and write them as JSON.
    Convert(&'m mut Conversion<'w>),
}

/// The state of a conversion to typed JSON, as the values are met.
struct Conversion<'w> {
    /// Convert every value, and not only the first.
    to_array: bool,
    /// The JSON of each value that has been converted, in order, if they are being kept.
    converted: Vec<Json>,
    /// The first value that was not converted, with the index of its source, which is where the warning is.
    left_out: Option<(usize, Pos)>,
    /// Where each value is written, as a line, as soon as it is converted, instead of being kept in `converted`.
    sink: Option<&'w mut dyn std::io::Write>,
    /// The failure to write to the sink, which ends the run.
    failure: Option<std::io::Error>,
}

fn check_with_layout(
    sources: &[Source],
    layout: Option<&Layout>,
    options: &CheckOptions,
    mode: &mut ValueMode,
) -> Report {
    let mut kinds = vec![None; sources.len()];
    let mut parsed_sources = Vec::new();
    for (index, source) in sources.iter().enumerate() {
        let (kind, parsed) = parse_file(&source.text, options.experimental);
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
                    consts: Vec::new(),
                    failed_projects: Vec::new(),
                };
            }
        }
    }
    let mut outcomes = vec![Outcome::Ok; sources.len()];
    // Each diagnostic is kept with the index of its source until the end, so that the names in it can be shown.
    let mut found: Vec<(usize, Diagnostic)> = Vec::new();
    for (index, diagnostic) in types_after_values(sources, &parsed_sources) {
        outcomes[index] = Outcome::Failed;
        found.push((index, diagnostic));
    }
    let plain;
    let layout = match layout {
        Some(layout) => layout,
        None => {
            plain = plain_layout(sources);
            &plain
        }
    };
    let resolution = resolve::resolve(sources, &mut parsed_sources, layout);
    // The names in a types file are unreliable if some could not be resolved, so the validity checks are skipped, as
    // they are for a syntax error. The names in the values are only looked at when the values are type-checked.
    let mut unresolved = !resolution.import_errors.is_empty();
    for (index, diagnostic) in &resolution.diagnostics {
        unresolved = true;
        outcomes[*index] = Outcome::Failed;
        found.push((*index, diagnostic.clone()));
    }
    // Values files take no part in validation, so each parsed types file is kept with the index of its source.
    let mut indexes = Vec::new();
    let mut files = Vec::new();
    for (index, parsed) in parsed_sources.iter().enumerate() {
        if let Parsed::Types(file) = parsed {
            indexes.push(index);
            files.push(ParsedFile {
                name: sources[index].name.clone(),
                decls: file.enums.clone(),
                consts: file.consts.clone(),
                prelude: layout.projects[layout.owners.get(index).copied().unwrap_or(layout.root)].builtin,
                visible_types: resolution.visible_types[index].clone(),
            });
        }
    }
    if !unresolved {
        for (file, diagnostic) in validate(&files, options.allow_refs) {
            outcomes[indexes[file]] = Outcome::Failed;
            found.push((indexes[file], diagnostic));
        }
    }
    let type_checked = found.is_empty() && !unresolved && options.type_check;
    let mut consts = Vec::new();
    if type_checked {
        consts = type_check_values(
            sources,
            &parsed_sources,
            &resolution.value_errors,
            options,
            mode,
            &mut outcomes,
            &mut found,
        );
    }
    let file_names: Vec<&str> = sources.iter().map(|source| source.name.as_str()).collect();
    let show = |index: usize, text: &str| resolution.show(index, text, &file_names);
    for info in &mut consts {
        info.name = show(info.source, &info.name);
        info.ty = show(info.source, &info.ty);
    }
    let shown = found.into_iter().map(|(index, mut diagnostic)| {
        diagnostic.message = show(index, &diagnostic.message);
        diagnostic.note = diagnostic.note.map(|note| show(index, &note));
        diagnostic
    });
    let failed_projects: Vec<usize> = resolution.import_errors.iter().map(|(owner, _)| *owner).collect();
    let diagnostics = resolution
        .import_errors
        .iter()
        .map(|(_, diagnostic)| diagnostic.clone())
        .chain(shown)
        .collect();
    Report {
        diagnostics,
        outcomes,
        kinds,
        type_checked,
        consts,
        failed_projects,
    }
}

/// The types files that come after a values file, each with the index of its source and a diagnostic. A types file
/// with no declarations, such as one with only comments, is not reported: it could not change what the values see.
fn types_after_values(sources: &[Source], parsed_sources: &[Parsed]) -> Vec<(usize, Diagnostic)> {
    let mut first_values: Option<&Source> = None;
    let mut late = Vec::new();
    for (index, (source, parsed)) in sources.iter().zip(parsed_sources).enumerate() {
        match (parsed, first_values) {
            (Parsed::Values(_), None) => first_values = Some(source),
            (Parsed::Types(file), Some(values)) if !file.enums.is_empty() || !file.consts.is_empty() => {
                let message = format!(
                    "A types file must come before the values files, but this follows {}",
                    values.name
                );
                late.push((index, Diagnostic::new(&source.name, Pos { line: 1, col: 1 }, message)));
            }
            _ => {}
        }
    }
    late
}

/// Type-checks the consts and then the values, and describes the type of each const.
fn type_check_values(
    sources: &[Source],
    parsed_sources: &[Parsed],
    value_errors: &[(usize, usize, Diagnostic)],
    options: &CheckOptions,
    mode: &mut ValueMode,
    outcomes: &mut [Outcome],
    diagnostics: &mut Vec<(usize, Diagnostic)>,
) -> Vec<ConstInfo> {
    // The declarations of a types file are all in scope in the whole of it, whatever their order, and in every types
    // file, so the consts are checked against all of them.
    let types_files: Vec<(usize, &TypesFile)> = parsed_sources
        .iter()
        .enumerate()
        .filter_map(|(index, parsed)| match parsed {
            Parsed::Types(file) => Some((index, file)),
            Parsed::Values(_) => None,
        })
        .collect();
    let mut everything = Env::new(
        types_files.iter().flat_map(|(_, file)| &file.enums),
        types_files.iter().flat_map(|(_, file)| &file.consts),
    )
    .with_default_limit(options.default_limit);
    let all_consts: Vec<(usize, &ConstDecl)> = types_files
        .iter()
        .flat_map(|&(index, file)| file.consts.iter().map(move |c| (index, c)))
        .collect();
    let errors = check_consts(&mut everything, options.widening, options.infer_limit, &all_consts);
    // The first type error stops the run, unless it was asked to carry on. Every file with an error has failed even if
    // its error is not reported, because the errors have all been found already.
    let mut stopped = false;
    for (index, e) in errors {
        outcomes[index] = Outcome::Failed;
        if !stopped {
            diagnostics.push((index, Diagnostic::new(&sources[index].name, e.pos, e.message)));
            stopped = !options.continue_after_error;
        }
    }
    let infos = describe_consts(&everything, &all_consts);

    // Every types file comes before the values files, or else there would be a diagnostic and no type-check, so the
    // values see all of the declarations.
    let env = Env::new(
        types_files.iter().flat_map(|(_, file)| &file.enums),
        types_files.iter().flat_map(|(_, file)| &file.consts),
    )
    .with_inferred_from(&everything);
    for (index, parsed) in parsed_sources.iter().enumerate() {
        match parsed {
            Parsed::Types(_) => {}
            Parsed::Values(_) if stopped => outcomes[index] = Outcome::NotChecked,
            Parsed::Values(values) => {
                for (position, value) in values.iter().enumerate() {
                    if let ValueMode::Convert(conversion) = mode {
                        // Only the values that are converted are checked. The first one that is not is only noted, for
                        // the warning, and then the run is over.
                        if !conversion.to_array && !conversion.converted.is_empty() {
                            conversion.left_out = Some((index, value.pos));
                            stopped = true;
                            break;
                        }
                    }
                    // A value with a name that could not be resolved is reported for that, and is not type-checked.
                    let unresolved = value_errors
                        .iter()
                        .find(|(source, at, _)| *source == index && *at == position)
                        .map(|(_, _, diagnostic)| diagnostic.clone());
                    let failure = match (unresolved, &mut *mode) {
                        (Some(diagnostic), _) => Some(diagnostic),
                        (None, ValueMode::Check) => check_value(&env, options.widening, value)
                            .err()
                            .map(|e| Diagnostic::new(&sources[index].name, e.pos, e.message)),
                        (None, ValueMode::Convert(conversion)) => {
                            let json = check_value_typed(&env, options.widening, value)
                                .and_then(|typed| encode_typed(&typed, options.widening, options.infer_limit, value));
                            match json {
                                Ok(json) => {
                                    match conversion.sink.as_mut() {
                                        // The value is written as a line now, and not kept.
                                        Some(sink) => {
                                            if let Err(e) = sink.write_all(json.write(true).as_bytes()) {
                                                conversion.failure = Some(e);
                                            }
                                        }
                                        None => conversion.converted.push(json),
                                    }
                                    None
                                }
                                Err(e) => Some(Diagnostic::new(&sources[index].name, e.pos, e.message)),
                            }
                        }
                    };
                    // A failure to write is not an error in the inputs, so there is nothing to report here, and the
                    // run is over.
                    if matches!(mode, ValueMode::Convert(conversion) if conversion.failure.is_some()) {
                        stopped = true;
                        break;
                    }
                    if let Some(diagnostic) = failure {
                        diagnostics.push((index, diagnostic));
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
    infos
}

/// Describes the type of each const that has one: the declared types, and the inferred ones that could be worked out.
fn describe_consts(env: &Env, consts: &[(usize, &ConstDecl)]) -> Vec<ConstInfo> {
    let mut infos = Vec::new();
    for &(source, constant) in consts {
        let (ty, parameters, inferred) = match &constant.ty {
            Some(ty) => {
                let names: Vec<&str> = constant.params.iter().map(|p| p.name.as_str()).collect();
                (ty.to_string(), names.join(", "), false)
            }
            None => match env.inferred_type(&constant.name) {
                Some(scheme) => {
                    let names: Vec<String> = (0..scheme.arity).map(typecheck::gen_name).collect();
                    (scheme.text.clone(), names.join(", "), true)
                }
                None => continue,
            },
        };
        let name = if parameters.is_empty() {
            constant.name.clone()
        } else {
            format!("{}<{}>", constant.name, parameters)
        };
        infos.push(ConstInfo {
            source,
            name,
            ty,
            inferred,
        });
    }
    infos
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
    fn a_prelude_name_that_is_redeclared_is_a_duplicate() {
        let errors = |text: &str| -> Vec<String> {
            let user = Source {
                name: "user".to_string(),
                text: text.to_string(),
            };
            check_sources(&[prelude_source(), user])
                .diagnostics
                .iter()
                .map(|d| d.to_string())
                .collect()
        };
        // The prelude is in the scope of the files given, so a declaration of its names clashes.
        assert_eq!(
            errors("enum Bool = Yes, True;\nconst a: Bool = Yes;"),
            vec![
                concat!(
                    "ERROR: Duplicate type Bool (user, line 1, col 6); ",
                    "first declared at constructo:/prelude/prelude.types, line 1, col 6"
                ),
                concat!(
                    "ERROR: Duplicate constructor True (user, line 1, col 18); ",
                    "first declared at constructo:/prelude/prelude.types, line 1, col 20"
                ),
            ]
        );
    }

    #[test]
    fn syntax_error_reports_only_syntax_problems() {
        let source = |text: &str| Source {
            name: "f".to_string(),
            text: text.to_string(),
        };
        assert_eq!(
            syntax_error(&source("enum A = X(Missing);"), Experimental::default()),
            None
        );
        let e = syntax_error(&source("enum A = X"), Experimental::default()).unwrap();
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
        assert_eq!(syntax_error(&source, Experimental::default()), None);
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
        let report = check_sources(&sources(&["enum A = X;", "enum B = Y(A);", "X; X;"]));
        assert!(report.is_ok());
        assert_eq!(report.outcomes, vec![Outcome::Ok; 3]);
        assert_eq!(
            report.kinds,
            vec![
                Some(SourceKind::Types),
                Some(SourceKind::Types),
                Some(SourceKind::Values)
            ]
        );
    }

    #[test]
    fn a_types_file_after_a_values_file_is_an_error() {
        let report = check_sources(&sources(&["enum A = X;", "X", "enum B = Y;"]));
        assert_eq!(
            messages(&report),
            vec!["ERROR: A types file must come before the values files, but this follows f1 (f2, line 1, col 1)"]
        );
        assert_eq!(report.outcomes, vec![Outcome::Ok, Outcome::Ok, Outcome::Failed]);
        // The error is reported even if the values are not type-checked, and a const counts as a declaration.
        let unchecked = CheckOptions {
            type_check: false,
            ..CheckOptions::default()
        };
        assert!(!check_sources_with(&sources(&["X", "const n: Nat = 1;"]), &unchecked).is_ok());
        // A values file by itself, or after only the types, is fine, and so are several values files together.
        assert!(check_sources(&sources(&["enum A = X;", "X", "[X]"])).is_ok());
    }

    #[test]
    fn a_types_file_without_declarations_may_follow_a_values_file() {
        assert!(check_sources(&sources(&["enum A = X;", "X", "// Only a comment.\n", ""])).is_ok());
    }

    #[test]
    fn every_types_file_after_a_values_file_is_reported() {
        let report = check_sources(&sources(&["1", "enum A = X;", "enum B = Y;"]));
        assert_eq!(messages(&report).len(), 2);
        assert_eq!(report.outcomes, vec![Outcome::Ok, Outcome::Failed, Outcome::Failed]);
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
    fn the_default_of_a_type_can_use_an_enum_that_is_declared_later() {
        let report = check_sources(&sources(&[
            "enum Holder = Holder(x: Later);",
            "enum Later = Ready;",
            "Holder()",
        ]));
        assert!(report.is_ok(), "{:?}", report.diagnostics);
    }

    #[test]
    fn a_postfix_question_is_checked_as_option_and_some() {
        let mut all = vec![prelude_source()];
        all.extend(sources(&[
            "enum Job = Job(String, retries: Nat?, backup: Job?);",
            "Job(\"a\", 3?, Job(\"b\")?); Job(\"c\"); Job(\"d\", retries: null)",
        ]));
        let options = CheckOptions {
            experimental: Experimental::all(),
            ..CheckOptions::default()
        };
        let report = check_sources_with(&all, &options);
        assert!(report.is_ok(), "{:?}", messages(&report));
        // The types are not experimental, but the values are.
        let report = check_sources(&all);
        assert_eq!(
            messages(&report),
            vec!["ERROR: Postfix '?' is experimental; enable it with --experimental=some-query (f1, line 1, col 11)"]
        );
        // A mismatch is described in the same way as for a constructor that is written out.
        let mut wrong = vec![prelude_source()];
        wrong.extend(sources(&[
            "enum Job = Job(String, retries: Nat?);",
            "Job(\"a\", \"x\"?)",
        ]));
        let report = check_sources_with(&wrong, &options);
        assert_eq!(
            messages(&report),
            vec!["ERROR: Expected Nat but found String (f1, line 1, col 10)"]
        );
        // Without the prelude the names that the shorthand stands for are unknown.
        let report = check_sources_with(&sources(&["enum Job = Job(Nat?);"]), &options);
        assert_eq!(
            messages(&report),
            vec!["ERROR: Unknown type Option (f0, line 1, col 16)"]
        );
    }

    #[test]
    fn values_are_checked_against_the_types_before_them() {
        let report = check_sources(&sources(&["enum A = X;", "Unrelated(1, 2)"]));
        assert_eq!(
            report.diagnostics.iter().map(|d| d.to_string()).collect::<Vec<_>>(),
            vec!["ERROR: Unknown constructor Unrelated (f1, line 1, col 1)"]
        );
        assert_eq!(report.outcomes, vec![Outcome::Ok, Outcome::Failed]);
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
        let texts = ["enum A = X;", "enum B = V;", "Y; Z; X", "W"];
        let report = check_sources(&sources(&texts));
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(
            report.outcomes,
            vec![Outcome::Ok, Outcome::Ok, Outcome::Failed, Outcome::NotChecked]
        );
        let options = CheckOptions {
            continue_after_error: true,
            ..CheckOptions::default()
        };
        let report = check_sources_with(&sources(&texts), &options);
        assert_eq!(
            report.diagnostics.iter().map(|d| d.to_string()).collect::<Vec<_>>(),
            vec![
                "ERROR: Unknown constructor Y (f2, line 1, col 1)",
                "ERROR: Unknown constructor Z (f2, line 1, col 4)",
                "ERROR: Unknown constructor W (f3, line 1, col 1)",
            ]
        );
    }

    #[test]
    fn a_name_error_in_a_value_does_not_hide_the_type_errors_of_the_others() {
        let texts = ["enum A = X(Nat);", "X(\"bad\"); missing::X; other::X; X(\"worse\")"];
        let report = check_sources(&sources(&texts));
        assert_eq!(
            report.diagnostics.iter().map(|d| d.to_string()).collect::<Vec<_>>(),
            vec!["ERROR: Expected Nat but found String (f1, line 1, col 3)"]
        );
        let options = CheckOptions {
            continue_after_error: true,
            ..CheckOptions::default()
        };
        let report = check_sources_with(&sources(&texts), &options);
        assert_eq!(
            report.diagnostics.iter().map(|d| d.to_string()).collect::<Vec<_>>(),
            vec![
                "ERROR: Expected Nat but found String (f1, line 1, col 3)",
                "ERROR: Unknown group missing (f1, line 1, col 11)",
                "ERROR: Unknown group other (f1, line 1, col 23)",
                "ERROR: Expected Nat but found String (f1, line 1, col 35)",
            ]
        );
    }

    #[test]
    fn name_errors_in_values_stop_at_the_first_unless_continuing() {
        let texts = ["enum A = X;", "missing::X; other::X"];
        let report = check_sources(&sources(&texts));
        assert_eq!(
            report.diagnostics.iter().map(|d| d.to_string()).collect::<Vec<_>>(),
            vec!["ERROR: Unknown group missing (f1, line 1, col 1)"]
        );
        let options = CheckOptions {
            continue_after_error: true,
            ..CheckOptions::default()
        };
        assert_eq!(check_sources_with(&sources(&texts), &options).diagnostics.len(), 2);
    }

    #[test]
    fn name_errors_in_values_are_ignored_without_the_type_check() {
        let options = CheckOptions {
            type_check: false,
            ..CheckOptions::default()
        };
        assert!(check_sources_with(&sources(&["enum A = X;", "missing::X"]), &options).is_ok());
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

    fn messages(report: &Report) -> Vec<String> {
        report.diagnostics.iter().map(|d| d.to_string()).collect()
    }

    fn with_refs(texts: &[&str]) -> Report {
        let options = CheckOptions {
            allow_refs: true,
            ..CheckOptions::default()
        };
        check_sources_with(&sources(texts), &options)
    }

    const SHAPES: &str = "enum Shape = Circle { r: Float }, Dot;\n";

    #[test]
    fn consts_may_be_declared_in_any_order_among_the_enums() {
        let text = "const origin: Shape = Dot;\nenum Shape = Circle { r: Float }, Dot;\nconst unit: Shape = Circle { r: 1.0 };";
        let report = check_sources(&sources(&[text]));
        assert_eq!(messages(&report), Vec::<String>::new());
        assert_eq!(report.kinds, vec![Some(SourceKind::Types)]);
    }

    #[test]
    fn a_file_may_start_with_a_const() {
        assert!(check_sources(&sources(&["const n: Nat = 1;"])).is_ok());
    }

    #[test]
    fn a_const_name_is_a_name_of_its_own_among_the_values() {
        let report = check_sources(&sources(&["const x: Nat = 1;\nconst x: Nat = 2;"]));
        assert_eq!(
            messages(&report),
            vec!["ERROR: Duplicate const x (f0, line 2, col 7); first declared at line 1, col 7"]
        );
        // A const starts with a lower-case letter and a constructor with a capital, so they cannot clash.
        let report = check_sources(&sources(&["const dev: Nat = 1;", "enum E = Dev, Other;"]));
        assert_eq!(messages(&report), Vec::<String>::new());
    }

    #[test]
    fn a_const_name_starts_with_a_lower_case_letter_or_an_underscore() {
        for name in ["limit", "_limit", "l", "limit2", "limit_two", "limitTwo"] {
            let text = format!("const {}: Nat = 1;", name);
            assert_eq!(
                messages(&check_sources(&sources(&[&text]))),
                Vec::<String>::new(),
                "{}",
                name
            );
        }
        let report = check_sources(&sources(&["const Limit: Nat = 1;"]));
        assert_eq!(
            messages(&report),
            vec!["ERROR: Expected const name that starts with a lower-case letter (f0, line 1, col 7)"]
        );
    }

    #[test]
    fn a_const_may_share_its_spelling_with_an_enum_type_apart_from_the_first_letter() {
        let text = "enum Config = Settings { port: Nat };\nconst config: Config = Settings { port: 80 };";
        assert_eq!(messages(&check_sources(&sources(&[text]))), Vec::<String>::new());
    }

    #[test]
    fn a_const_cannot_be_named_after_a_basic_type() {
        // The basic types start with a capital, which a const name cannot.
        let report = check_sources(&sources(&["const Nat: Nat = 1;"]));
        assert_eq!(
            messages(&report),
            vec!["ERROR: Expected const name that starts with a lower-case letter (f0, line 1, col 7)"]
        );
    }

    #[test]
    fn the_type_of_a_const_must_be_known() {
        let report = check_sources(&sources(&[
            "const n: Missing = 1;",
            "enum L<T> = Nil, C(T);\nconst m: L = Nil;",
        ]));
        assert_eq!(
            messages(&report),
            vec![
                "ERROR: Unknown type Missing (f0, line 1, col 10)",
                "ERROR: Type L takes 1 type argument(s) but 0 supplied (f1, line 2, col 10)",
            ]
        );
        // A type parameter is not in scope in a const.
        assert_eq!(check_sources(&sources(&["const n: T = 1;"])).diagnostics.len(), 1);
    }

    #[test]
    fn the_value_of_a_const_is_checked_against_its_type() {
        let text = format!(
            "{}const a: Shape = Circle {{ r: 1 }};\nconst b: Nat = \"s\";\nconst c: Shape = Square;",
            SHAPES
        );
        let report = check_sources_with(
            &sources(&[&text]),
            &CheckOptions {
                continue_after_error: true,
                ..CheckOptions::default()
            },
        );
        assert_eq!(
            messages(&report),
            vec![
                "ERROR: Expected Nat but found String (f0, line 3, col 16)",
                "ERROR: Unknown constructor Square (f0, line 4, col 18)",
            ]
        );
        assert_eq!(report.outcomes, vec![Outcome::Failed]);
        // The run stops at the first type error unless it is told to continue.
        assert_eq!(check_sources(&sources(&[&text])).diagnostics.len(), 1);
    }

    #[test]
    fn a_const_value_is_not_type_checked_under_no_type_check() {
        let options = CheckOptions {
            type_check: false,
            ..CheckOptions::default()
        };
        assert!(check_sources_with(&sources(&["const n: Nat = \"s\";"]), &options).is_ok());
    }

    #[test]
    fn a_values_file_may_refer_to_consts() {
        let types = format!(
            "{}const origin: Shape = Dot;\nconst limit: Nat = 10;\nconst names: [String] = [\"a\"];",
            SHAPES
        );
        let ok = "Circle { r: limit }; origin; [origin, Dot]";
        assert_eq!(messages(&check_sources(&sources(&[&types, ok]))), Vec::<String>::new());
        let report = check_sources(&sources(&[&types, "Circle { r: origin }"]));
        assert_eq!(
            messages(&report),
            vec!["ERROR: Expected Float but found const origin of type Shape (f1, line 1, col 13)"]
        );
        // A const takes no body, and a name that starts with a lower-case letter is always a const, so this is a
        // syntax error and not a type error.
        let report = check_sources(&sources(&[&types, "origin { r: 1.0 }"]));
        assert_eq!(
            messages(&report),
            vec!["ERROR: Expected semi-colon (f1, line 1, col 8)"]
        );
        // And a name that starts with a capital is a constructor, whatever consts there are.
        let report = check_sources(&sources(&[&types, "Origin"]));
        assert_eq!(
            messages(&report),
            vec!["ERROR: Unknown constructor Origin (f1, line 1, col 1)"]
        );
        let report = check_sources(&sources(&[&types, "missing"]));
        assert_eq!(
            messages(&report),
            vec!["ERROR: Unknown const missing (f1, line 1, col 1)"]
        );
    }

    #[test]
    fn a_const_reference_has_the_declared_type_with_widening() {
        let types = "enum Option<T> = None, Some(T);\nconst n: Nat = 1;\nconst x: Float = 1.5;\nconst o: Option<Nat> = Some(1);";
        assert!(check_sources(&sources(&[types, "[n, x]; [x, n]; Some(n)"])).is_ok());
        let report = check_sources(&sources(&[types, "[o, None]; [o, Some(2.5)]"]));
        assert_eq!(report.diagnostics.len(), 1);
        let strict = CheckOptions {
            widening: false,
            ..CheckOptions::default()
        };
        assert_eq!(
            check_sources_with(&sources(&[types, "[n, x]"]), &strict)
                .diagnostics
                .len(),
            1
        );
    }

    #[test]
    fn a_const_may_not_refer_to_a_const_without_allow_refs() {
        let text = "const a: Nat = 1;\nconst b: [Nat] = [0, a];";
        assert_eq!(
            messages(&check_sources(&sources(&[text]))),
            vec!["ERROR: Reference to const a in const b requires --allow-refs (f0, line 2, col 22)"]
        );
        assert_eq!(messages(&with_refs(&[text])), Vec::<String>::new());
    }

    #[test]
    fn consts_may_refer_to_consts_in_any_order_across_files() {
        let report = with_refs(&["const b: Nat = a;\nconst c: Nat = b;", "const a: Nat = 3;"]);
        assert_eq!(messages(&report), Vec::<String>::new());
    }

    #[test]
    fn a_reference_is_type_checked_against_the_declared_type() {
        let report = with_refs(&["const a: String = \"s\";\nconst b: Nat = a;"]);
        assert_eq!(
            messages(&report),
            vec!["ERROR: Expected Nat but found const a of type String (f0, line 2, col 16)"]
        );
    }

    #[test]
    fn circular_consts_are_reported_once_each() {
        let report = with_refs(&["const a: Nat = b;\nconst b: Nat = c;\nconst c: Nat = a;\nconst d: Nat = d;"]);
        assert_eq!(
            messages(&report),
            vec![
                "ERROR: Circular const definition a -> b -> c -> a (f0, line 3, col 16)",
                "ERROR: Circular const definition d -> d (f0, line 4, col 16)",
            ]
        );
        // The cycle is reported without any type-check, so it is found under --no-type-check as well.
        let options = CheckOptions {
            allow_refs: true,
            type_check: false,
            ..CheckOptions::default()
        };
        assert_eq!(
            check_sources_with(&sources(&["const a: Nat = a;"]), &options)
                .diagnostics
                .len(),
            1
        );
    }

    #[test]
    fn a_long_chain_of_consts_does_not_overflow_the_stack() {
        let text: String = (0..20_000)
            .map(|i| format!("const c{}: Nat = c{};\n", i, i + 1))
            .chain(std::iter::once("const c20000: Nat = 1;\n".to_string()))
            .collect();
        assert!(with_refs(&[&text]).is_ok());
    }

    #[test]
    fn a_const_value_is_attributed_to_its_own_file() {
        let report = check_sources(&sources(&["enum A = X;", "const n: Nat = \"s\";"]));
        assert_eq!(report.outcomes, vec![Outcome::Ok, Outcome::Failed]);
    }

    #[test]
    fn a_failed_const_stops_the_values_that_follow() {
        let report = check_sources(&sources(&["const n: Nat = \"s\";", "1"]));
        assert_eq!(report.outcomes, vec![Outcome::Failed, Outcome::NotChecked]);
    }

    #[test]
    fn a_syntax_error_in_a_const_names_it() {
        let report = check_sources(&sources(&["const n: Nat = 1;\nconst m: Nat = 1 2;"]));
        assert_eq!(
            messages(&report),
            vec!["ERROR: Expected semi-colon (f0, line 2, col 18); in const m"]
        );
    }

    fn with_prelude(texts: &[&str]) -> Report {
        let mut all = vec![prelude_source()];
        all.extend(sources(texts));
        check_sources(&all)
    }

    #[test]
    fn the_prelude_provides_null_true_and_false() {
        let report = with_prelude(&["null; true; [false, true]; [null, Some(1)]; Some(null)"]);
        assert_eq!(messages(&report), Vec::<String>::new());
        let report = with_prelude(&["Some(true); Some(null); [None, null, Some(false)]"]);
        assert_eq!(messages(&report), Vec::<String>::new());
        assert_eq!(with_prelude(&["[true, 1]"]).diagnostics.len(), 1);
        assert_eq!(with_prelude(&["[Some(true), Some(1)]"]).diagnostics.len(), 1);
    }

    #[test]
    fn without_the_prelude_null_true_and_false_are_unknown() {
        let report = check_sources(&sources(&["null"]));
        assert_eq!(messages(&report), vec!["ERROR: Unknown const null (f0, line 1, col 1)"]);
        assert_eq!(check_sources(&sources(&["true"])).diagnostics.len(), 1);
    }

    #[test]
    fn a_prelude_const_that_is_redefined_is_a_duplicate() {
        let report = with_prelude(&["const null: Nat = 1;\nconst x: Nat = null;"]);
        assert_eq!(
            messages(&report),
            vec![concat!(
                "ERROR: Duplicate const null (f0, line 1, col 7); ",
                "first declared at constructo:/prelude/prelude.types, line 3, col 7"
            )]
        );
    }

    #[test]
    fn a_const_may_refer_to_a_prelude_const_without_allow_refs() {
        let text = "const flag: Bool = true;\nconst missing: Option<String> = null;\nconst some2: [Option<Nat>] = [null, Some(1)];";
        assert_eq!(messages(&with_prelude(&[text])), Vec::<String>::new());
        // Other consts still need the option, even when a prelude const is referred to as well.
        let report = with_prelude(&["const a: Nat = 1;\nconst b: [Nat] = [a];\nconst c: Option<Nat> = null;"]);
        assert_eq!(
            messages(&report),
            vec!["ERROR: Reference to const a in const b requires --allow-refs (f0, line 2, col 19)"]
        );
        // And the reference is checked against the type of the prelude const.
        let report = with_prelude(&["const n: Nat = null;"]);
        assert_eq!(
            messages(&report),
            vec!["ERROR: Expected Nat but found const null of type Option<_> (f0, line 1, col 16)"]
        );
    }

    #[test]
    fn checks_the_type_parameters_of_a_const() {
        let report = check_sources(&sources(&[
            "enum B = Y;\nconst a<T, T>: [T] = [];\nconst b<U>: Nat = 1;\nconst c<_U>: Nat = 1;\nconst d<B>: [B] = [];\nconst e<Int>: [Int] = [];",
        ]));
        assert_eq!(
            messages(&report),
            vec![
                "ERROR: Duplicate type parameter T (f0, line 2, col 12)",
                "ERROR: Unused type parameter U (f0, line 3, col 9)",
                "ERROR: Type parameter B clashes with a declared type (f0, line 5, col 9)",
                "ERROR: Type parameter Int clashes with a basic type (f0, line 6, col 9)",
            ]
        );
        // The parameters of a const are in scope in its type only.
        assert_eq!(
            check_sources(&sources(&["const a<T>: [T] = [];\nconst b: T = 1;"]))
                .diagnostics
                .len(),
            1
        );
        assert_eq!(
            check_sources(&sources(&["const a<T>: [T] = [];\nenum E = X(T);"]))
                .diagnostics
                .len(),
            1
        );
    }

    #[test]
    fn a_generic_const_is_checked_for_every_type() {
        let text = "enum Option<T> = None, Some(T);\nconst bad<T>: Option<T> = Some(1);";
        assert_eq!(
            messages(&check_sources(&sources(&[text]))),
            vec!["ERROR: Expected T but found Nat (f0, line 2, col 32)"]
        );
        let text = "enum Option<T> = None, Some(T);\nconst none<T>: Option<T> = None;\nconst a: Option<Nat> = none;\nconst b: Option<String> = none;";
        // Referring to one const from another needs --allow-refs, as for any other const.
        assert_eq!(messages(&with_refs(&[text])), Vec::<String>::new());
    }

    #[test]
    fn every_file_with_a_const_error_has_failed_even_when_only_the_first_error_is_reported() {
        let bad = |name: &str| format!("const {}: Nat = \"bad\";", name);
        let texts = [bad("a"), "enum E = X;".to_string(), bad("b"), "1".to_string()];
        let texts: Vec<&str> = texts.iter().map(String::as_str).collect();
        let report = check_sources(&sources(&texts));
        assert_eq!(
            messages(&report),
            vec!["ERROR: Expected Nat but found String (f0, line 1, col 16)"]
        );
        assert_eq!(
            report.outcomes,
            vec![Outcome::Failed, Outcome::Ok, Outcome::Failed, Outcome::NotChecked]
        );
        // With --continue the error of each file is reported as well.
        let options = CheckOptions {
            continue_after_error: true,
            ..CheckOptions::default()
        };
        let report = check_sources_with(&sources(&texts), &options);
        assert_eq!(report.diagnostics.len(), 2);
        assert_eq!(
            report.outcomes,
            vec![Outcome::Failed, Outcome::Ok, Outcome::Failed, Outcome::Ok]
        );
    }

    /// A writer that keeps what it is given in the chunks that it was given, and can be made to fail.
    #[derive(Default)]
    struct Sink {
        chunks: Vec<String>,
        fail_after: Option<usize>,
    }

    impl std::io::Write for Sink {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.fail_after == Some(self.chunks.len()) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "the reader has gone",
                ));
            }
            self.chunks.push(String::from_utf8(bytes.to_vec()).unwrap());
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// Converts the values to typed lines, with the prelude, giving what was written and the result.
    fn typed_lines(
        types: &str,
        values: &str,
        fail_after: Option<usize>,
    ) -> (Vec<String>, Result<(), convert::LinesError>) {
        let sources = vec![
            prelude_source(),
            Source {
                name: "types".to_string(),
                text: types.to_string(),
            },
            Source {
                name: "values".to_string(),
                text: values.to_string(),
            },
        ];
        let mut sink = Sink {
            fail_after,
            ..Sink::default()
        };
        let layout = plain_layout(&sources);
        let result = convert_typed_lines(&sources, &layout, &CheckOptions::default(), &mut sink);
        (sink.chunks, result)
    }

    const LINE_SHAPES: &str = "enum Shape = Circle { r: Float }, Dot;";

    #[test]
    fn typed_lines_are_written_one_at_a_time_in_the_form_of_json_typed() {
        let (chunks, result) = typed_lines(LINE_SHAPES, "Circle { r: 1 }; Dot; [1, 2.5]; true", None);
        assert!(result.is_ok());
        assert_eq!(
            chunks,
            ["{\"Circle\":{\"r\":1.0}}\n", "\"Dot\"\n", "[1.0,2.5]\n", "true\n"]
        );
    }

    #[test]
    fn typed_lines_before_an_error_in_a_value_have_been_written() {
        let (chunks, result) = typed_lines(LINE_SHAPES, "Dot; Circle { r: \"x\" }; Dot", None);
        let Err(convert::LinesError::Invalid(diagnostics)) = result else {
            panic!("a type error was expected");
        };
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0]
            .to_string()
            .starts_with("ERROR: Expected Float but found String"));
        assert_eq!(chunks, ["\"Dot\"\n"]);
    }

    #[test]
    fn an_error_in_the_types_is_found_before_anything_is_written() {
        let (chunks, result) = typed_lines("enum Bad = Bad(Missing);", "5; 6", None);
        assert!(matches!(result, Err(convert::LinesError::Invalid(_))));
        assert!(chunks.is_empty());
    }

    #[test]
    fn typed_lines_with_no_values_are_no_output() {
        let (chunks, result) = typed_lines(LINE_SHAPES, "", None);
        assert!(result.is_ok());
        assert!(chunks.is_empty());
    }

    #[test]
    fn a_failure_to_write_a_typed_line_ends_the_run_and_is_reported_as_one() {
        let (chunks, result) = typed_lines(LINE_SHAPES, "Dot; Dot; Dot", Some(1));
        let Err(convert::LinesError::Write(e)) = result else {
            panic!("a failure to write was expected");
        };
        assert_eq!(e.kind(), std::io::ErrorKind::BrokenPipe);
        assert_eq!(chunks.len(), 1);
    }
}
