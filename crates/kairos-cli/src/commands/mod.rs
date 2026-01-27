use crate::config::{load_config, Config};
use kairos_core::backtest::{BacktestResults, BacktestRunner};
use kairos_core::data::{ohlcv, sentiment};
use kairos_core::market_data::{MarketDataSource, VecBarSource};
use kairos_core::metrics::MetricsConfig;
use kairos_core::report::{
    read_equity_csv, read_trades_csv, recompute_summary, write_audit_jsonl, write_equity_csv,
    write_summary_html, write_summary_json, write_trades_csv, AuditEvent,
    SummaryMeta,
};
use kairos_core::risk::RiskLimits;
use kairos_core::strategy::{AgentStrategy, BuyAndHold, HoldStrategy, SimpleSma, StrategyKind};
use kairos_core::types::{ActionType, EquityPoint};
use kairos_core::{agents::AgentClient, engine_name, features};
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

pub enum Command {
    Backtest { config: PathBuf, out: Option<PathBuf> },
    Bench {
        bars: usize,
        step_seconds: i64,
        mode: String,
        json: bool,
    },
    Paper { config: PathBuf, out: Option<PathBuf> },
    Validate {
        config: PathBuf,
        strict: bool,
        out: Option<PathBuf>,
    },
    Report { input: PathBuf },
}

pub fn run(command: Command) -> Result<(), String> {
    match command {
        Command::Backtest { config, out } => run_backtest(config, out),
        Command::Bench {
            bars,
            step_seconds,
            mode,
            json,
        } => run_bench(bars, step_seconds, mode, json),
        Command::Paper { config, out } => run_paper(config, out),
        Command::Validate {
            config,
            strict,
            out,
        } => run_validate(config, strict, out),
        Command::Report { input } => run_report(input),
    }
}

#[derive(Clone, Copy)]
enum BenchMode {
    Engine,
    Features,
}

fn run_bench(bars: usize, step_seconds: i64, mode: String, json: bool) -> Result<(), String> {
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

    let run_id = format!("bench_{}_{}", match bench_mode { BenchMode::Engine => "engine", BenchMode::Features => "features" }, bars);
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
        println!("{}", line.to_string());
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
            results.summary.trades, results.summary.net_profit, results.summary.sharpe, results.summary.max_drawdown
        );
    }

    Ok(())
}

fn run_validate(config_path: PathBuf, strict: bool, out: Option<PathBuf>) -> Result<(), String> {
    let config = load_config(&config_path)?;
    print_config_summary("validate", &config, None);

    let expected_step = parse_duration_like(&config.run.timeframe)?;
    let timeframe_label = normalize_timeframe_label(&config.run.timeframe)?;
    let source_timeframe_label = normalize_timeframe_label(
        config
            .db
            .source_timeframe
            .as_deref()
            .unwrap_or(&timeframe_label),
    )?;
    let source_step = parse_duration_like(&source_timeframe_label)?;
    let exchange = config.db.exchange.to_lowercase();
    let market = config.db.market.to_lowercase();
    let (source_bars, source_report) = ohlcv::load_postgres(
        &config.db.url,
        &config.db.ohlcv_table,
        &exchange,
        &market,
        &config.run.symbol,
        &source_timeframe_label,
        Some(source_step),
    )?;
    let source_rows = source_bars.len();
    let (ohlcv_report, ohlcv_source_report, effective_rows, resampled) =
        if source_timeframe_label != timeframe_label {
        if source_step > expected_step {
            return Err(format!(
                "cannot resample OHLCV: source timeframe ({}) is larger than run timeframe ({})",
                source_timeframe_label, timeframe_label
            ));
        }
        let resampled_bars = ohlcv::resample_bars(&source_bars, expected_step)?;
        let report = ohlcv::data_quality_from_bars(&resampled_bars, Some(expected_step));
        (report, Some(source_report), resampled_bars.len(), true)
    } else {
        (source_report, None, source_rows, false)
    };

    if let Some(source) = &ohlcv_source_report {
        println!(
            "ohlcv source report (timeframe={}): duplicates={}, gaps={}, out_of_order={}, invalid_close={}",
            source_timeframe_label, source.duplicates, source.gaps, source.out_of_order, source.invalid_close
        );
    }
    println!(
        "ohlcv report{}: duplicates={}, gaps={}, out_of_order={}, invalid_close={}",
        if resampled { " (resampled)" } else { "" },
        ohlcv_report.duplicates,
        ohlcv_report.gaps,
        ohlcv_report.out_of_order,
        ohlcv_report.invalid_close
    );

    let (mut s_duplicates, mut s_out_of_order, mut s_missing, mut s_invalid, mut s_dropped) =
        (0, 0, 0, 0, 0);
    let mut sentiment_schema: Vec<String> = Vec::new();
    if let Some(path) = &config.paths.sentiment_path {
        let path_buf = PathBuf::from(path);
        let ext = path_buf
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        let missing_policy = resolve_sentiment_missing_policy(&config);
        let (_, report) = if ext == "json" {
            sentiment::load_json_with_policy(path_buf.as_path(), missing_policy)?
        } else {
            sentiment::load_csv_with_policy(path_buf.as_path(), missing_policy)?
        };
        println!(
            "sentiment report: duplicates={}, out_of_order={}, missing_values={}, invalid_values={}, dropped_rows={}",
            report.duplicates,
            report.out_of_order,
            report.missing_values,
            report.invalid_values,
            report.dropped_rows
        );
        s_duplicates = report.duplicates;
        s_out_of_order = report.out_of_order;
        s_missing = report.missing_values;
        s_invalid = report.invalid_values;
        s_dropped = report.dropped_rows;
        sentiment_schema = report.schema;
    }

    let limits = config.data_quality.as_ref();
    let max_gaps = limits.and_then(|l| l.max_gaps).unwrap_or(0);
    let max_duplicates = limits.and_then(|l| l.max_duplicates).unwrap_or(0);
    let max_out_of_order = limits.and_then(|l| l.max_out_of_order).unwrap_or(0);
    let max_invalid_close = limits.and_then(|l| l.max_invalid_close).unwrap_or(0);
    let max_sentiment_missing = limits.and_then(|l| l.max_sentiment_missing).unwrap_or(0);
    let max_sentiment_invalid = limits.and_then(|l| l.max_sentiment_invalid).unwrap_or(0);
    let max_sentiment_dropped = limits.and_then(|l| l.max_sentiment_dropped).unwrap_or(0);

    if strict {
        if ohlcv_report.gaps > max_gaps
            || ohlcv_report.duplicates > max_duplicates
            || ohlcv_report.out_of_order > max_out_of_order
            || ohlcv_report.invalid_close > max_invalid_close
            || s_duplicates > max_duplicates
            || s_out_of_order > max_out_of_order
            || s_missing > max_sentiment_missing
            || s_invalid > max_sentiment_invalid
            || s_dropped > max_sentiment_dropped
        {
            return Err("strict validation failed: data quality limits exceeded".to_string());
        }
    }

    if let Some(out_path) = out {
        let report_json = serde_json::json!({
            "ohlcv_resample": if resampled { serde_json::json!({
                "from_timeframe": source_timeframe_label,
                "to_timeframe": timeframe_label,
                "source_step_seconds": source_step,
                "target_step_seconds": expected_step,
                "source_rows": source_rows,
                "resampled_rows": effective_rows,
            }) } else { serde_json::Value::Null },
            "ohlcv_source": ohlcv_source_report.as_ref().map(|r| serde_json::json!({
                "rows": source_rows,
                "duplicates": r.duplicates,
                "gaps": r.gaps,
                "out_of_order": r.out_of_order,
                "invalid_close": r.invalid_close,
                "first_timestamp": r.first_timestamp,
                "last_timestamp": r.last_timestamp,
                "first_gap": r.first_gap,
                "first_duplicate": r.first_duplicate,
                "first_out_of_order": r.first_out_of_order,
                "first_invalid_close": r.first_invalid_close,
                "max_gap_seconds": r.max_gap_seconds,
                "gap_count": r.gap_count,
            })),
            "ohlcv": {
                "rows": effective_rows,
                "duplicates": ohlcv_report.duplicates,
                "gaps": ohlcv_report.gaps,
                "out_of_order": ohlcv_report.out_of_order,
                "invalid_close": ohlcv_report.invalid_close,
                "first_timestamp": ohlcv_report.first_timestamp,
                "last_timestamp": ohlcv_report.last_timestamp,
                "first_gap": ohlcv_report.first_gap,
                "first_duplicate": ohlcv_report.first_duplicate,
                "first_out_of_order": ohlcv_report.first_out_of_order,
                "first_invalid_close": ohlcv_report.first_invalid_close,
                "max_gap_seconds": ohlcv_report.max_gap_seconds,
                "gap_count": ohlcv_report.gap_count,
            },
            "sentiment": {
                "duplicates": s_duplicates,
                "out_of_order": s_out_of_order,
                "missing_values": s_missing,
                "invalid_values": s_invalid,
                "dropped_rows": s_dropped,
                "schema": sentiment_schema,
            },
            "limits": {
                "max_gaps": max_gaps,
                "max_duplicates": max_duplicates,
                "max_out_of_order": max_out_of_order,
                "max_invalid_close": max_invalid_close,
                "max_sentiment_missing": max_sentiment_missing,
                "max_sentiment_invalid": max_sentiment_invalid,
                "max_sentiment_dropped": max_sentiment_dropped,
            },
            "strict": strict
        });
        std::fs::write(&out_path, report_json.to_string())
            .map_err(|err| format!("failed to write report {}: {}", out_path.display(), err))?;
    }

    Ok(())
}

fn run_report(input: PathBuf) -> Result<(), String> {
    let trades_path = input.join("trades.csv");
    let equity_path = input.join("equity.csv");
    let config_path = input.join("config_snapshot.toml");

    if !trades_path.exists() || !equity_path.exists() {
        return Err(format!(
            "missing trades.csv or equity.csv in {}",
            input.display()
        ));
    }

    let trades = read_trades_csv(trades_path.as_path())?;
    let equity = read_equity_csv(equity_path.as_path())?;
    let summary = recompute_summary(&trades, &equity);

    let (meta, config_snapshot, report_html, run_id) = if config_path.exists() {
        match load_config(config_path.as_path()) {
            Ok(config) => {
                let meta = summary_meta_from_equity(&config, &equity);
                let snapshot = serde_json::json!({
                    "db": {
                        "exchange": config.db.exchange,
                        "market": config.db.market,
                        "ohlcv_table": config.db.ohlcv_table,
                    },
                    "costs": {
                        "fee_bps": config.costs.fee_bps,
                        "slippage_bps": config.costs.slippage_bps,
                    },
                    "risk": {
                        "max_position_qty": config.risk.max_position_qty,
                        "max_drawdown_pct": config.risk.max_drawdown_pct,
                        "max_exposure_pct": config.risk.max_exposure_pct,
                    },
                    "orders": {
                        "size_mode": config.orders.as_ref().and_then(|o| o.size_mode.as_deref()).unwrap_or("qty"),
                    },
                    "features": {
                        "return_mode": config.features.return_mode,
                        "sma_windows": config.features.sma_windows,
                        "volatility_windows": config.features.volatility_windows,
                        "rsi_enabled": config.features.rsi_enabled,
                        "sentiment_lag": config.features.sentiment_lag,
                        "sentiment_missing": config.features.sentiment_missing.as_deref().unwrap_or("error"),
                    },
                    "agent": {
                        "mode": config.agent.mode,
                        "url": config.agent.url,
                        "timeout_ms": config.agent.timeout_ms,
                        "retries": config.agent.retries,
                        "fallback_action": config.agent.fallback_action,
                        "api_version": config.agent.api_version,
                        "feature_version": config.agent.feature_version,
                    },
                    "data_quality": config.data_quality.as_ref().map(|dq| serde_json::json!({
                        "max_gaps": dq.max_gaps,
                        "max_duplicates": dq.max_duplicates,
                        "max_out_of_order": dq.max_out_of_order,
                        "max_invalid_close": dq.max_invalid_close,
                        "max_sentiment_missing": dq.max_sentiment_missing,
                        "max_sentiment_invalid": dq.max_sentiment_invalid,
                        "max_sentiment_dropped": dq.max_sentiment_dropped,
                    })),
                });
                let run_id = meta
                    .as_ref()
                    .map(|m| m.run_id.clone())
                    .unwrap_or_else(|| config.run.run_id);
                let report_html = config
                    .report
                    .as_ref()
                    .and_then(|report| report.html)
                    .unwrap_or(false);
                (meta, Some(snapshot), report_html, run_id)
            }
            Err(_) => (None, None, false, "report".to_string()),
        }
    } else {
        (None, None, false, "report".to_string())
    };

    write_summary_json(
        input.join("summary.json").as_path(),
        &summary,
        meta.as_ref(),
        config_snapshot.as_ref(),
    )?;
    if report_html {
        write_summary_html(input.join("summary.html").as_path(), &summary, meta.as_ref())?;
    }

    let end_ts = equity.last().map(|p| p.timestamp).unwrap_or(0);

    let mut events = Vec::with_capacity(trades.len() + 2);
    for trade in &trades {
        events.push(AuditEvent {
            run_id: run_id.clone(),
            timestamp: trade.timestamp,
            stage: "trade".to_string(),
            symbol: Some(trade.symbol.clone()),
            action: format!("{:?}", trade.side),
            error: None,
            details: serde_json::json!({
                "qty": trade.quantity,
                "price": trade.price,
                "fee": trade.fee,
                "slippage": trade.slippage,
                "strategy_id": trade.strategy_id,
                "reason": trade.reason,
            }),
        });
    }

    events.push(AuditEvent {
        run_id: run_id.clone(),
        timestamp: end_ts,
        stage: "report".to_string(),
        symbol: None,
        action: "recompute".to_string(),
        error: None,
        details: serde_json::json!({
            "input_dir": input.display().to_string(),
            "trades": trades.len(),
            "bars_processed": summary.bars_processed,
        }),
    });

    events.push(AuditEvent {
        run_id: run_id.clone(),
        timestamp: end_ts,
        stage: "summary".to_string(),
        symbol: meta.as_ref().map(|m| m.symbol.clone()),
        action: "complete".to_string(),
        error: None,
        details: serde_json::json!({
            "meta": meta.as_ref().map(|m| serde_json::json!({
                "run_id": m.run_id,
                "symbol": m.symbol,
                "timeframe": m.timeframe,
                "start": m.start,
                "end": m.end,
            })),
            "bars_processed": summary.bars_processed,
            "trades": summary.trades,
            "win_rate": summary.win_rate,
            "net_profit": summary.net_profit,
            "sharpe": summary.sharpe,
            "max_drawdown": summary.max_drawdown,
        }),
    });

    events.sort_by(|a, b| {
        a.timestamp
            .cmp(&b.timestamp)
            .then_with(|| a.stage.cmp(&b.stage))
            .then_with(|| a.action.cmp(&b.action))
    });
    write_audit_jsonl(input.join("logs.jsonl").as_path(), &events)?;

    println!(
        "{} cli: report regenerated (trades={}, bars={})",
        engine_name(),
        trades.len(),
        summary.bars_processed
    );
    Ok(())
}

fn print_config_summary(command: &str, config: &Config, out: Option<&PathBuf>) {
    println!(
        "{} cli: {} (run_id={}, symbol={}, timeframe={}, initial_capital={})",
        engine_name(),
        command,
        config.run.run_id,
        config.run.symbol,
        config.run.timeframe,
        config.run.initial_capital
    );
    println!(
        "data: db_url={}, table={}, exchange={}, market={}, source_timeframe={}, sentiment={}, out_dir={}",
        config.db.url,
        config.db.ohlcv_table,
        config.db.exchange,
        config.db.market,
        config
            .db
            .source_timeframe
            .as_deref()
            .unwrap_or("same_as_run"),
        config
            .paths
            .sentiment_path
            .as_deref()
            .unwrap_or("none"),
        config.paths.out_dir
    );
    println!(
        "costs: fee_bps={}, slippage_bps={}",
        config.costs.fee_bps, config.costs.slippage_bps
    );
    println!(
        "risk: max_position_qty={}, max_drawdown_pct={}, max_exposure_pct={}",
        config.risk.max_position_qty,
        config.risk.max_drawdown_pct,
        config.risk.max_exposure_pct
    );
    println!(
        "orders: size_mode={}",
        config
            .orders
            .as_ref()
            .and_then(|orders| orders.size_mode.as_deref())
            .unwrap_or("qty")
    );
    println!(
        "features: return_mode={}, sma_windows={:?}, rsi_enabled={}, sentiment_lag={}, sentiment_missing={}",
        config.features.return_mode,
        config.features.sma_windows,
        config.features.rsi_enabled,
        config.features.sentiment_lag,
        config.features
            .sentiment_missing
            .as_deref()
            .unwrap_or("error")
    );
    println!(
        "agent: mode={}, url={}, timeout_ms={}, retries={}, fallback_action={}, api_version={}, feature_version={}",
        config.agent.mode,
        config.agent.url,
        config.agent.timeout_ms,
        config.agent.retries,
        config.agent.fallback_action,
        config.agent.api_version,
        config.agent.feature_version
    );
    if let Some(out_dir) = out {
        println!("output dir: {}", out_dir.display());
    }
}

fn run_backtest(config_path: PathBuf, out: Option<PathBuf>) -> Result<(), String> {
    let config = load_config(&config_path)?;
    print_config_summary("backtest", &config, out.as_ref());

    let mut audit_extras: Vec<AuditEvent> = Vec::new();
    let overall_start = Instant::now();

    let expected_step = parse_duration_like(&config.run.timeframe)?;
    let timeframe_label = normalize_timeframe_label(&config.run.timeframe)?;
    let source_timeframe_label = normalize_timeframe_label(
        config
            .db
            .source_timeframe
            .as_deref()
            .unwrap_or(&timeframe_label),
    )?;
    let source_step = parse_duration_like(&source_timeframe_label)?;
    let exchange = config.db.exchange.to_lowercase();
    let market = config.db.market.to_lowercase();
    let stage_start = Instant::now();
    let (source_bars, source_report) = ohlcv::load_postgres(
        &config.db.url,
        &config.db.ohlcv_table,
        &exchange,
        &market,
        &config.run.symbol,
        &source_timeframe_label,
        Some(source_step),
    )?;
    let (bars, data_report, resampled) = if source_timeframe_label != timeframe_label {
        if source_step > expected_step {
            return Err(format!(
                "cannot resample OHLCV: source timeframe ({}) is larger than run timeframe ({})",
                source_timeframe_label, timeframe_label
            ));
        }
        let resample_start = Instant::now();
        let resampled_bars = ohlcv::resample_bars(&source_bars, expected_step)?;
        let report = ohlcv::data_quality_from_bars(&resampled_bars, Some(expected_step));
        audit_extras.push(timing_event(
            &config.run.run_id,
            0,
            "timing",
            Some(&config.run.symbol),
            "resample_ohlcv",
            resample_start.elapsed().as_millis() as u64,
            serde_json::json!({
                "from_timeframe": source_timeframe_label,
                "to_timeframe": timeframe_label,
                "source_rows": source_bars.len(),
                "resampled_rows": resampled_bars.len(),
            }),
        ));
        (resampled_bars, report, true)
    } else {
        (source_bars, source_report, false)
    };
    audit_extras.push(timing_event(
        &config.run.run_id,
        0,
        "timing",
        Some(&config.run.symbol),
        "load_ohlcv",
        stage_start.elapsed().as_millis() as u64,
        serde_json::json!({
            "rows": bars.len(),
            "duplicates": data_report.duplicates,
            "gaps": data_report.gaps,
            "out_of_order": data_report.out_of_order,
            "invalid_close": data_report.invalid_close,
            "resampled": resampled,
        }),
    ));

    let sentiment_points = if let Some(path) = &config.paths.sentiment_path {
        let stage_start = Instant::now();
        let path_buf = PathBuf::from(path);
        let ext = path_buf
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        let missing_policy = resolve_sentiment_missing_policy(&config);
        let (points, report) = if ext == "json" {
            sentiment::load_json_with_policy(path_buf.as_path(), missing_policy)?
        } else {
            sentiment::load_csv_with_policy(path_buf.as_path(), missing_policy)?
        };
        if report.duplicates > 0
            || report.out_of_order > 0
            || report.missing_values > 0
            || report.invalid_values > 0
            || report.dropped_rows > 0
        {
            println!(
                "sentiment report: duplicates={}, out_of_order={}, missing_values={}, invalid_values={}, dropped_rows={}",
                report.duplicates,
                report.out_of_order,
                report.missing_values,
                report.invalid_values,
                report.dropped_rows
            );
        }
        audit_extras.push(timing_event(
            &config.run.run_id,
            0,
            "timing",
            Some(&config.run.symbol),
            "load_sentiment",
            stage_start.elapsed().as_millis() as u64,
            serde_json::json!({
                "rows": points.len(),
                "duplicates": report.duplicates,
                "out_of_order": report.out_of_order,
                "missing_values": report.missing_values,
                "invalid_values": report.invalid_values,
                "dropped_rows": report.dropped_rows,
                "schema": report.schema,
            }),
        ));
        Some(points)
    } else {
        None
    };

    if data_report.duplicates > 0
        || data_report.gaps > 0
        || data_report.out_of_order > 0
        || data_report.invalid_close > 0
    {
        println!(
            "ohlcv report: duplicates={}, gaps={}, out_of_order={}, invalid_close={}",
            data_report.duplicates,
            data_report.gaps,
            data_report.out_of_order,
            data_report.invalid_close
        );
    }

    let sentiment_lag = parse_duration_like(&config.features.sentiment_lag)?;
    let bar_timestamps: Vec<i64> = bars.iter().map(|bar| bar.timestamp).collect();
    let stage_start = Instant::now();
    let aligned_sentiment = sentiment_points
        .as_ref()
        .map(|points| sentiment::align_with_bars(&bar_timestamps, points, sentiment_lag))
        .unwrap_or_else(|| vec![None; bars.len()]);
    audit_extras.push(timing_event(
        &config.run.run_id,
        0,
        "timing",
        Some(&config.run.symbol),
        "align_sentiment",
        stage_start.elapsed().as_millis() as u64,
        serde_json::json!({
            "lag_seconds": sentiment_lag,
        }),
    ));

    let feature_config = features::FeatureConfig {
        return_mode: match config.features.return_mode.as_str() {
            "log" => features::ReturnMode::Log,
            _ => features::ReturnMode::Pct,
        },
        sma_windows: config.features.sma_windows.iter().map(|w| *w as usize).collect(),
        volatility_windows: config
            .features
            .volatility_windows
            .as_ref()
            .map(|windows| windows.iter().map(|w| *w as usize).collect())
            .unwrap_or_default(),
        rsi_enabled: config.features.rsi_enabled,
    };
    let builder = features::FeatureBuilder::new(feature_config);

    let risk_limits = RiskLimits {
        max_position_qty: config.risk.max_position_qty,
        max_drawdown_pct: config.risk.max_drawdown_pct,
        max_exposure_pct: config.risk.max_exposure_pct,
    };

    let size_mode = resolve_size_mode(&config);

    let strategy = match config.agent.mode.as_str() {
        "remote" => {
            let fallback_action = parse_action_type(&config.agent.fallback_action)?;
            let agent = AgentClient::new(
                config.agent.url.clone(),
                config.agent.timeout_ms,
                config.agent.api_version.clone(),
                config.agent.feature_version.clone(),
                config.agent.retries,
                fallback_action,
            );
            StrategyKind::Agent(AgentStrategy::new(
                config.run.run_id.clone(),
                config.run.symbol.clone(),
                config.run.timeframe.clone(),
                config.agent.feature_version.clone(),
                agent,
                builder,
                aligned_sentiment,
            ))
        }
        "baseline" => {
            let baseline = config
                .strategy
                .as_ref()
                .map(|strategy| strategy.baseline.as_str())
                .unwrap_or("buy_and_hold");
            match baseline {
                "sma" => {
                    let (short, long) = resolve_sma_windows(&config);
                    StrategyKind::SimpleSma(SimpleSma::new(short, long))
                }
                _ => StrategyKind::BuyAndHold(BuyAndHold::new(1.0)),
            }
        }
        _ => StrategyKind::Hold(HoldStrategy),
    };

    let metrics_config = build_metrics_config(&config);

    let data = VecBarSource::new(bars);
    let stage_start = Instant::now();
    let mut runner = BacktestRunner::new(
        config.run.run_id.clone(),
        strategy,
        data,
        risk_limits,
        config.run.initial_capital,
        metrics_config,
        config.costs.fee_bps,
        config.costs.slippage_bps,
        config.run.symbol.clone(),
        size_mode,
    );
    let results = runner.run();
    audit_extras.push(timing_event(
        &config.run.run_id,
        0,
        "timing",
        Some(&config.run.symbol),
        "run_engine",
        stage_start.elapsed().as_millis() as u64,
        serde_json::json!({}),
    ));

    write_outputs(&config, out, results, &config_path, audit_extras)?;
    println!(
        "{} cli: backtest total_ms={}",
        engine_name(),
        overall_start.elapsed().as_millis()
    );
    Ok(())
}

fn write_outputs(
    config: &Config,
    out: Option<PathBuf>,
    results: BacktestResults,
    config_path: &PathBuf,
    mut audit_extras: Vec<AuditEvent>,
) -> Result<(), String> {
    let base_dir = out.unwrap_or_else(|| PathBuf::from(&config.paths.out_dir));
    let run_dir = base_dir.join(&config.run.run_id);
    std::fs::create_dir_all(&run_dir)
        .map_err(|err| format!("failed to create run dir {}: {}", run_dir.display(), err))?;

    write_trades_csv(run_dir.join("trades.csv").as_path(), &results.trades)?;
    write_equity_csv(run_dir.join("equity.csv").as_path(), &results.equity)?;
    let meta = summary_meta_from_equity(config, &results.equity);
    let config_snapshot = serde_json::json!({
        "db": {
            "exchange": config.db.exchange.clone(),
            "market": config.db.market.clone(),
            "ohlcv_table": config.db.ohlcv_table.clone(),
        },
        "costs": {
            "fee_bps": config.costs.fee_bps,
            "slippage_bps": config.costs.slippage_bps,
        },
        "risk": {
            "max_position_qty": config.risk.max_position_qty,
            "max_drawdown_pct": config.risk.max_drawdown_pct,
            "max_exposure_pct": config.risk.max_exposure_pct,
        },
        "orders": {
            "size_mode": config.orders.as_ref().and_then(|o| o.size_mode.as_deref()).unwrap_or("qty"),
        },
        "features": {
            "return_mode": config.features.return_mode.clone(),
            "sma_windows": config.features.sma_windows.clone(),
            "volatility_windows": config.features.volatility_windows.clone(),
            "rsi_enabled": config.features.rsi_enabled,
            "sentiment_lag": config.features.sentiment_lag.clone(),
            "sentiment_missing": config.features.sentiment_missing.as_deref().unwrap_or("error"),
        },
        "agent": {
            "mode": config.agent.mode.clone(),
            "url": config.agent.url.clone(),
            "timeout_ms": config.agent.timeout_ms,
            "retries": config.agent.retries,
            "fallback_action": config.agent.fallback_action.clone(),
            "api_version": config.agent.api_version.clone(),
            "feature_version": config.agent.feature_version.clone(),
        },
        "data_quality": config.data_quality.as_ref().map(|dq| serde_json::json!({
            "max_gaps": dq.max_gaps,
            "max_duplicates": dq.max_duplicates,
            "max_out_of_order": dq.max_out_of_order,
            "max_invalid_close": dq.max_invalid_close,
            "max_sentiment_missing": dq.max_sentiment_missing,
            "max_sentiment_invalid": dq.max_sentiment_invalid,
            "max_sentiment_dropped": dq.max_sentiment_dropped,
        })),
    });
    write_summary_json(
        run_dir.join("summary.json").as_path(),
        &results.summary,
        meta.as_ref(),
        Some(&config_snapshot),
    )?;
    let mut audit_events = results.audit_events;
    audit_events.append(&mut audit_extras);
    audit_events.sort_by(|a, b| {
        a.timestamp
            .cmp(&b.timestamp)
            .then_with(|| a.stage.cmp(&b.stage))
            .then_with(|| a.action.cmp(&b.action))
    });
    write_audit_jsonl(run_dir.join("logs.jsonl").as_path(), &audit_events)?;
    if config
        .report
        .as_ref()
        .and_then(|report| report.html)
        .unwrap_or(false)
    {
        write_summary_html(
            run_dir.join("summary.html").as_path(),
            &results.summary,
            meta.as_ref(),
        )?;
    }
    std::fs::copy(config_path, run_dir.join("config_snapshot.toml")).map_err(|err| {
        format!(
            "failed to copy config to snapshot {}: {}",
            run_dir.display(),
            err
        )
    })?;

    println!("run output: {}", run_dir.display());
    Ok(())
}

fn parse_duration_like(value: &str) -> Result<i64, String> {
    let trimmed = value.trim().to_lowercase();
    if trimmed.is_empty() {
        return Err("empty duration".to_string());
    }
    if let Ok(seconds) = trimmed.parse::<i64>() {
        return Ok(seconds);
    }

    let (number_part, unit) = if let Some(stripped) = trimmed.strip_suffix("min") {
        (stripped, "min")
    } else if let Some(stripped) = trimmed.strip_suffix("hour") {
        (stripped, "hour")
    } else if let Some(stripped) = trimmed.strip_suffix("day") {
        (stripped, "day")
    } else if let Some(stripped) = trimmed.strip_suffix("week") {
        (stripped, "week")
    } else {
        let (number_part, unit) = trimmed.split_at(trimmed.len() - 1);
        (number_part, unit)
    };

    let multiplier = match unit {
        "s" => 1,
        "m" | "min" => 60,
        "h" | "hour" => 3600,
        "d" | "day" => 86400,
        "w" | "week" => 604800,
        _ => return Err(format!("unsupported duration unit: {unit}")),
    };

    let number: i64 = number_part
        .parse()
        .map_err(|_| format!("invalid duration: {value}"))?;
    Ok(number * multiplier)
}

fn normalize_timeframe_label(value: &str) -> Result<String, String> {
    let normalized = value.trim().to_lowercase();
    let label = match normalized.as_str() {
        "1m" | "1min" => "1min",
        "3m" | "3min" => "3min",
        "5m" | "5min" => "5min",
        "15m" | "15min" => "15min",
        "30m" | "30min" => "30min",
        "1h" | "1hour" => "1hour",
        "2h" | "2hour" => "2hour",
        "4h" | "4hour" => "4hour",
        "6h" | "6hour" => "6hour",
        "8h" | "8hour" => "8hour",
        "12h" | "12hour" => "12hour",
        "1d" | "1day" => "1day",
        "1w" | "1week" => "1week",
        "1mo" | "1month" => "1month",
        _ => return Err(format!("unsupported timeframe: {value}")),
    };
    Ok(label.to_string())
}

fn summary_meta_from_equity(config: &Config, equity: &[EquityPoint]) -> Option<SummaryMeta> {
    let start = equity.first()?.timestamp;
    let end = equity.last()?.timestamp;
    Some(SummaryMeta {
        run_id: config.run.run_id.clone(),
        symbol: config.run.symbol.clone(),
        timeframe: config.run.timeframe.clone(),
        start,
        end,
    })
}

fn build_metrics_config(config: &Config) -> MetricsConfig {
    let risk_free_rate = config
        .metrics
        .as_ref()
        .and_then(|metrics| metrics.risk_free_rate)
        .unwrap_or(0.0);
    let annualization_factor = config
        .metrics
        .as_ref()
        .and_then(|metrics| metrics.annualization_factor);
    MetricsConfig {
        risk_free_rate,
        annualization_factor,
    }
}

fn resolve_sma_windows(config: &Config) -> (usize, usize) {
    if let Some(strategy) = &config.strategy {
        if let (Some(short), Some(long)) = (strategy.sma_short, strategy.sma_long) {
            return (short as usize, long as usize);
        }
    }
    if config.features.sma_windows.len() >= 2 {
        return (
            config.features.sma_windows[0] as usize,
            config.features.sma_windows[1] as usize,
        );
    }
    (10, 50)
}

fn parse_action_type(value: &str) -> Result<ActionType, String> {
    match value.to_uppercase().as_str() {
        "BUY" => Ok(ActionType::Buy),
        "SELL" => Ok(ActionType::Sell),
        "HOLD" => Ok(ActionType::Hold),
        _ => Err(format!("unsupported action type: {}", value)),
    }
}

fn run_paper(config_path: PathBuf, out: Option<PathBuf>) -> Result<(), String> {
    let config = load_config(&config_path)?;
    print_config_summary("paper", &config, out.as_ref());

    let mut audit_extras: Vec<AuditEvent> = Vec::new();
    let overall_start = Instant::now();

    let expected_step = parse_duration_like(&config.run.timeframe)?;
    let timeframe_label = normalize_timeframe_label(&config.run.timeframe)?;
    let source_timeframe_label = normalize_timeframe_label(
        config
            .db
            .source_timeframe
            .as_deref()
            .unwrap_or(&timeframe_label),
    )?;
    let source_step = parse_duration_like(&source_timeframe_label)?;
    let exchange = config.db.exchange.to_lowercase();
    let market = config.db.market.to_lowercase();
    let stage_start = Instant::now();
    let (source_bars, source_report) = ohlcv::load_postgres(
        &config.db.url,
        &config.db.ohlcv_table,
        &exchange,
        &market,
        &config.run.symbol,
        &source_timeframe_label,
        Some(source_step),
    )?;
    let (bars, data_report, resampled) = if source_timeframe_label != timeframe_label {
        if source_step > expected_step {
            return Err(format!(
                "cannot resample OHLCV: source timeframe ({}) is larger than run timeframe ({})",
                source_timeframe_label, timeframe_label
            ));
        }
        let resample_start = Instant::now();
        let resampled_bars = ohlcv::resample_bars(&source_bars, expected_step)?;
        let report = ohlcv::data_quality_from_bars(&resampled_bars, Some(expected_step));
        audit_extras.push(timing_event(
            &config.run.run_id,
            0,
            "timing",
            Some(&config.run.symbol),
            "resample_ohlcv",
            resample_start.elapsed().as_millis() as u64,
            serde_json::json!({
                "from_timeframe": source_timeframe_label,
                "to_timeframe": timeframe_label,
                "source_rows": source_bars.len(),
                "resampled_rows": resampled_bars.len(),
            }),
        ));
        (resampled_bars, report, true)
    } else {
        (source_bars, source_report, false)
    };
    audit_extras.push(timing_event(
        &config.run.run_id,
        0,
        "timing",
        Some(&config.run.symbol),
        "load_ohlcv",
        stage_start.elapsed().as_millis() as u64,
        serde_json::json!({
            "rows": bars.len(),
            "duplicates": data_report.duplicates,
            "gaps": data_report.gaps,
            "out_of_order": data_report.out_of_order,
            "invalid_close": data_report.invalid_close,
            "resampled": resampled,
        }),
    ));

    if data_report.duplicates > 0
        || data_report.gaps > 0
        || data_report.out_of_order > 0
        || data_report.invalid_close > 0
    {
        println!(
            "ohlcv report: duplicates={}, gaps={}, out_of_order={}, invalid_close={}",
            data_report.duplicates,
            data_report.gaps,
            data_report.out_of_order,
            data_report.invalid_close
        );
    }

    let sentiment_points = if let Some(path) = &config.paths.sentiment_path {
        let stage_start = Instant::now();
        let path_buf = PathBuf::from(path);
        let ext = path_buf
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        let missing_policy = resolve_sentiment_missing_policy(&config);
        let (points, report) = if ext == "json" {
            sentiment::load_json_with_policy(path_buf.as_path(), missing_policy)?
        } else {
            sentiment::load_csv_with_policy(path_buf.as_path(), missing_policy)?
        };
        if report.duplicates > 0
            || report.out_of_order > 0
            || report.missing_values > 0
            || report.invalid_values > 0
            || report.dropped_rows > 0
        {
            println!(
                "sentiment report: duplicates={}, out_of_order={}, missing_values={}, invalid_values={}, dropped_rows={}",
                report.duplicates,
                report.out_of_order,
                report.missing_values,
                report.invalid_values,
                report.dropped_rows
            );
        }
        audit_extras.push(timing_event(
            &config.run.run_id,
            0,
            "timing",
            Some(&config.run.symbol),
            "load_sentiment",
            stage_start.elapsed().as_millis() as u64,
            serde_json::json!({
                "rows": points.len(),
                "duplicates": report.duplicates,
                "out_of_order": report.out_of_order,
                "missing_values": report.missing_values,
                "invalid_values": report.invalid_values,
                "dropped_rows": report.dropped_rows,
                "schema": report.schema,
            }),
        ));
        Some(points)
    } else {
        None
    };

    let sentiment_lag = parse_duration_like(&config.features.sentiment_lag)?;
    let bar_timestamps: Vec<i64> = bars.iter().map(|bar| bar.timestamp).collect();
    let stage_start = Instant::now();
    let aligned_sentiment = sentiment_points
        .as_ref()
        .map(|points| sentiment::align_with_bars(&bar_timestamps, points, sentiment_lag))
        .unwrap_or_else(|| vec![None; bars.len()]);
    audit_extras.push(timing_event(
        &config.run.run_id,
        0,
        "timing",
        Some(&config.run.symbol),
        "align_sentiment",
        stage_start.elapsed().as_millis() as u64,
        serde_json::json!({
            "lag_seconds": sentiment_lag,
        }),
    ));

    let feature_config = features::FeatureConfig {
        return_mode: match config.features.return_mode.as_str() {
            "log" => features::ReturnMode::Log,
            _ => features::ReturnMode::Pct,
        },
        sma_windows: config.features.sma_windows.iter().map(|w| *w as usize).collect(),
        volatility_windows: config
            .features
            .volatility_windows
            .as_ref()
            .map(|windows| windows.iter().map(|w| *w as usize).collect())
            .unwrap_or_default(),
        rsi_enabled: config.features.rsi_enabled,
    };
    let builder = features::FeatureBuilder::new(feature_config);

    let strategy = match config.agent.mode.as_str() {
        "remote" => {
            let fallback_action = parse_action_type(&config.agent.fallback_action)?;
            let agent = AgentClient::new(
                config.agent.url.clone(),
                config.agent.timeout_ms,
                config.agent.api_version.clone(),
                config.agent.feature_version.clone(),
                config.agent.retries,
                fallback_action,
            );
            StrategyKind::Agent(AgentStrategy::new(
                config.run.run_id.clone(),
                config.run.symbol.clone(),
                config.run.timeframe.clone(),
                config.agent.feature_version.clone(),
                agent,
                builder,
                aligned_sentiment,
            ))
        }
        "baseline" => {
            let baseline = config
                .strategy
                .as_ref()
                .map(|strategy| strategy.baseline.as_str())
                .unwrap_or("buy_and_hold");
            match baseline {
                "sma" => {
                    let (short, long) = resolve_sma_windows(&config);
                    StrategyKind::SimpleSma(SimpleSma::new(short, long))
                }
                _ => StrategyKind::BuyAndHold(BuyAndHold::new(1.0)),
            }
        }
        _ => StrategyKind::Hold(HoldStrategy),
    };

    let metrics_config = build_metrics_config(&config);

    let risk_limits = RiskLimits {
        max_position_qty: config.risk.max_position_qty,
        max_drawdown_pct: config.risk.max_drawdown_pct,
        max_exposure_pct: config.risk.max_exposure_pct,
    };

    let size_mode = resolve_size_mode(&config);

    let timeframe_seconds = parse_duration_like(&config.run.timeframe)?;
    let replay_scale = config
        .paper
        .as_ref()
        .and_then(|paper| paper.replay_scale)
        .unwrap_or(60);
    let data = RealtimeBarSource::new(bars, timeframe_seconds, replay_scale);
    let stage_start = Instant::now();
    let mut runner = BacktestRunner::new(
        config.run.run_id.clone(),
        strategy,
        data,
        risk_limits,
        config.run.initial_capital,
        metrics_config,
        config.costs.fee_bps,
        config.costs.slippage_bps,
        config.run.symbol.clone(),
        size_mode,
    );
    let results = runner.run();
    audit_extras.push(timing_event(
        &config.run.run_id,
        0,
        "timing",
        Some(&config.run.symbol),
        "run_engine",
        stage_start.elapsed().as_millis() as u64,
        serde_json::json!({}),
    ));
    write_outputs(&config, out, results, &config_path, audit_extras)?;
    println!(
        "{} cli: paper total_ms={}",
        engine_name(),
        overall_start.elapsed().as_millis()
    );
    Ok(())
}

fn resolve_size_mode(config: &Config) -> kairos_core::backtest::OrderSizeMode {
    match config
        .orders
        .as_ref()
        .and_then(|orders| orders.size_mode.as_deref())
        .map(|s| s.trim().to_lowercase())
        .as_deref()
    {
        Some("pct_equity") | Some("equity_pct") | Some("pct") => {
            kairos_core::backtest::OrderSizeMode::PctEquity
        }
        _ => kairos_core::backtest::OrderSizeMode::Quantity,
    }
}

fn resolve_sentiment_missing_policy(config: &Config) -> sentiment::MissingValuePolicy {
    match config
        .features
        .sentiment_missing
        .as_deref()
        .unwrap_or("error")
        .trim()
        .to_lowercase()
        .as_str()
    {
        "zero" | "zero_fill" => sentiment::MissingValuePolicy::ZeroFill,
        "ffill" | "forward_fill" => sentiment::MissingValuePolicy::ForwardFill,
        "drop" | "drop_row" => sentiment::MissingValuePolicy::DropRow,
        _ => sentiment::MissingValuePolicy::Error,
    }
}

fn timing_event(
    run_id: &str,
    timestamp: i64,
    stage: &str,
    symbol: Option<&str>,
    action: &str,
    duration_ms: u64,
    details: serde_json::Value,
) -> AuditEvent {
    AuditEvent {
        run_id: run_id.to_string(),
        timestamp,
        stage: stage.to_string(),
        symbol: symbol.map(|s| s.to_string()),
        action: action.to_string(),
        error: None,
        details: serde_json::json!({
            "duration_ms": duration_ms,
            "details": details,
        }),
    }
}

struct RealtimeBarSource {
    bars: Vec<kairos_core::types::Bar>,
    index: usize,
    sleep_seconds: i64,
    last_tick: Option<Instant>,
}

impl RealtimeBarSource {
    fn new(
        bars: Vec<kairos_core::types::Bar>,
        sleep_seconds: i64,
        replay_scale: u64,
    ) -> Self {
        let scaled = if replay_scale == 0 {
            0
        } else {
            sleep_seconds / replay_scale as i64
        };
        Self {
            bars,
            index: 0,
            sleep_seconds: scaled.max(0),
            last_tick: None,
        }
    }
}

impl MarketDataSource for RealtimeBarSource {
    fn next_bar(&mut self) -> Option<kairos_core::types::Bar> {
        if self.index >= self.bars.len() {
            return None;
        }
        if self.sleep_seconds > 0 {
            let now = Instant::now();
            if let Some(last) = self.last_tick {
                let elapsed = now.saturating_duration_since(last);
                let target = Duration::from_secs(self.sleep_seconds as u64);
                if elapsed < target {
                    thread::sleep(target - elapsed);
                }
            } else {
                thread::sleep(Duration::from_secs(self.sleep_seconds as u64));
            }
            self.last_tick = Some(Instant::now());
        }
        let bar = self.bars[self.index].clone();
        self.index += 1;
        Some(bar)
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_timeframe_label, parse_action_type, parse_duration_like, run_backtest, run_validate};
    use std::fs;
    use std::path::PathBuf;

    fn write_file(path: &PathBuf, contents: &str) {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::write(path, contents).expect("write file");
    }

    fn sample_config(tmp_dir: &PathBuf, db_url: &str) -> PathBuf {
        let config_path = tmp_dir.join("config.toml");
        let toml_contents = format!(
            "\
[run]\n\
run_id = \"test_run\"\n\
symbol = \"BTCUSD\"\n\
timeframe = \"1m\"\n\
initial_capital = 1000.0\n\
\n\
[db]\n\
url = \"{}\"\n\
ohlcv_table = \"ohlcv_candles\"\n\
exchange = \"kucoin\"\n\
market = \"spot\"\n\
\n\
[paths]\n\
out_dir = \"{}\"\n\
\n\
[costs]\n\
fee_bps = 0.0\n\
slippage_bps = 0.0\n\
\n\
[risk]\n\
max_position_qty = 1.0\n\
max_drawdown_pct = 1.0\n\
max_exposure_pct = 1.0\n\
\n\
[features]\n\
return_mode = \"pct\"\n\
sma_windows = [2]\n\
rsi_enabled = false\n\
sentiment_lag = \"1s\"\n\
\n\
[agent]\n\
mode = \"baseline\"\n\
url = \"http://127.0.0.1:8000\"\n\
timeout_ms = 200\n\
retries = 0\n\
fallback_action = \"HOLD\"\n\
api_version = \"v1\"\n\
feature_version = \"v1\"\n",
            db_url,
            tmp_dir.display()
        );
        write_file(&config_path, &toml_contents);
        config_path
    }

    #[test]
    fn parse_duration_like_handles_units() {
        assert_eq!(parse_duration_like("5s").unwrap(), 5);
        assert_eq!(parse_duration_like("2m").unwrap(), 120);
        assert_eq!(parse_duration_like("1h").unwrap(), 3600);
        assert_eq!(parse_duration_like("1min").unwrap(), 60);
    }

    #[test]
    fn parse_action_type_handles_values() {
        assert_eq!(parse_action_type("buy").unwrap() as u8, 0);
        assert_eq!(parse_action_type("sell").unwrap() as u8, 1);
        assert_eq!(parse_action_type("hold").unwrap() as u8, 2);
    }

    #[test]
    fn normalize_timeframe_label_handles_aliases() {
        assert_eq!(normalize_timeframe_label("1m").unwrap(), "1min");
        assert_eq!(normalize_timeframe_label("1hour").unwrap(), "1hour");
        assert_eq!(normalize_timeframe_label("1d").unwrap(), "1day");
    }

    #[test]
    fn run_validate_reads_postgres() {
        if std::env::var("KAIROS_DB_RUN_TESTS").ok().as_deref() != Some("1") {
            return;
        }
        let db_url = std::env::var("KAIROS_DB_URL").expect("KAIROS_DB_URL must be set");
        let tmp_dir = PathBuf::from("/tmp/kairos_cli_validate");
        let config_path = sample_config(&tmp_dir, &db_url);
        run_validate(config_path, false, None).expect("validate");
    }

    #[test]
    fn run_backtest_writes_outputs() {
        if std::env::var("KAIROS_DB_RUN_TESTS").ok().as_deref() != Some("1") {
            return;
        }
        let db_url = std::env::var("KAIROS_DB_URL").expect("KAIROS_DB_URL must be set");
        let tmp_dir = PathBuf::from("/tmp/kairos_cli_backtest");
        let config_path = sample_config(&tmp_dir, &db_url);
        run_backtest(config_path.clone(), None).expect("backtest");
        let run_dir = tmp_dir.join("test_run");
        assert!(run_dir.join("summary.json").exists());
        assert!(run_dir.join("trades.csv").exists());
        assert!(run_dir.join("equity.csv").exists());
        assert!(run_dir.join("config_snapshot.toml").exists());
    }
}
