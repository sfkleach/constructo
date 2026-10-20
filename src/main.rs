use clap::{Parser, Subcommand};
use constructo::{check_sources_with, prelude_source, syntax_error, CheckOptions, Outcome, Source, SourceKind};
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
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Check the syntax and type-correctness of Constructo files
    Check {
        /// Also process the standard input as if it were another file
        #[arg(long)]
        stdin: bool,

        /// Do not load the standard prelude (Bool and Option)
        #[arg(long)]
        no_prelude: bool,

        /// Only check that data-values are well-formed, not that they match the data-types
        #[arg(long)]
        no_type_check: bool,

        /// Require numbers to have exactly the expected type: no Nat as an Int, and no Nat or Int as a Float
        #[arg(long)]
        no_widening: bool,

        /// After a type error, carry on with the next value to report more errors
        #[arg(long = "continue")]
        continue_after_error: bool,

        /// Print a message when the check succeeds
        #[arg(long, short)]
        verbose: bool,

        /// The files to check
        #[arg(value_name = "FILES", required_unless_present = "stdin")]
        files: Vec<PathBuf>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Check {
            stdin,
            no_prelude,
            no_type_check,
            no_widening,
            continue_after_error,
            verbose,
            files,
        } => {
            let options = CheckOptions {
                type_check: !no_type_check,
                widening: !no_widening,
                continue_after_error,
            };
            check(stdin, no_prelude, verbose, &options, &files)
        }
    }
}

fn check(stdin: bool, no_prelude: bool, verbose: bool, options: &CheckOptions, files: &[PathBuf]) -> ExitCode {
    // The inputs are read one at a time, and reading stops at the first syntax error. That way an unreadable later
    // input cannot hide an earlier syntax error, which is the error the run is meant to stop at.
    let mut sources = Vec::new();
    let mut syntax_failed = false;
    for file in files {
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
        if syntax_error(sources.last().expect("a source was just pushed")).is_some() {
            syntax_failed = true;
            break;
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
    let report = check_sources_with(&sources, options);
    for diagnostic in &report.diagnostics {
        eprintln!("{}", diagnostic);
    }
    if verbose {
        // The prelude is not one of the user's inputs, so it is never reported.
        for ((source, outcome), kind) in sources
            .iter()
            .zip(&report.outcomes)
            .zip(&report.kinds)
            .skip(first_input)
        {
            let label = kind.map(|k| format!(" ({})", k.label())).unwrap_or_default();
            match outcome {
                Outcome::Ok if report.type_checked && *kind == Some(SourceKind::Values) => {
                    println!("OK: {} (data-values, type-checked)", source.name)
                }
                Outcome::Ok => println!("OK: {}{}", source.name, label),
                Outcome::Failed => println!("FAILED: {}{}", source.name, label),
                Outcome::NotChecked => {}
            }
        }
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
