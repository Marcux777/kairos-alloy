pub(super) fn run_bench(
    bars: usize,
    step_seconds: i64,
    mode: String,
    json: bool,
) -> Result<(), String> {
    let bench = kairos_application::benchmarking::run_bench(bars, step_seconds, &mode)?;
    let elapsed_ms = bench.elapsed_ms;
    let bars_processed = bench.bars_processed;
    let bars_per_sec = bench.bars_per_sec;
    let results = bench.results;

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
