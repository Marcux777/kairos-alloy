use clap::{Parser, ValueEnum};
use kairos_alloy::headless::{HeadlessArgs, HeadlessMode};
use kairos_alloy::{logging, TuiOpts};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(name = "kairos-alloy")]
#[command(about = "Kairos Alloy TUI + optional headless runner.", version)]
struct Cli {
    /// Run without TUI and exit after the selected mode completes.
    #[arg(long)]
    headless: bool,

    /// Headless mode: validate | backtest | paper | report | sweep | cpcv
    #[arg(long)]
    mode: Option<Mode>,

    /// Config file path (TOML). If omitted, uses env KAIROS_CONFIG.
    #[arg(long)]
    config: Option<PathBuf>,

    /// Enable strict validation limits (validate mode only).
    #[arg(long)]
    strict: bool,

    /// Input run directory for report regeneration (report mode only).
    #[arg(long)]
    run_dir: Option<PathBuf>,

    /// Sweep config file (sweep mode only).
    #[arg(long)]
    sweep_config: Option<PathBuf>,

    /// Output path for CPCV folds CSV (cpcv mode only).
    #[arg(long)]
    cpcv_out: Option<PathBuf>,

    /// Number of contiguous groups to split the time series into (cpcv mode only).
    #[arg(long, default_value_t = 6)]
    cpcv_n_groups: usize,

    /// Number of groups held out for testing in each fold (cpcv mode only).
    #[arg(long, default_value_t = 2)]
    cpcv_k_test: usize,

    /// Label/lookahead horizon (in bars) used for purge calculations (cpcv mode only).
    #[arg(long, default_value_t = 1)]
    cpcv_horizon_bars: usize,

    /// Extra purge bars before each test segment (cpcv mode only).
    #[arg(long, default_value_t = 0)]
    cpcv_purge_bars: usize,

    /// Embargo bars after each test segment (cpcv mode only).
    #[arg(long, default_value_t = 0)]
    cpcv_embargo_bars: usize,

    /// Optional start timestamp filter (epoch seconds or RFC3339, inclusive) (cpcv mode only).
    #[arg(long)]
    cpcv_start: Option<String>,

    /// Optional end timestamp filter (epoch seconds or RFC3339, inclusive) (cpcv mode only).
    #[arg(long)]
    cpcv_end: Option<String>,
}

#[derive(ValueEnum, Debug, Clone, Copy)]
enum Mode {
    Validate,
    Backtest,
    Paper,
    Report,
    Sweep,
    Cpcv,
}

fn main() {
    let cli = Cli::parse();

    let log_store = Arc::new(parking_lot::Mutex::new(logging::LogStore::new(5000)));
    if let Err(err) = init_tracing(log_store.clone()) {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
    if let Err(err) = init_metrics() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }

    if cli.headless {
        let mode = match cli.mode {
            Some(m) => m,
            None => {
                eprintln!("error: --mode is required with --headless");
                std::process::exit(1);
            }
        };

        let mode = match mode {
            Mode::Validate => HeadlessMode::Validate,
            Mode::Backtest => HeadlessMode::Backtest,
            Mode::Paper => HeadlessMode::Paper,
            Mode::Report => HeadlessMode::Report,
            Mode::Sweep => HeadlessMode::Sweep,
            Mode::Cpcv => HeadlessMode::Cpcv,
        };

        let config_path = match mode {
            HeadlessMode::Sweep => cli.config.or_else(|| {
                std::env::var("KAIROS_CONFIG")
                    .ok()
                    .filter(|v| !v.trim().is_empty())
                    .map(PathBuf::from)
            }),
            _ => Some(
                cli.config
                    .or_else(|| {
                        std::env::var("KAIROS_CONFIG")
                            .ok()
                            .filter(|v| !v.trim().is_empty())
                            .map(PathBuf::from)
                    })
                    .unwrap_or_else(|| {
                        eprintln!("error: missing --config and env KAIROS_CONFIG is not set");
                        std::process::exit(1);
                    }),
            ),
        };

        let result = kairos_alloy::headless::run_headless(HeadlessArgs {
            mode,
            config_path,
            strict: cli.strict,
            run_dir: cli.run_dir,
            sweep_config: cli.sweep_config,
            cpcv_out: cli.cpcv_out,
            cpcv_n_groups: cli.cpcv_n_groups,
            cpcv_k_test: cli.cpcv_k_test,
            cpcv_horizon_bars: cli.cpcv_horizon_bars,
            cpcv_purge_bars: cli.cpcv_purge_bars,
            cpcv_embargo_bars: cli.cpcv_embargo_bars,
            cpcv_start: cli.cpcv_start,
            cpcv_end: cli.cpcv_end,
        });

        match result {
            Ok(json) => {
                println!(
                    "{}",
                    serde_json::to_string(&json)
                        .unwrap_or_else(|_| "{\"status\":\"error\",\"error\":\"json\"}".to_string())
                );
                std::process::exit(0);
            }
            Err(err) => {
                let lower = err.to_lowercase();
                let code = if lower.contains("strict validation failed") {
                    2
                } else {
                    1
                };
                eprintln!("error: {err}");
                std::process::exit(code);
            }
        }
    }

    let initial_config_path = cli.config.or_else(|| {
        std::env::var("KAIROS_CONFIG")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .map(PathBuf::from)
    });
    let initial_config_path =
        match kairos_alloy::bootstrap::prepare_tui_startup(initial_config_path) {
            Ok(path) => Some(path),
            Err(err) => {
                eprintln!("error: {err}");
                std::process::exit(1);
            }
        };

    let opts = TuiOpts {
        initial_config_path,
        log_store,
        default_out_dir: PathBuf::from("runs"),
    };

    if let Err(err) = kairos_alloy::run(opts) {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn init_tracing(log_store: Arc<parking_lot::Mutex<logging::LogStore>>) -> Result<(), String> {
    let filter = std::env::var("KAIROS_LOG").unwrap_or_else(|_| "info".to_string());
    let env_filter = tracing_subscriber::EnvFilter::try_new(filter)
        .map_err(|err| format!("invalid log filter: {err}"))?;

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_writer(logging::LogMakeWriter::new(log_store))
        .init();

    Ok(())
}

#[cfg(feature = "prometheus")]
fn init_metrics() -> Result<Option<SocketAddr>, String> {
    use metrics_exporter_prometheus::PrometheusBuilder;

    let Some(raw) = std::env::var("KAIROS_METRICS_ADDR").ok() else {
        return Ok(None);
    };
    if raw.trim().is_empty() {
        return Ok(None);
    }

    let addr: SocketAddr = raw
        .parse()
        .map_err(|err| format!("invalid KAIROS_METRICS_ADDR (expected host:port): {err}"))?;

    PrometheusBuilder::new()
        .with_http_listener(addr)
        .install()
        .map_err(|err| format!("failed to install prometheus exporter: {err}"))?;

    tracing::info!(metrics_addr = %addr, "prometheus metrics exporter enabled");
    Ok(Some(addr))
}

#[cfg(not(feature = "prometheus"))]
fn init_metrics() -> Result<Option<SocketAddr>, String> {
    Ok(None)
}
