use clap::{Parser, Subcommand};
use std::path::PathBuf;

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

        /// The files to check
        #[arg(value_name = "FILES", required_unless_present = "stdin")]
        files: Vec<PathBuf>,
    },
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Check { stdin, files } => {
            println!("check");
            println!("  stdin: {}", stdin);
            for file in &files {
                println!("  file: {}", file.display());
            }
        }
    }
}
