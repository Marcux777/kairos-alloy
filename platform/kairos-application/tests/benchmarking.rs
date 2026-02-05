use kairos_application::benchmarking::{run_bench, BenchMode};

#[test]
fn run_bench_rejects_invalid_args() {
    assert!(run_bench(0, 60, "engine").is_err());
    assert!(run_bench(10, 0, "engine").is_err());
    assert!(run_bench(10, 60, "nope").is_err());
}

#[test]
fn run_bench_engine_processes_all_bars_and_trades() {
    let out = run_bench(50, 60, "engine").expect("bench engine");
    assert_eq!(out.mode, BenchMode::Engine);
    assert_eq!(out.bars_requested, 50);
    assert_eq!(out.bars_processed, 50);
    assert!(out.results.summary.trades >= 1);
    assert_eq!(out.results.summary.bars_processed, 50);
}

#[test]
fn run_bench_features_does_not_trade_but_processes_bars() {
    let out = run_bench(50, 60, "features").expect("bench features");
    assert_eq!(out.mode, BenchMode::Features);
    assert_eq!(out.bars_processed, 50);
    assert_eq!(out.results.summary.trades, 0);
    assert_eq!(out.results.summary.bars_processed, 50);
}
