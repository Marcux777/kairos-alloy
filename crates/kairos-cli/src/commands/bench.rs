use super::features;
use super::BacktestRunner;
use super::BuyAndHold;
use super::MetricsConfig;
use super::RiskLimits;
use super::VecBarSource;
use std::time::Instant;

#[derive(Clone, Copy)]
enum BenchMode {
    Engine,
    Features,
}

pub(super) fn run_bench(
    bars: usize,
    step_seconds: i64,
    mode: String,
    json: bool,
) -> Result<(), String> {
    if bars == 0 {
        return Err("--bars must be > 0".to_string());
    }
    if step_seconds <= 0 {
        return Err("--step-seconds must be > 0".to_string());
    }

    let bench_mode = match mode.trim().to_lowercase().as_str() {
        "engine" => BenchMode::Engine,
        "features" => BenchMode::Features,
        _ => return Err("unsupported --mode (use: engine | features)".to_string()),
    };

    let start_ts = 1_700_000_000i64;
    let symbol = "BENCH";

    let mut synthetic = Vec::with_capacity(bars);
    let mut price = 100.0f64;
    for i in 0..bars {
        let t = start_ts + (i as i64) * step_seconds;
        // Deterministic, non-random "market" dynamics.
        let drift = ((i as f64) * 0.000001).sin() * 0.05;
        let shock = ((i as f64) * 0.001).cos() * 0.01;
        let ret = drift + shock;
        let open = price;
        let close = (price * (1.0 + ret)).max(0.01);
        let high = open.max(close) * 1.001;
        let low = open.min(close) * 0.999;
        let volume = 1000.0 + ((i as f64) * 0.01).sin().abs() * 100.0;
        synthetic.push(kairos_core::types::Bar {
            symbol: symbol.to_string(),
            timestamp: t,
            open,
            high,
            low,
            close,
            volume,
        });
        price = close;
    }

    let data = VecBarSource::new(synthetic);

    let metrics_config = MetricsConfig::default();
    let risk_limits = RiskLimits {
        max_position_qty: 0.0,
        max_drawdown_pct: 1.0,
        max_exposure_pct: 1.0,
    };

    let run_id = format!(
        "bench_{}_{}",
        match bench_mode {
            BenchMode::Engine => "engine",
            BenchMode::Features => "features",
        },
        bars
    );
    let size_mode = kairos_core::backtest::OrderSizeMode::Quantity;

    let start = Instant::now();
    let results = match bench_mode {
        BenchMode::Engine => {
            let strategy = BuyAndHold::new(1.0);
            let mut runner = BacktestRunner::new(
                run_id.clone(),
                strategy,
                data,
                risk_limits,
                10_000.0,
                metrics_config,
                0.0,
                0.0,
                symbol.to_string(),
                size_mode,
            );
            runner.run()
        }
        BenchMode::Features => {
            struct FeatureBenchStrategy {
                builder: features::FeatureBuilder,
            }

            impl kairos_core::strategy::Strategy for FeatureBenchStrategy {
                fn name(&self) -> &str {
                    "feature_bench_hold"
                }

                fn on_bar(
                    &mut self,
                    bar: &kairos_core::types::Bar,
                    _portfolio: &kairos_core::portfolio::Portfolio,
                ) -> kairos_core::types::Action {
                    let _obs = self.builder.update(bar, None);
                    kairos_core::types::Action::hold()
                }
            }

            let feature_config = features::FeatureConfig {
                return_mode: features::ReturnMode::Log,
                sma_windows: vec![10, 50],
                volatility_windows: vec![10],
                rsi_enabled: false,
            };
            let builder = features::FeatureBuilder::new(feature_config);
            let strategy = FeatureBenchStrategy { builder };
            let mut runner = BacktestRunner::new(
                run_id.clone(),
                strategy,
                data,
                risk_limits,
                10_000.0,
                metrics_config,
                0.0,
                0.0,
                symbol.to_string(),
                size_mode,
            );
            runner.run()
        }
    };

    let elapsed = start.elapsed();
    let elapsed_ms = elapsed.as_millis() as u64;
    let bars_processed = results.summary.bars_processed as u64;
    let bars_per_sec = if elapsed.as_secs_f64() > 0.0 {
        bars_processed as f64 / elapsed.as_secs_f64()
    } else {
        0.0
    };

    if json {
        let line = serde_json::json!({
            "mode": match bench_mode { BenchMode::Engine => "engine", BenchMode::Features => "features" },
            "bars_requested": bars,
            "bars_processed": bars_processed,
            "elapsed_ms": elapsed_ms,
            "bars_per_sec": bars_per_sec,
            "size_mode": "qty",
        });
        println!("{}", line);
    } else {
        println!(
            "bench: mode={} bars={} elapsed_ms={} bars_per_sec={:.2}",
            match bench_mode {
                BenchMode::Engine => "engine",
                BenchMode::Features => "features",
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
