use clap::{Parser, Subcommand};
use constructo::{check_sources, Source};
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
        Command::Check { stdin, verbose, files } => check(stdin, verbose, &files),
    }
}

fn check(stdin: bool, verbose: bool, files: &[PathBuf]) -> ExitCode {
    let mut sources = Vec::new();
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
    }
    if stdin {
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

    // The processing is deliberately independent of the command line so that it is available as a library.
    let diagnostics = check_sources(&sources);
    for diagnostic in &diagnostics {
        eprintln!("{}", diagnostic);
    }
    if diagnostics.is_empty() {
        if verbose {
            println!("OK: checked {} input(s)", sources.len());
        }
        ExitCode::SUCCESS
    } else {
        ExitCode::from(EXIT_ERRORS)
    }
}
