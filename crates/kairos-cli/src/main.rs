mod commands;
mod config;
mod output;

use clap::{Parser, Subcommand};
use commands::Command;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "kairos-alloy")]
#[command(about = "Kairos Alloy CLI", version, arg_required_else_help = true)]
#[command(
    after_help = "Examples:\n  kairos-alloy backtest --config configs/sample.toml --out runs/\n  kairos-alloy paper --config configs/sample.toml --out runs/\n  kairos-alloy validate --config configs/sample.toml\n  kairos-alloy report --input runs/<run_id>/\n"
)]
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
    Bench {
        /// Number of synthetic bars to generate (default: 500_000).
        #[arg(long, default_value_t = 500_000)]
        bars: usize,
        /// Timeframe step in seconds for timestamps (default: 60).
        #[arg(long, default_value_t = 60)]
        step_seconds: i64,
        /// Benchmark mode: engine (baseline strategy) or features (feature pipeline + HOLD).
        #[arg(long, default_value = "features")]
        mode: String,
        /// Print a single JSON line instead of human output.
        #[arg(long, default_value_t = false)]
        json: bool,
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
        #[arg(long, default_value_t = false)]
        strict: bool,
        #[arg(long)]
        out: Option<PathBuf>,
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
        CliCommand::Bench {
            bars,
            step_seconds,
            mode,
            json,
        } => Command::Bench {
            bars,
            step_seconds,
            mode,
            json,
        },
        CliCommand::Paper { config, out } => Command::Paper { config, out },
        CliCommand::Validate { config, strict, out } => Command::Validate {
            config,
            strict,
            out,
        },
        CliCommand::Report { input } => Command::Report { input },
    };

    if let Err(err) = commands::run(command) {
        eprintln!("error: {}", err);
        std::process::exit(1);
    }
}
