use std::fs;
use std::path::PathBuf;

pub(super) fn run_bench(
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
        return Err("profiling requires kairos-cli feature `pprof`".to_string());
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
