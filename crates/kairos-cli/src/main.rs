mod commands;
mod config;
mod output;

use clap::{Parser, Subcommand};
use commands::Command;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "kairos-alloy")]
#[command(about = "Kairos Alloy CLI", version)]
struct Cli {
    #[command(subcommand)]
    command: CliCommand,
}

#[derive(Subcommand)]
enum CliCommand {
    Backtest {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        out: Option<PathBuf>,
    },
    Paper {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        out: Option<PathBuf>,
    },
    Validate {
        #[arg(long)]
        config: PathBuf,
    },
    Report {
        #[arg(long)]
        input: PathBuf,
    },
}

fn main() {
    output::print_banner();
    let cli = Cli::parse();
    let command = match cli.command {
        CliCommand::Backtest { config, out } => Command::Backtest { config, out },
        CliCommand::Paper { config, out } => Command::Paper { config, out },
        CliCommand::Validate { config } => Command::Validate { config },
        CliCommand::Report { input } => Command::Report { input },
    };

    if let Err(err) = commands::run(command) {
        eprintln!("error: {}", err);
        std::process::exit(1);
    }
}
