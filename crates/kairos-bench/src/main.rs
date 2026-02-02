use clap::Parser;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "kairos-bench")]
#[command(about = "Synthetic benchmark tool for Kairos Alloy (dev)")]
struct Args {
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

    /// Prometheus metrics listen addr (e.g. 127.0.0.1:9898). Optional.
    #[arg(long)]
    metrics_addr: Option<String>,

    /// Write a CPU profile as an SVG flamegraph to this path (requires feature `pprof`).
    #[arg(long)]
    profile_svg: Option<PathBuf>,
}

fn main() {
    let args = Args::parse();

    if let Err(err) = init_tracing() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
    if let Err(err) = init_metrics(args.metrics_addr.as_deref()) {
        eprintln!("error: {err}");
        std::process::exit(1);
    }

    if let Err(err) = run_bench(
        args.bars,
        args.step_seconds,
        args.mode,
        args.json,
        args.profile_svg,
    ) {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn init_tracing() -> Result<(), String> {
    let filter = std::env::var("KAIROS_LOG").unwrap_or_else(|_| "info".to_string());
    let env_filter = tracing_subscriber::EnvFilter::try_new(filter)
        .map_err(|err| format!("invalid log filter: {err}"))?;
    tracing_subscriber::fmt().with_env_filter(env_filter).init();
    Ok(())
}

#[cfg(feature = "prometheus")]
fn init_metrics(metrics_addr: Option<&str>) -> Result<Option<SocketAddr>, String> {
    use metrics_exporter_prometheus::PrometheusBuilder;

    let Some(raw) = metrics_addr else {
        return Ok(None);
    };
    let addr: SocketAddr = raw
        .parse()
        .map_err(|err| format!("invalid --metrics-addr (expected host:port): {err}"))?;

    PrometheusBuilder::new()
        .with_http_listener(addr)
        .install()
        .map_err(|err| format!("failed to install prometheus exporter: {err}"))?;

    tracing::info!(metrics_addr = %addr, "prometheus metrics exporter enabled");
    Ok(Some(addr))
}

#[cfg(not(feature = "prometheus"))]
fn init_metrics(metrics_addr: Option<&str>) -> Result<Option<SocketAddr>, String> {
    if metrics_addr.is_some() {
        return Err("metrics exporter requires kairos-bench feature `prometheus`".to_string());
    }
    Ok(None)
}

fn run_bench(
    bars: usize,
    step_seconds: i64,
    mode: String,
    json: bool,
    profile_svg: Option<PathBuf>,
) -> Result<(), String> {
    let mode_label = mode.trim().to_lowercase();

    #[cfg(feature = "pprof")]
    let guard = if let Some(path) = &profile_svg {
        let _ = fs::create_dir_all(
            path.parent()
                .ok_or_else(|| "profile_svg path has no parent".to_string())?,
        );
        Some(
            pprof::ProfilerGuard::new(100)
                .map_err(|err| format!("failed to start profiler: {err}"))?,
        )
    } else {
        None
    };

    #[cfg(not(feature = "pprof"))]
    if profile_svg.is_some() {
        return Err("profiling requires kairos-bench feature `pprof`".to_string());
    }

    let bench = kairos_application::benchmarking::run_bench(bars, step_seconds, &mode_label)?;
    let elapsed_ms = bench.elapsed_ms;
    let bars_processed = bench.bars_processed;
    let bars_per_sec = bench.bars_per_sec;
    let results = bench.results;

    metrics::histogram!("kairos.bench.elapsed_ms", "mode" => mode_label.clone())
        .record(elapsed_ms as f64);
    metrics::gauge!("kairos.bench.bars_per_sec", "mode" => mode_label.clone()).set(bars_per_sec);
    metrics::gauge!("kairos.bench.bars_processed", "mode" => mode_label.clone())
        .set(bars_processed as f64);

    #[cfg(feature = "pprof")]
    if let (Some(guard), Some(path)) = (guard, &profile_svg) {
        let report = guard
            .report()
            .build()
            .map_err(|err| format!("failed to build profile report: {err}"))?;
        let file = std::fs::File::create(path)
            .map_err(|err| format!("failed to create {}: {err}", path.display()))?;
        report
            .flamegraph(file)
            .map_err(|err| format!("failed to write flamegraph: {err}"))?;
        tracing::info!(profile_svg = %path.display(), "wrote cpu profile flamegraph");
    }

    if json {
        let line = serde_json::json!({
            "mode": match bench.mode {
                kairos_application::benchmarking::BenchMode::Engine => "engine",
                kairos_application::benchmarking::BenchMode::Features => "features",
            },
            "bars_requested": bench.bars_requested,
            "bars_processed": bars_processed,
            "elapsed_ms": elapsed_ms,
            "bars_per_sec": bars_per_sec,
            "size_mode": "qty",
        });
        println!("{}", line);
    } else {
        println!(
            "bench: mode={} bars={} elapsed_ms={} bars_per_sec={:.2}",
            match bench.mode {
                kairos_application::benchmarking::BenchMode::Engine => "engine",
                kairos_application::benchmarking::BenchMode::Features => "features",
            },
            bars_processed,
            elapsed_ms,
            bars_per_sec
        );
        println!(
            "bench: trades={} net_profit={:.4} sharpe={:.4} max_drawdown={:.4}",
            results.summary.trades,
            results.summary.net_profit,
            results.summary.sharpe,
            results.summary.max_drawdown
        );
    }

    Ok(())
}
