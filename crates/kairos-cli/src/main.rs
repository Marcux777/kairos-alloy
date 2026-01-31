mod commands;
mod config;
mod infra;
mod obs;
mod output;

use clap::{CommandFactory, Parser, Subcommand};
use commands::Command;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "kairos-alloy")]
#[command(about = "Kairos Alloy CLI", version)]
#[command(
    after_help = "Examples:\n  kairos-alloy backtest --config configs/sample.toml --out runs/\n  kairos-alloy paper --config configs/sample.toml --out runs/\n  kairos-alloy validate --config configs/sample.toml\n  kairos-alloy report --input runs/<run_id>/\n"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<CliCommand>,

    /// Print build metadata (version, commit, toolchain) and exit.
    #[arg(long, default_value_t = false)]
    build_info: bool,

    /// Runtime log filter (EnvFilter). Also supports KAIROS_LOG env var.
    #[arg(long, default_value = "info")]
    log_level: String,

    /// Log format: pretty | json
    #[arg(long, default_value = "pretty")]
    log_format: String,

    /// Prometheus metrics listen addr (e.g. 127.0.0.1:9898). Optional.
    #[arg(long)]
    metrics_addr: Option<String>,
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

        /// Write a CPU profile as an SVG flamegraph to this path (requires feature `pprof`).
        #[arg(long)]
        profile_svg: Option<PathBuf>,
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
    let cli = Cli::parse();

    if cli.build_info {
        print_build_info();
        return;
    }

    if let Err(err) = obs::init_tracing(&cli.log_level, &cli.log_format) {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
    if let Err(err) = obs::init_metrics(cli.metrics_addr.as_deref()) {
        eprintln!("error: {err}");
        std::process::exit(1);
    }

    let command = match cli.command {
        Some(command) => command,
        None => {
            output::print_banner();
            let mut cmd = Cli::command();
            let _ = cmd.print_help();
            println!();
            return;
        }
    };

    output::print_banner();
    let command = match command {
        CliCommand::Backtest { config, out } => Command::Backtest { config, out },
        CliCommand::Bench {
            bars,
            step_seconds,
            mode,
            json,
            profile_svg,
        } => Command::Bench {
            bars,
            step_seconds,
            mode,
            json,
            profile_svg,
        },
        CliCommand::Paper { config, out } => Command::Paper { config, out },
        CliCommand::Validate {
            config,
            strict,
            out,
        } => Command::Validate {
            config,
            strict,
            out,
        },
        CliCommand::Report { input } => Command::Report { input },
    };

    metrics::counter!(
        "kairos.cli.command.invocations",
        "command" => match &command {
            Command::Backtest { .. } => "backtest",
            Command::Bench { .. } => "bench",
            Command::Paper { .. } => "paper",
            Command::Validate { .. } => "validate",
            Command::Report { .. } => "report",
        }
    )
    .increment(1);

    if let Err(err) = commands::run(command) {
        eprintln!("error: {}", err);
        std::process::exit(1);
    }
}

fn print_build_info() {
    let version = env!("CARGO_PKG_VERSION");
    let git_sha = option_env!("KAIROS_GIT_SHA").unwrap_or("unknown");
    let build_unix_epoch =
        option_env!("KAIROS_BUILD_UNIX_EPOCH").and_then(|v| v.parse::<u64>().ok());
    let rustc = option_env!("KAIROS_RUSTC_VERSION").unwrap_or("unknown");
    let target = option_env!("KAIROS_TARGET").unwrap_or("unknown");

    let info = serde_json::json!({
        "name": "kairos-alloy",
        "version": version,
        "git_sha": git_sha,
        "build_unix_epoch": build_unix_epoch,
        "rustc": rustc,
        "target": target,
    });

    println!("{}", info);
}
