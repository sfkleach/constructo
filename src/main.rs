use clap::{ArgAction, ArgGroup, ArgMatches, CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};
use constructo::convert::{
    convert, convert_from_source, convert_from_typed_lines, convert_from_typed_source, convert_jsonl_from_reader,
    convert_lines, values_of, ConvertOptions, Format, FromFormat, LinesError,
};
use constructo::diagnostics::{Diagnostic, Pos};
use constructo::fetch::Cache;
use constructo::lock::Lock;
use constructo::project::{init_project, Init, ProjectError, Remotes, PROJECT_FILE};
use constructo::typed_files::{first_token, types_end, FirstToken};
use constructo::verify::{read_lock, update_lock_with, verify_lock_with};
use constructo::{
    check_in_projects, convert_typed, convert_typed_back, convert_typed_lines, source_kind, syntax_error, CheckOptions,
    Config, Experimental, ImportInfo, InputFile, Layout, Outcome, Project, ProjectInfo, ProjectSet, Source, SourceKind,
};
use std::collections::{HashSet, VecDeque};
use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
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

    /// The folder of the cache of projects from git repositories. Without it, `cache` in the [global] table of the
    /// --config file, then CONSTRUCTO_CACHE, then `constructo` in the cache folder of the user
    #[arg(long, global = true, value_name = "FOLDER")]
    cache: Option<PathBuf>,

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

/// The subcommands of `constructo project`.
#[derive(Subcommand, Debug)]
enum ProjectCommand {
    /// Write a constructo.toml with the defaults, unless the folder has one already
    Init {
        /// The folder of the project [default: the current folder]
        folder: Option<PathBuf>,
    },
    /// Write constructo.lock, which records the checksums of the projects that the project imports, and say what
    /// changed
    Update {
        /// The folder of the project [default: the current folder]
        folder: Option<PathBuf>,
    },
}

/// The ids of the options of `TypingArgs`, which are the names of the options with underscores.
const TYPING_OPTIONS: [&str; 11] = [
    "prelude",
    "no_prelude",
    "widening",
    "no_widening",
    "allow_refs",
    "no_allow_refs",
    "infer_limit",
    "default_limit",
    "dev",
    "no_dev",
    "project",
];

/// The options that decide how the types are found and used, which `check` and `convert` share when `--to` is
/// `json-typed` or `jsonl-typed`.
#[derive(clap::Args, Debug)]
struct TypingArgs {
    /// Load the standard prelude (Bool and Option) [default]
    #[arg(long, overrides_with = "no_prelude")]
    prelude: bool,

    /// Do not load the standard prelude (Bool and Option)
    #[arg(long, overrides_with = "prelude")]
    no_prelude: bool,

    /// Let a Nat fit where an Int or Float is expected, and an Int where a Float is expected [default]
    #[arg(long, overrides_with = "no_widening")]
    widening: bool,

    /// Require numbers to have exactly the expected type: no Nat as an Int, and no Nat or Int as a Float
    #[arg(long, overrides_with = "widening")]
    no_widening: bool,

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

    /// Do not verify the imported projects that are folders on disk against constructo.lock, and do not need
    /// the file for them. For a project that is being developed. Projects from a repository are still verified
    #[arg(long, short, overrides_with = "no_dev")]
    dev: bool,

    /// Verify the imported projects against constructo.lock [default]
    #[arg(long, overrides_with = "dev")]
    no_dev: bool,

    /// Use the project in the folder: the types files that its constructo.toml names come first, then FILES
    #[arg(long, value_name = "FOLDER")]
    project: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Work with a project
    #[command(subcommand)]
    Project(ProjectCommand),

    /// Print the version of constructo, as `constructo --version` does
    Version,

    /// Convert values to another notation
    #[command(
        group(ArgGroup::new("direction").required(true).args(["to", "from"])),
        after_help = "\
For json-exact the first value is converted, and a warning says if more follow. Use --to-array to convert every \
value into an array. For jsonl-exact every value is a line. For xml the document is always a list of all the \
values. A format that is one document is written only if there is no error, and a lines format as it goes. With --from the JSON that --to wrote is read back and the values are \
written as text to the standard output. The settings file is read for [global] and [experimental]."
    )]
    Convert {
        /// The notation to write: json-exact, which keeps every kind of value and needs no types, jsonl-exact, which
        /// is the same with each value on a line of its own, json-typed, which is natural JSON and needs the types, or
        /// xml, which keeps every kind of value as json-exact does and always writes a list of all the values
        #[arg(long, value_name = "FORMAT")]
        to: Option<String>,

        /// The notation to read, writing the values as text: json-exact, jsonl-exact, json-typed or jsonl-typed, as
        /// --to writes them
        #[arg(long, value_name = "FORMAT")]
        from: Option<String>,

        /// With json-typed or jsonl-typed, the type that each value is read as, such as Shape or [Shape], which is
        /// resolved in the project that --project names
        #[arg(long = "type", value_name = "TYPE")]
        type_expr: Option<String>,

        /// With json-exact, convert every value, of every input, into an array, instead of only the first value
        #[arg(long)]
        to_array: bool,

        /// Write the output with no white space
        #[arg(long)]
        compact: bool,

        /// Also read the standard input as if it were another file
        #[arg(long)]
        stdin: bool,

        #[command(flatten)]
        typing: TypingArgs,

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

        /// The files to convert. With --to=json-typed the types files come among them or from --project, and with
        /// --from=json-typed they come first, until the first file that is JSON or a `--`, which ends them
        #[arg(value_name = "FILES", required_unless_present_any = ["stdin", "project"])]
        files: Vec<PathBuf>,
    },

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

        #[command(flatten)]
        typing: TypingArgs,

        /// Check that data-values match the data-types [default]
        #[arg(long, overrides_with = "no_type_check")]
        type_check: bool,

        /// Only check that data-values are well-formed, not that they match the data-types
        #[arg(long, overrides_with = "type_check")]
        no_type_check: bool,

        /// After a type error, carry on with the next value to report more errors
        #[arg(long = "continue", overrides_with = "no_continue")]
        continue_after_error: bool,

        /// Stop at the first type error [default]
        #[arg(long = "no-continue", overrides_with = "continue_after_error")]
        no_continue: bool,

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

/// How many of the `total` files come before the first `--` of the arguments, or `None` if there is no `--`. Every
/// argument after the marker is a file, since nothing after it is an option, so the files after it are counted from the
/// arguments and the others are what is left. Clap's own indices cannot be used, since they count the options and
/// their values and not the places in the arguments, and they do not count the marker. If more arguments follow
/// the marker than there are files, which is when the marker is the value of an option, there is no marker.
fn files_before_marker(args: &[OsString], total: usize) -> Option<usize> {
    let marker = args.iter().position(|arg| arg == "--")?;
    total.checked_sub(args.len() - marker - 1)
}

/// Reads the configuration file named by `--config`, if there is one. A `cache` in it that is relative is made relative
/// to the folder of the file, so that the file means the same wherever it is used from.
fn load_config(path: Option<&PathBuf>) -> Result<Config, ExitCode> {
    let Some(path) = path else {
        return Ok(Config::default());
    };
    let text = std::fs::read_to_string(path).map_err(|e| {
        eprintln!("ERROR: Cannot read {}: {}", path.display(), e);
        ExitCode::from(EXIT_USAGE)
    })?;
    let mut config = Config::parse(&text, &path.display().to_string()).map_err(|diagnostic| {
        eprintln!("{}", diagnostic);
        ExitCode::from(EXIT_USAGE)
    })?;
    if let Some(cache) = &config.global.cache {
        if cache.is_relative() {
            let folder = path.parent().unwrap_or(Path::new(""));
            config.global.cache = Some(folder.join(cache));
        }
    }
    Ok(config)
}

/// The folder of the cache that was given, by `--cache` or by the configuration file, which wins in that order, as an
/// absolute path, so that what `--print-config` writes means the same from any folder. `None` leaves the cache to
/// the environment.
fn given_cache(option: Option<&Path>, config: &Config) -> Result<Option<PathBuf>, ExitCode> {
    let Some(folder) = option.or(config.global.cache.as_deref()) else {
        return Ok(None);
    };
    if folder.as_os_str().is_empty() {
        eprintln!("ERROR: The folder of the cache cannot be empty");
        return Err(ExitCode::from(EXIT_USAGE));
    }
    std::path::absolute(folder).map(Some).map_err(|e| {
        eprintln!("ERROR: Cannot use {} as the cache: {}", folder.display(), e);
        ExitCode::from(EXIT_USAGE)
    })
}

/// The cache that a run uses, as the loader takes it: an error is kept until a project from a repository needs it.
fn cache_for(given: Option<&Path>) -> Result<Cache, String> {
    Cache::resolve(given).map_err(|e| match e {
        ProjectError::Cannot(message) => message,
        other => other.to_string(),
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
    if matches!(cli.command, Command::Version) {
        println!("constructo {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    // The settings come from the built-in defaults, which the configuration file overrides, and the arguments that were
    // given on the command line override both.
    let config = match load_config(cli.config.as_ref()) {
        Ok(config) => config,
        Err(code) => return code,
    };
    let cache = match given_cache(cli.cache.as_deref(), &config) {
        Ok(cache) => cache,
        Err(code) => return code,
    };
    match cli.command {
        // The version is printed before any setting is read, so it is told even where `--config` is wrong.
        Command::Version => unreachable!("the version subcommand is handled before the settings are read"),
        Command::Project(ProjectCommand::Init { folder }) => {
            let folder = folder.unwrap_or_else(|| PathBuf::from("."));
            match init_project(&folder) {
                Ok(Init::Written(name)) => {
                    println!(
                        "Wrote {} for the project {}; change the name to the one that the project should have",
                        folder.join(PROJECT_FILE).display(),
                        name
                    );
                    ExitCode::SUCCESS
                }
                Ok(Init::Exists) => {
                    println!(
                        "{} exists already, so it was not changed",
                        folder.join(PROJECT_FILE).display()
                    );
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{}", e);
                    ExitCode::from(EXIT_USAGE)
                }
            }
        }
        Command::Project(ProjectCommand::Update { folder }) => {
            let folder = folder.unwrap_or_else(|| PathBuf::from("."));
            match update_lock_with(&folder, Remotes::Fetch(cache_for(cache.as_deref()))) {
                Ok(lines) => {
                    for line in lines {
                        println!("{}", line);
                    }
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{}", e);
                    ExitCode::from(EXIT_USAGE)
                }
            }
        }
        Command::Convert {
            to,
            from,
            type_expr,
            to_array,
            compact,
            stdin,
            typing,
            experimental: _,
            no_experimental: _,
            files,
        } => {
            if let Some(from) = from {
                let convert_matches = matches
                    .subcommand_matches("convert")
                    .expect("the convert subcommand was given");
                let back = Back {
                    name: &from,
                    type_expr,
                    compact,
                    stdin,
                    files: &files,
                    matches: convert_matches,
                };
                let format = match FromFormat::from_name(&from) {
                    Ok(format) => format,
                    Err(message) => {
                        eprintln!("ERROR: {}", message);
                        return ExitCode::from(EXIT_USAGE);
                    }
                };
                return if format.needs_types() {
                    convert_back_typed(format, &back, typing, &config, cache.as_deref())
                } else {
                    convert_back(format, &back)
                };
            }
            let to = to.expect("the argument group needs --to or --from");
            if type_expr.is_some() {
                eprintln!("ERROR: --type does not apply with --to, which writes JSON; it names the type to read as");
                return ExitCode::from(EXIT_USAGE);
            }
            let format = match Format::from_name(&to) {
                Ok(format) => format,
                Err(message) => {
                    eprintln!("ERROR: {}", message);
                    return ExitCode::from(EXIT_USAGE);
                }
            };
            // An array is how one document holds many values, and a lines format has no such thing, so asking for both
            // is a mistake that would otherwise be ignored without a word.
            if to_array && format.is_lines() {
                eprintln!(
                    "ERROR: --to-array does not apply to {}, which writes each value on a line of its own",
                    to
                );
                return ExitCode::from(EXIT_USAGE);
            }
            // The document of xml is always a list of the values, so asking for an array as well is a mistake.
            if to_array && format == Format::Xml {
                eprintln!("ERROR: --to-array does not apply to xml, which always writes a list of values");
                return ExitCode::from(EXIT_USAGE);
            }
            let convert_matches = matches
                .subcommand_matches("convert")
                .expect("the convert subcommand was given");
            let from_file = config.check_options();
            let experimental = experimental(from_file.experimental, &feature_changes(convert_matches));
            if !format.needs_types() {
                // The options for the types are refused and not ignored, for the reason that --to-array is.
                for id in TYPING_OPTIONS {
                    if convert_matches.value_source(id) == Some(clap::parser::ValueSource::CommandLine) {
                        eprintln!(
                            "ERROR: --{} does not apply to {}, which needs no types",
                            id.replace('_', "-"),
                            to
                        );
                        return ExitCode::from(EXIT_USAGE);
                    }
                }
                let options = ConvertOptions {
                    format,
                    to_array,
                    compact,
                    experimental,
                };
                return convert_files(stdin, &options, &files);
            }
            let TypingArgs {
                prelude,
                no_prelude,
                widening,
                no_widening,
                allow_refs,
                no_allow_refs,
                infer_limit,
                default_limit,
                dev,
                no_dev,
                project,
            } = typing;
            // The values are always checked, and the run stops at the first type error, so `type_check` and
            // `continue_after_error` are not settings here.
            let options = CheckOptions {
                type_check: true,
                widening: setting(widening, no_widening, from_file.widening),
                continue_after_error: false,
                allow_refs: setting(allow_refs, no_allow_refs, from_file.allow_refs),
                infer_limit: infer_limit.unwrap_or(from_file.infer_limit),
                default_limit: default_limit.unwrap_or(from_file.default_limit),
                experimental,
            };
            let load_prelude = setting(prelude, no_prelude, config.prelude());
            let dev = setting(dev, no_dev, config.dev());
            let set = match load_projects(project.as_deref(), load_prelude, dev, cache.as_deref()) {
                Ok(set) => set,
                Err(code) => return code,
            };
            convert_typed_files(stdin, to_array, compact, format.is_lines(), &options, &set, &files)
        }
        Command::Check {
            stdin,
            typing,
            type_check,
            no_type_check,
            continue_after_error,
            no_continue,
            experimental: _,
            no_experimental: _,
            verbose,
            no_verbose,
            print_config,
            files,
        } => {
            let TypingArgs {
                prelude,
                no_prelude,
                widening,
                no_widening,
                allow_refs,
                no_allow_refs,
                infer_limit,
                default_limit,
                dev,
                no_dev,
                project,
            } = typing;
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
            let dev = setting(dev, no_dev, config.dev());
            if print_config {
                print!(
                    "{}",
                    Config::effective(&options, load_prelude, verbose, dev, cache.as_deref()).to_toml()
                );
                return ExitCode::SUCCESS;
            }
            let set = match load_projects(project.as_deref(), load_prelude, dev, cache.as_deref()) {
                Ok(set) => set,
                Err(code) => return code,
            };
            check(stdin, verbose, dev, &options, &set, &files)
        }
    }
}

/// Loads the project that is named, or the virtual project of the prelude alone if none is, and verifies it against its
/// lock. The error is the exit code, after the problem has been written. The lock is read first, since the projects in
/// repositories are loaded from the commits that it names, and `dev` leaves out the verification of the projects that
/// are folders on disk.
fn load_projects(
    project: Option<&Path>,
    load_prelude: bool,
    dev: bool,
    cache: Option<&Path>,
) -> Result<ProjectSet, ExitCode> {
    // The project is read after the settings, so that a problem with either is reported by itself. There is
    // always a project: without one named it is a virtual project with nothing but the prelude.
    //
    // The lock is read first, since the projects in repositories are loaded from the commits that it names. A
    // run with no project named has no lock. A problem with the lock file itself is a problem with the
    // invocation.
    let lock = match project {
        Some(folder) => read_lock(folder),
        None => Ok(None),
    };
    // A lock that cannot be used is reported at once, unless `--dev` means it is not needed. A project in a
    // repository needs it whatever `--dev` says, and then it is reported when that project is reached.
    if let (Err(e), false) = (&lock, dev) {
        eprintln!("{}", e);
        return Err(ExitCode::from(EXIT_USAGE));
    }
    let set = match project {
        Some(folder) => ProjectSet::load_with(
            folder,
            load_prelude,
            Remotes::Locked {
                cache: cache_for(cache),
                lock: lock.clone(),
            },
        ),
        None => Ok(ProjectSet::unnamed(load_prelude)),
    };
    let set = match set {
        Ok(set) => set,
        Err(e) => {
            eprintln!("{}", e);
            let unlocked = matches!(e, ProjectError::Unverified(_));
            return Err(ExitCode::from(if unlocked { EXIT_ERRORS } else { EXIT_USAGE }));
        }
    };
    // The lock is verified before anything else is read, and only when a project was named. `--dev` leaves
    // out the projects that are folders on disk, and not those from repositories.
    if project.is_some() {
        if let Some(code) = verify_projects(&set, lock.as_ref().ok().and_then(Option::as_ref), dev) {
            return Err(code);
        }
    }
    Ok(set)
}

/// Verifies the projects against the lock file of the root project. The result is the exit code if there is a problem.
fn verify_projects(set: &ProjectSet, lock: Option<&Lock>, dev: bool) -> Option<ExitCode> {
    match verify_lock_with(set, lock, dev) {
        Ok(found) if found.is_empty() => None,
        Ok(found) => {
            for diagnostic in &found {
                eprintln!("{}", diagnostic);
            }
            Some(ExitCode::from(EXIT_ERRORS))
        }
        Err(e) => {
            eprintln!("{}", e);
            Some(ExitCode::from(EXIT_USAGE))
        }
    }
}

/// How the sources are divided among the projects. The files of each project come in turn, and these are followed by
/// the files and the standard input that were given, which are written in the project that was named, as are the
/// values.
fn layout(set: &ProjectSet, sources: usize) -> Layout {
    let projects = set
        .projects
        .iter()
        .map(|project| ProjectInfo {
            name: project.name.clone(),
            imports: project
                .imports
                .iter()
                .map(|import| ImportInfo {
                    group: import.group.clone(),
                    target: import.target.expect("the set has been loaded"),
                    hide: import.hide.clone(),
                    rename: import.rename.clone(),
                    file: project.project_file(),
                })
                .collect(),
            builtin: project.builtin,
        })
        .collect();
    let root = set.projects.len() - 1;
    let mut owners = Vec::new();
    for (index, project) in set.projects.iter().enumerate() {
        owners.extend(std::iter::repeat_n(index, project.files.len()));
    }
    // A syntax error can end the reading early, and then there are fewer sources than files.
    owners.resize(sources, root);
    Layout { projects, owners, root }
}

/// Reads an input: the text of a file of a built-in project is with it, and any other file is read from disk.
fn read_input(file: &InputFile) -> std::io::Result<String> {
    match file.embedded {
        Some(text) => Ok(text.to_string()),
        None => std::fs::read_to_string(&file.path),
    }
}

/// Reads the files and then the standard input, and converts what was read. As with `check`, an input that cannot be
/// read ends the run at once with exit code 2, and reading stops after an input that has an error, so that an
/// unreadable input after it is never reached. A format that writes one document writes nothing unless the conversion
/// is whole, and a lines format writes each line as it goes.
fn convert_files(stdin: bool, options: &ConvertOptions, files: &[PathBuf]) -> ExitCode {
    let mut sources = Vec::new();
    let mut failed = false;
    for path in files {
        match std::fs::read_to_string(path) {
            Ok(text) => sources.push(Source {
                name: path.display().to_string(),
                text,
            }),
            Err(e) => {
                eprintln!("ERROR: Cannot read {}: {}", path.display(), e);
                return ExitCode::from(EXIT_USAGE);
            }
        }
        let source = sources.last().expect("a source was just pushed");
        // The conversion reports this error, as the first one, so it is only noticed here to stop reading.
        if values_of(source, options.experimental).is_err() {
            failed = true;
            break;
        }
    }
    if stdin && !failed {
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
    if options.format.is_lines() {
        return stream_lines(|out| convert_lines(&sources, options, out));
    }
    match convert(&sources, options) {
        Ok(done) => {
            for warning in &done.warnings {
                eprintln!("{}", warning);
            }
            match write_stdout(&done.output) {
                Ok(()) => ExitCode::SUCCESS,
                Err(code) => code,
            }
        }
        Err(diagnostic) => {
            eprintln!("{}", diagnostic);
            ExitCode::from(EXIT_ERRORS)
        }
    }
}

/// What the reading of values back from JSON is given by the command line.
struct Back<'a> {
    /// The name of the format, as it was written.
    name: &'a str,
    type_expr: Option<String>,
    compact: bool,
    stdin: bool,
    files: &'a [PathBuf],
    matches: &'a ArgMatches,
}

/// The first of the options that are refused with `--from` and that were given on the command line, as the error to
/// show. They are refused and not ignored, since they would otherwise do nothing without a word.
fn refused_with_from(matches: &ArgMatches, ids: impl IntoIterator<Item = &'static str>, why: &str) -> Option<String> {
    ids.into_iter()
        .find(|id| matches.value_source(id) == Some(clap::parser::ValueSource::CommandLine))
        .map(|id| format!("--{} does not apply with --from, which {}", id.replace('_', "-"), why))
}

/// Reads one JSON input that was given by a path or on the standard input: the text, or the message that says why not.
fn read_json_text(path: Option<&Path>) -> Result<Source, String> {
    match path {
        Some(path) => match std::fs::read_to_string(path) {
            Ok(text) => Ok(Source {
                name: path.display().to_string(),
                text,
            }),
            Err(e) => Err(format!("Cannot read {}: {}", path.display(), e)),
        },
        None => {
            let mut text = String::new();
            match std::io::stdin().read_to_string(&mut text) {
                Ok(_) => Ok(Source {
                    name: "<stdin>".to_string(),
                    text,
                }),
                Err(e) => Err(format!("Cannot read standard input: {}", e)),
            }
        }
    }
}

/// Reads one input of a lines format by the conversion that is given its name and a reader, which is a file or the
/// standard input if there is no path. The result is the message that says why the input cannot be read, if it cannot,
/// and otherwise nothing.
fn read_lines(
    path: Option<&Path>,
    convert: impl FnOnce(&str, &mut std::io::BufReader<Box<dyn Read>>) -> Result<Option<std::io::Error>, LinesError>,
) -> Result<Option<String>, LinesError> {
    match path {
        Some(path) => {
            let name = path.display().to_string();
            match std::fs::File::open(path) {
                Ok(file) => {
                    let mut reader = std::io::BufReader::new(Box::new(file) as Box<dyn Read>);
                    Ok(convert(&name, &mut reader)?.map(|e| format!("Cannot read {}: {}", name, e)))
                }
                Err(e) => Ok(Some(format!("Cannot read {}: {}", name, e))),
            }
        }
        None => {
            let mut reader = std::io::BufReader::new(Box::new(std::io::stdin().lock()) as Box<dyn Read>);
            Ok(convert("<stdin>", &mut reader)?.map(|e| format!("Cannot read standard input: {}", e)))
        }
    }
}

/// Reads values back from `json-exact` or `jsonl-exact`, with `--from`, and writes them as text to the standard output.
/// The inputs are read and converted one at a time, so that the values of an input are written before a later input is
/// looked at. An input that cannot be read ends the run with exit code 2, after what came before it has been written,
/// and a problem in an input ends it with exit code 1.
fn convert_back(from: FromFormat, back: &Back) -> ExitCode {
    let refused = ["to_array", "experimental", "no_experimental"]
        .into_iter()
        .chain(TYPING_OPTIONS);
    if let Some(message) = refused_with_from(back.matches, refused, "only reads JSON") {
        eprintln!("ERROR: {}", message);
        return ExitCode::from(EXIT_USAGE);
    }
    if back.type_expr.is_some() {
        eprintln!("ERROR: --type does not apply to {}, which needs no types", back.name);
        return ExitCode::from(EXIT_USAGE);
    }
    let mut unreadable = None;
    let code = stream_lines(|out| {
        let inputs = back
            .files
            .iter()
            .map(|path| Some(path.as_path()))
            .chain(back.stdin.then_some(None));
        for input in inputs {
            let result = if from == FromFormat::JsonlExact {
                // The lines are converted as they are read, so that one is not kept back until the end of the input.
                read_lines(input, |name, reader| {
                    convert_jsonl_from_reader(name, reader, back.compact, out)
                })
            } else {
                match read_json_text(input) {
                    Ok(source) => convert_from_source(&source, from, back.compact, out).map(|()| None),
                    Err(message) => Ok(Some(message)),
                }
            };
            if let Some(message) = result? {
                unreadable = Some(message);
                return Ok(());
            }
        }
        Ok(())
    });
    match unreadable {
        Some(message) => {
            eprintln!("ERROR: {}", message);
            ExitCode::from(EXIT_USAGE)
        }
        None => code,
    }
}

/// What the first token of a file says, or why the file cannot be read. Only as much of the file as that is read.
fn first_token_of(path: &Path) -> Result<FirstToken, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("Cannot read {}: {}", path.display(), e))?;
    first_token(&mut std::io::BufReader::new(file)).map_err(|e| format!("Cannot read {}: {}", path.display(), e))
}

/// Reads values back from `json-typed` or `jsonl-typed`, with `--from` and `--type`, and writes them as text to the
/// standard output. The types are those of `--project` and of the files that come first among the files, which end at
/// the first file that does not start a declaration, or at the first `--`. Only the start of a file is read to tell
/// which it is. The rest of the files and the standard input are JSON, and are read one at a time after the types are
/// checked, so the values of an input are written before a later input is looked at.
fn convert_back_typed(
    from: FromFormat,
    back: &Back,
    typing: TypingArgs,
    config: &Config,
    cache: Option<&Path>,
) -> ExitCode {
    let why = "reads JSON as values of a type";
    if let Some(message) = refused_with_from(back.matches, ["to_array", "experimental", "no_experimental"], why) {
        eprintln!("ERROR: {}", message);
        return ExitCode::from(EXIT_USAGE);
    }
    let Some(type_expr) = &back.type_expr else {
        eprintln!("ERROR: --type is required with --from={}", back.name);
        return ExitCode::from(EXIT_USAGE);
    };
    let from_file = config.check_options();
    let options = CheckOptions {
        type_check: true,
        widening: setting(typing.widening, typing.no_widening, from_file.widening),
        continue_after_error: false,
        allow_refs: setting(typing.allow_refs, typing.no_allow_refs, from_file.allow_refs),
        infer_limit: typing.infer_limit.unwrap_or(from_file.infer_limit),
        default_limit: typing.default_limit.unwrap_or(from_file.default_limit),
        experimental: from_file.experimental,
    };
    let load_prelude = setting(typing.prelude, typing.no_prelude, config.prelude());
    let dev = setting(typing.dev, typing.no_dev, config.dev());
    let set = match load_projects(typing.project.as_deref(), load_prelude, dev, cache) {
        Ok(set) => set,
        Err(code) => return code,
    };
    // The types files are the first of the files, up to the `--` or the first file that is not one.
    let arguments: Vec<OsString> = std::env::args_os().collect();
    let marker = files_before_marker(&arguments, back.files.len());
    let end = match types_end(back.files.len(), marker, |index| first_token_of(&back.files[index])) {
        Ok(end) => end,
        Err(message) => {
            eprintln!("ERROR: {}", message);
            return ExitCode::from(EXIT_USAGE);
        }
    };
    let (types_files, json_files) = back.files.split_at(end);
    if json_files.is_empty() && !back.stdin {
        eprintln!("ERROR: There is no JSON to read: give a file after the types files, or --stdin");
        return ExitCode::from(EXIT_USAGE);
    }
    let sources = match read_sources(&set, types_files, false, options.experimental) {
        Ok(sources) => sources,
        Err(code) => return code,
    };
    // A file before the marker is a types file, so one that holds data-values is a mistake that is better said than
    // left to the parser of the types, which would complain of its syntax.
    let given_from = sources.len().saturating_sub(types_files.len());
    for source in &sources[given_from..] {
        if source_kind(source, options.experimental) == SourceKind::Values {
            let message = "This file comes before --, so it is a types file, but it holds data-values";
            eprintln!("{}", Diagnostic::new(&source.name, Pos { line: 1, col: 1 }, message));
            return ExitCode::from(EXIT_ERRORS);
        }
    }
    let mut unreadable = None;
    let layout = layout(&set, sources.len());
    let code = stream_lines(|out| {
        convert_typed_back(&sources, &layout, &options, type_expr, &mut |context| {
            let inputs = json_files
                .iter()
                .map(|path| Some(path.as_path()))
                .chain(back.stdin.then_some(None));
            for input in inputs {
                let result = if from == FromFormat::JsonlTyped {
                    read_lines(input, |name, reader| {
                        convert_from_typed_lines(context, name, reader, options.widening, back.compact, out)
                    })
                } else {
                    match read_json_text(input) {
                        Ok(source) => {
                            convert_from_typed_source(context, &source, from, options.widening, back.compact, out)
                                .map(|()| None)
                        }
                        Err(message) => Ok(Some(message)),
                    }
                };
                if let Some(message) = result? {
                    unreadable = Some(message);
                    return Ok(());
                }
            }
            Ok(())
        })
    });
    match unreadable {
        Some(message) => {
            eprintln!("ERROR: {}", message);
            ExitCode::from(EXIT_USAGE)
        }
        None => code,
    }
}

/// Writes the lines of a lines format to the standard output as they are converted, by the conversion that is given the
/// writer. The lines before an error are written, and are flushed before the errors are reported, so that they come
/// first.
fn stream_lines(convert: impl FnOnce(&mut dyn Write) -> Result<(), LinesError>) -> ExitCode {
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    let result = convert(&mut out);
    match result {
        Ok(()) => match out.flush() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => write_failure(&e),
        },
        Err(LinesError::Invalid(diagnostics)) => {
            // The errors are about the inputs, so a failure to write what came before them is not reported over them.
            let _ = out.flush();
            for diagnostic in &diagnostics {
                eprintln!("{}", diagnostic);
            }
            ExitCode::from(EXIT_ERRORS)
        }
        Err(LinesError::Write(e)) => write_failure(&e),
    }
}

/// Writes the output of a conversion that is one document to the standard output. The error is the exit code.
fn write_stdout(text: &str) -> Result<(), ExitCode> {
    let mut out = std::io::stdout().lock();
    out.write_all(text.as_bytes())
        .and_then(|()| out.flush())
        .map_err(|e| write_failure(&e))
}

/// The exit code when the output could not be written. It is a problem with where the output goes and not with the
/// inputs, so it is 2. A pipe whose reader has gone, as when the output is piped into `head`, is what the reader asked
/// for and gets no message.
fn write_failure(e: &std::io::Error) -> ExitCode {
    if e.kind() != std::io::ErrorKind::BrokenPipe {
        eprintln!("ERROR: Cannot write the output: {}", e);
    }
    ExitCode::from(EXIT_USAGE)
}

/// Reads the inputs of a run: the types files of the projects, then the files that were given, then the standard input.
/// The error is the exit code, after the problem has been written. Reading stops at the first syntax error, which is
/// found later, with the others, by the check.
fn read_sources(
    set: &ProjectSet,
    files: &[PathBuf],
    stdin: bool,
    experimental: Experimental,
) -> Result<Vec<Source>, ExitCode> {
    // The inputs are read one at a time, and reading stops at the first syntax error. That way an unreadable later
    // input cannot hide an earlier syntax error, which is the error the run is meant to stop at. The files of the
    // projects come first, and the project says that they are types files.
    let given: Vec<InputFile> = files
        .iter()
        .map(|path| InputFile {
            path: path.clone(),
            embedded: None,
        })
        .collect();
    let from_projects: Vec<&InputFile> = set.projects.iter().flat_map(|project| &project.files).collect();
    let inputs: Vec<&InputFile> = from_projects.iter().copied().chain(&given).collect();
    let mut sources = Vec::new();
    let mut syntax_failed = false;
    for (index, file) in inputs.into_iter().enumerate() {
        match read_input(file) {
            Ok(text) => sources.push(Source {
                name: file.path.display().to_string(),
                text,
            }),
            Err(e) => {
                eprintln!("ERROR: Cannot read {}: {}", file.path.display(), e);
                return Err(ExitCode::from(EXIT_USAGE));
            }
        }
        let source = sources.last().expect("a source was just pushed");
        if syntax_error(source, experimental).is_some() {
            syntax_failed = true;
            break;
        }
        if index < from_projects.len() && source_kind(source, experimental) == SourceKind::Values {
            let message = "The project includes this file as a types file but it holds data-values";
            eprintln!("{}", Diagnostic::new(&source.name, Pos { line: 1, col: 1 }, message));
            return Err(ExitCode::from(EXIT_ERRORS));
        }
    }
    if stdin && !syntax_failed {
        let mut text = String::new();
        if let Err(e) = std::io::stdin().read_to_string(&mut text) {
            eprintln!("ERROR: Cannot read standard input: {}", e);
            return Err(ExitCode::from(EXIT_USAGE));
        }
        sources.push(Source {
            name: "<stdin>".to_string(),
            text,
        });
    }
    Ok(sources)
}

/// Converts the values to typed JSON. The inputs are those of `check`, read in the same way, and the conversion is of
/// the library, which does the checking. Nothing is written to the standard output unless the conversion is whole.
fn convert_typed_files(
    stdin: bool,
    to_array: bool,
    compact: bool,
    lines: bool,
    options: &CheckOptions,
    set: &ProjectSet,
    files: &[PathBuf],
) -> ExitCode {
    let sources = match read_sources(set, files, stdin, options.experimental) {
        Ok(sources) => sources,
        Err(code) => return code,
    };
    if lines {
        let layout = layout(set, sources.len());
        return stream_lines(|out| convert_typed_lines(&sources, &layout, options, out));
    }
    match convert_typed(&sources, &layout(set, sources.len()), options, to_array, compact) {
        Ok(done) => {
            for warning in &done.warnings {
                eprintln!("{}", warning);
            }
            match write_stdout(&done.output) {
                Ok(()) => ExitCode::SUCCESS,
                Err(code) => code,
            }
        }
        Err(diagnostics) => {
            for diagnostic in &diagnostics {
                eprintln!("{}", diagnostic);
            }
            ExitCode::from(EXIT_ERRORS)
        }
    }
}

fn check(
    stdin: bool,
    verbose: bool,
    dev: bool,
    options: &CheckOptions,
    set: &ProjectSet,
    files: &[PathBuf],
) -> ExitCode {
    let sources = match read_sources(set, files, stdin, options.experimental) {
        Ok(sources) => sources,
        Err(code) => return code,
    };

    // The processing is deliberately independent of the command line so that it is available as a library.
    let report = check_in_projects(&sources, &layout(set, sources.len()), options);
    for diagnostic in &report.diagnostics {
        eprintln!("{}", diagnostic);
    }
    // The files of the projects that are built in are not one of the user's inputs, so they are never reported, and
    // the project with no name is not either.
    let mut hidden = HashSet::new();
    let mut ranges = Vec::new();
    let mut start = 0;
    for (number, project) in set.projects.iter().enumerate() {
        if project.builtin {
            hidden.extend(start..start + project.files.len());
        } else if !project.name.is_empty() {
            ranges.push((start, number, project));
        }
        start += project.files.len();
    }
    let counted = ranges.len();
    if verbose {
        // Each project is reported just before its files, and it has failed if any one of them has, or if its
        // project-file has an error.
        let mut pending: VecDeque<(usize, usize, &Project)> = ranges.into();
        let report_projects = |until: usize, pending: &mut VecDeque<(usize, usize, &Project)>| {
            while let Some(&(start, number, project)) = pending.front() {
                if start > until {
                    break;
                }
                pending.pop_front();
                let end = (start + project.files.len()).min(report.outcomes.len());
                // An error in its project-file fails a project that has no failed file, or no files at all.
                let failed = report.outcomes[start.min(end)..end].contains(&Outcome::Failed)
                    || report.failed_projects.contains(&number);
                let label = if failed { "FAILED" } else { "OK" };
                // The root project is not in the lock. Any other project that is loaded was checked against it, unless
                // that was switched off.
                let checked = if std::ptr::eq(project, set.root()) {
                    ""
                } else if dev && project.remote.is_none() && project.confined.is_none() {
                    // A project from a repository is verified whatever `--dev` says.
                    ", unverified"
                } else {
                    ", verified"
                };
                println!(
                    "{}: {} (project {}{})",
                    label,
                    project.folder.display(),
                    project.name,
                    checked
                );
            }
        };
        for (index, ((source, outcome), kind)) in sources.iter().zip(&report.outcomes).zip(&report.kinds).enumerate() {
            if hidden.contains(&index) {
                continue;
            }
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
            // The projects that are counted are the root and the imported ones, and not the built-in ones. A run with
            // no project named has none to count.
            let files = sources.len() - hidden.len();
            match counted {
                0 => println!("OK: checked {} file(s)", files),
                projects => println!("OK: checked {} file(s) in {} project(s)", files, projects),
            }
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

    #[test]
    fn the_files_before_the_marker_are_those_that_are_not_counted_after_it() {
        let before = |args: &[&str]| {
            let args: Vec<OsString> = args.iter().map(OsString::from).collect();
            let matches = Cli::command().try_get_matches_from(&args).unwrap();
            let convert = matches.subcommand_matches("convert").unwrap();
            let total = convert.get_many::<PathBuf>("files").map_or(0, |files| files.len());
            files_before_marker(&args, total)
        };
        let c = ["constructo", "convert", "--from=json-typed"];
        assert_eq!(before(&[&c[..], &["a", "b"]].concat()), None);
        assert_eq!(before(&[&c[..], &["a", "b", "--", "c"]].concat()), Some(2));
        assert_eq!(before(&[&c[..], &["--", "a", "b"]].concat()), Some(0));
        assert_eq!(before(&[&c[..], &["a", "b", "--"]].concat()), Some(2));
        assert_eq!(before(&[&c[..], &["a", "--compact", "b", "--", "c"]].concat()), Some(2));
        assert_eq!(before(&[&c[..], &["a", "--", "--compact", "c"]].concat()), Some(1));
        let separate = [
            "constructo",
            "--config=x.toml",
            "convert",
            "--from",
            "json-typed",
            "a",
            "--",
            "b",
        ];
        assert_eq!(before(&separate), Some(1));
        assert_eq!(
            before(&["constructo", "convert", "--from=json-typed", "--stdin", "a", "--"]),
            Some(1)
        );
        assert_eq!(
            files_before_marker(&["x".into(), "--".into(), "a".into(), "b".into()], 1),
            None
        );
    }
}
