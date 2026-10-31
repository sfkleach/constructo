use clap::{ArgAction, ArgMatches, CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};
use constructo::diagnostics::{Diagnostic, Pos};
use constructo::{
    check_in_projects, check_sources_with, prelude_source, source_kind, syntax_error, CheckOptions, Config,
    Experimental, Layout, Outcome, Owner, Project, ProjectInfo, ProjectSet, Source, SourceKind,
};
use std::collections::VecDeque;
use std::ffi::OsString;
use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

/// The exit code when the inputs contain errors.
const EXIT_ERRORS: u8 = 1;
/// The exit code for problems with the invocation or with reading the inputs.
const EXIT_USAGE: u8 = 2;

#[derive(Parser, Debug)]
#[command(name = "constructo", version, about = "Tools for Constructo files")]
struct Cli {
    /// Read settings from a TOML file. An option given on the command line overrides the file.
    /// `constructo check --print-config` writes a file to start from
    #[arg(long, global = true, value_name = "PATH")]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

/// The experimental features that can be switched on with `--experimental`.
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum Feature {
    /// Every experimental feature
    All,
    /// The postfix `?` on a value, as in `0?`, which is shorthand for `Some(0)`
    SomeQuery,
}

/// Works out which experimental features have been switched on, given the ones that are on to begin with and the
/// arguments that switch features on or off in the order in which they were written. The last argument to mention a
/// feature wins.
fn experimental(start: Experimental, changes: &[(bool, Feature)]) -> Experimental {
    let mut experimental = start;
    for (on, feature) in changes {
        match feature {
            Feature::All => {
                experimental = if *on {
                    Experimental::all()
                } else {
                    Experimental::default()
                }
            }
            Feature::SomeQuery => experimental.some_query = *on,
        }
    }
    experimental
}

/// The arguments `--experimental` and `--no-experimental` in the order in which they were written, each with whether
/// it switches features on. Clap keeps the values of the two options apart, so the order comes from their indices.
fn feature_changes(matches: &ArgMatches) -> Vec<(bool, Feature)> {
    let mut changes: Vec<(usize, bool, Feature)> = Vec::new();
    for (name, on) in [("experimental", true), ("no_experimental", false)] {
        if let (Some(indices), Some(values)) = (matches.indices_of(name), matches.get_many::<Feature>(name)) {
            changes.extend(indices.zip(values).map(|(index, feature)| (index, on, *feature)));
        }
    }
    changes.sort_by_key(|(index, _, _)| *index);
    changes.into_iter().map(|(_, on, feature)| (on, feature)).collect()
}

/// The value of a setting that has a positive and a negative flag, where `default` is what applies if neither is given,
/// which is the setting from the configuration file or else the built-in default. Giving one of the flags after the
/// other undoes it, which clap arranges with `overrides_with`, so at most one of them is set here.
fn setting(on: bool, off: bool, default: bool) -> bool {
    if on {
        true
    } else if off {
        false
    } else {
        default
    }
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Check the syntax and type-correctness of Constructo files
    #[command(after_help = "\
Settings can also be put in a TOML file that is named with --config=PATH. An option on the command line overrides the \
file, which overrides the built-in default. To write a template that has every setting, with a comment on each, run:

    constructo check --print-config > constructo-settings.toml

The format of the file is described in
https://github.com/sfkleach/constructo/blob/main/docs/specs/config-file.md")]
    Check {
        /// Also process the standard input as if it were another file
        #[arg(long)]
        stdin: bool,

        /// Load the standard prelude (Bool and Option) [default]
        #[arg(long, overrides_with = "no_prelude")]
        prelude: bool,

        /// Do not load the standard prelude (Bool and Option)
        #[arg(long, overrides_with = "prelude")]
        no_prelude: bool,

        /// Check that data-values match the data-types [default]
        #[arg(long, overrides_with = "no_type_check")]
        type_check: bool,

        /// Only check that data-values are well-formed, not that they match the data-types
        #[arg(long, overrides_with = "type_check")]
        no_type_check: bool,

        /// Let a Nat fit where an Int or Float is expected, and an Int where a Float is expected [default]
        #[arg(long, overrides_with = "no_widening")]
        widening: bool,

        /// Require numbers to have exactly the expected type: no Nat as an Int, and no Nat or Int as a Float
        #[arg(long, overrides_with = "widening")]
        no_widening: bool,

        /// After a type error, carry on with the next value to report more errors
        #[arg(long = "continue", overrides_with = "no_continue")]
        continue_after_error: bool,

        /// Stop at the first type error [default]
        #[arg(long = "no-continue", overrides_with = "continue_after_error")]
        no_continue: bool,

        /// Let the definition of a const refer to other consts
        #[arg(long, overrides_with = "no_allow_refs")]
        allow_refs: bool,

        /// Do not let the definition of a const refer to other consts [default]
        #[arg(long, overrides_with = "allow_refs")]
        no_allow_refs: bool,

        /// The most nodes that the type inferred for a const may have [default: 10000]
        #[arg(long, value_name = "N")]
        infer_limit: Option<usize>,

        /// The most nodes that the default value of an omitted argument may have [default: 10000]
        #[arg(long, value_name = "N")]
        default_limit: Option<usize>,

        /// Switch on experimental features. On its own it switches on all of them, and `-x=FEATURE` switches on just
        /// that one. The option can be repeated, and for each feature the last of this and --no-experimental wins.
        #[arg(
            short = 'x',
            long,
            value_name = "FEATURE",
            value_enum,
            num_args = 0..=1,
            require_equals = true,
            default_missing_value = "all",
            action = ArgAction::Append
        )]
        experimental: Vec<Feature>,

        /// Switch off experimental features. On its own it switches off all of them, and `--no-experimental=FEATURE`
        /// switches off just that one. The option can be repeated.
        #[arg(
            long,
            value_name = "FEATURE",
            value_enum,
            num_args = 0..=1,
            require_equals = true,
            default_missing_value = "all",
            action = ArgAction::Append
        )]
        no_experimental: Vec<Feature>,

        /// Print a message when the check succeeds
        #[arg(long, short, overrides_with = "no_verbose")]
        verbose: bool,

        /// Do not print a message when the check succeeds [default]
        #[arg(long, overrides_with = "verbose")]
        no_verbose: bool,

        /// Print the settings that the run would use, after the defaults, --config and the other options, as a
        /// commented TOML file that --config can read, and exit without checking. It is a template for a config file
        #[arg(long)]
        print_config: bool,

        /// Check the project in the folder: the types files that its constructo.toml names come first, then FILES
        #[arg(long, value_name = "FOLDER")]
        project: Option<PathBuf>,

        /// The files to check
        #[arg(value_name = "FILES", required_unless_present_any = ["stdin", "print_config", "project"])]
        files: Vec<PathBuf>,
    },
}

/// How many times `--config` is given in the arguments, wherever it is placed. Nothing after `--` is an option.
fn config_occurrences(args: impl IntoIterator<Item = OsString>) -> usize {
    args.into_iter()
        .take_while(|arg| arg != "--")
        .filter(|arg| {
            let arg = arg.to_string_lossy();
            arg == "--config" || arg.starts_with("--config=")
        })
        .count()
}

/// Reads the configuration file named by `--config`, if there is one.
fn load_config(path: Option<&PathBuf>) -> Result<Config, ExitCode> {
    let Some(path) = path else {
        return Ok(Config::default());
    };
    let text = std::fs::read_to_string(path).map_err(|e| {
        eprintln!("ERROR: Cannot read {}: {}", path.display(), e);
        ExitCode::from(EXIT_USAGE)
    })?;
    Config::parse(&text, &path.display().to_string()).map_err(|diagnostic| {
        eprintln!("{}", diagnostic);
        ExitCode::from(EXIT_USAGE)
    })
}

fn main() -> ExitCode {
    let matches = Cli::command().get_matches();
    let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());
    // Clap keeps only the last value of a global option that is given on both sides of the subcommand, so repeats are
    // counted in the raw arguments.
    if config_occurrences(std::env::args_os().skip(1)) > 1 {
        eprintln!("ERROR: --config can only be given once");
        return ExitCode::from(EXIT_USAGE);
    }
    // The settings come from the built-in defaults, which the configuration file overrides, and the arguments that were
    // given on the command line override both.
    let config = match load_config(cli.config.as_ref()) {
        Ok(config) => config,
        Err(code) => return code,
    };
    match cli.command {
        Command::Check {
            stdin,
            prelude,
            no_prelude,
            type_check,
            no_type_check,
            widening,
            no_widening,
            continue_after_error,
            no_continue,
            allow_refs,
            no_allow_refs,
            infer_limit,
            default_limit,
            experimental: _,
            no_experimental: _,
            verbose,
            no_verbose,
            print_config,
            project,
            files,
        } => {
            // The subcommand is there, since the command line was parsed into it.
            let check_matches = matches
                .subcommand_matches("check")
                .expect("the check subcommand was given");
            let from_file = config.check_options();
            let options = CheckOptions {
                type_check: setting(type_check, no_type_check, from_file.type_check),
                widening: setting(widening, no_widening, from_file.widening),
                continue_after_error: setting(continue_after_error, no_continue, from_file.continue_after_error),
                allow_refs: setting(allow_refs, no_allow_refs, from_file.allow_refs),
                infer_limit: infer_limit.unwrap_or(from_file.infer_limit),
                default_limit: default_limit.unwrap_or(from_file.default_limit),
                experimental: experimental(from_file.experimental, &feature_changes(check_matches)),
            };
            let load_prelude = setting(prelude, no_prelude, config.prelude());
            let verbose = setting(verbose, no_verbose, config.verbose());
            if print_config {
                print!("{}", Config::effective(&options, load_prelude, verbose).to_toml());
                return ExitCode::SUCCESS;
            }
            // The project is read after the settings, so that a problem with either is reported by itself.
            let project = match project.as_deref().map(ProjectSet::load).transpose() {
                Ok(project) => project,
                Err(e) => {
                    eprintln!("{}", e);
                    return ExitCode::from(EXIT_USAGE);
                }
            };
            check(stdin, !load_prelude, verbose, &options, project.as_ref(), &files)
        }
    }
}

/// How the sources are divided among the projects. The prelude, if there is one, is the first source. Then come the
/// files of each project in turn, and these are followed by the files and the standard input that were given, which
/// are written in the project that was named, as are the values.
fn layout(set: &ProjectSet, sources: usize, first_input: usize) -> Layout {
    let projects = set
        .projects
        .iter()
        .map(|project| ProjectInfo {
            name: project.name.clone(),
            imports: project
                .imports
                .iter()
                .map(|import| (import.group.clone(), import.target.expect("the set has been loaded")))
                .collect(),
        })
        .collect();
    let root = set.projects.len() - 1;
    let mut owners = vec![Owner::Prelude; first_input];
    for (index, project) in set.projects.iter().enumerate() {
        owners.extend(std::iter::repeat_n(Owner::Project(index), project.files.len()));
    }
    // A syntax error can end the reading early, and then there are fewer sources than files.
    owners.resize(sources, Owner::Project(root));
    owners.truncate(sources);
    Layout { projects, owners, root }
}

fn check(
    stdin: bool,
    no_prelude: bool,
    verbose: bool,
    options: &CheckOptions,
    project: Option<&ProjectSet>,
    files: &[PathBuf],
) -> ExitCode {
    // The inputs are read one at a time, and reading stops at the first syntax error. That way an unreadable later
    // input cannot hide an earlier syntax error, which is the error the run is meant to stop at. The files of the
    // project come first, and the project says that they are types files.
    let from_project = project.map_or(0, |set| set.projects.iter().map(|project| project.files.len()).sum());
    let inputs: Vec<&PathBuf> = project
        .into_iter()
        .flat_map(|set| set.projects.iter().flat_map(|project| &project.files))
        .chain(files)
        .collect();
    let mut sources = Vec::new();
    let mut syntax_failed = false;
    for (index, file) in inputs.into_iter().enumerate() {
        match std::fs::read_to_string(file) {
            Ok(text) => sources.push(Source {
                name: file.display().to_string(),
                text,
            }),
            Err(e) => {
                eprintln!("ERROR: Cannot read {}: {}", file.display(), e);
                return ExitCode::from(EXIT_USAGE);
            }
        }
        let source = sources.last().expect("a source was just pushed");
        if syntax_error(source, options.experimental).is_some() {
            syntax_failed = true;
            break;
        }
        if index < from_project && source_kind(source, options.experimental) == SourceKind::Values {
            let message = "The project includes this file as a types file but it holds data-values";
            eprintln!("{}", Diagnostic::new(&source.name, Pos { line: 1, col: 1 }, message));
            return ExitCode::from(EXIT_ERRORS);
        }
    }
    if stdin && !syntax_failed {
        let mut text = String::new();
        if let Err(e) = std::io::stdin().read_to_string(&mut text) {
            eprintln!("ERROR: Cannot read standard input: {}", e);
            return ExitCode::from(EXIT_USAGE);
        }
        sources.push(Source {
            name: "<stdin>".to_string(),
            text,
        });
    }

    // The prelude is an ordinary source that is checked ahead of the user's inputs.
    let first_input = usize::from(!no_prelude);
    if !no_prelude {
        sources.insert(0, prelude_source());
    }

    // The processing is deliberately independent of the command line so that it is available as a library.
    let report = match project {
        Some(set) => check_in_projects(&sources, &layout(set, sources.len(), first_input), options),
        None => check_sources_with(&sources, options),
    };
    for diagnostic in &report.diagnostics {
        eprintln!("{}", diagnostic);
    }
    if verbose {
        // Each project is reported just before its files, and it has failed if any one of them has.
        let mut starts = Vec::new();
        let mut start = first_input;
        for project in project.into_iter().flat_map(|set| &set.projects) {
            starts.push((start, project));
            start += project.files.len();
        }
        let mut pending: VecDeque<(usize, &Project)> = starts.into();
        let report_projects = |until: usize, pending: &mut VecDeque<(usize, &Project)>| {
            while let Some(&(start, project)) = pending.front() {
                if start > until {
                    break;
                }
                pending.pop_front();
                let end = (start + project.files.len()).min(report.outcomes.len());
                let failed = report.outcomes[start.min(end)..end].contains(&Outcome::Failed);
                let label = if failed { "FAILED" } else { "OK" };
                println!("{}: {} (project {})", label, project.folder.display(), project.name);
            }
        };
        // The prelude is not one of the user's inputs, so it is never reported.
        for (index, ((source, outcome), kind)) in sources
            .iter()
            .zip(&report.outcomes)
            .zip(&report.kinds)
            .enumerate()
            .skip(first_input)
        {
            report_projects(index, &mut pending);
            let label = kind.map(|k| format!(" ({})", k.label())).unwrap_or_default();
            match outcome {
                Outcome::Ok if report.type_checked && *kind == Some(SourceKind::Values) => {
                    println!("OK: {} (data-values, type-checked)", source.name)
                }
                Outcome::Ok => {
                    println!("OK: {}{}", source.name, label);
                    // The consts are listed with their types, and the types that were inferred are marked.
                    for info in report.consts.iter().filter(|info| info.source == index) {
                        let how = if info.inferred { " (inferred)" } else { "" };
                        println!("  const {}: {}{}", info.name, info.ty, how);
                    }
                }
                Outcome::Failed => println!("FAILED: {}{}", source.name, label),
                Outcome::NotChecked => {}
            }
        }
        report_projects(usize::MAX, &mut pending);
    }
    if report.is_ok() {
        if verbose {
            println!("OK: checked {} file(s)", sources.len() - first_input);
        }
        ExitCode::SUCCESS
    } else {
        ExitCode::from(EXIT_ERRORS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_change_to_a_feature_wins() {
        let none = Experimental::default();
        assert_eq!(experimental(none, &[]), none);
        assert_eq!(experimental(none, &[(true, Feature::SomeQuery)]), Experimental::all());
        assert_eq!(
            experimental(none, &[(true, Feature::SomeQuery), (false, Feature::SomeQuery)]),
            none
        );
        assert_eq!(
            experimental(none, &[(true, Feature::All), (false, Feature::SomeQuery)]),
            none
        );
        assert_eq!(
            experimental(none, &[(false, Feature::All), (true, Feature::SomeQuery)]),
            Experimental::all()
        );
        assert_eq!(
            experimental(none, &[(false, Feature::SomeQuery), (true, Feature::All)]),
            Experimental::all()
        );
    }

    #[test]
    fn the_features_that_the_file_switches_on_are_the_start() {
        let all = Experimental::all();
        assert_eq!(experimental(all, &[]), all);
        assert_eq!(
            experimental(all, &[(false, Feature::SomeQuery)]),
            Experimental::default()
        );
        assert_eq!(experimental(all, &[(false, Feature::All)]), Experimental::default());
        assert_eq!(experimental(all, &[(true, Feature::SomeQuery)]), all);
    }

    #[test]
    fn config_is_counted_on_both_sides_of_the_subcommand() {
        let count = |args: &[&str]| config_occurrences(args.iter().map(OsString::from));
        assert_eq!(count(&["check", "a.types"]), 0);
        assert_eq!(count(&["--config=a.toml", "check", "a.types"]), 1);
        assert_eq!(count(&["check", "--config", "a.toml"]), 1);
        assert_eq!(count(&["--config=a.toml", "check", "--config=b.toml"]), 2);
        assert_eq!(count(&["--config", "a.toml", "check", "--config", "b.toml"]), 2);
        assert_eq!(count(&["check", "--", "--config=a.toml"]), 0);
    }

    #[test]
    fn a_setting_takes_the_flag_that_was_given() {
        assert!(setting(true, false, false));
        assert!(!setting(false, true, true));
        assert!(setting(false, false, true));
        assert!(!setting(false, false, false));
    }
}
