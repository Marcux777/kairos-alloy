use crate::config::{AgentMode, Config};
use crate::shared::{
    build_metrics_config, config_snapshot_json, normalize_timeframe_label, parse_duration_like,
    resolve_execution_config, resolve_sentiment_missing_policy, resolve_size_mode,
    resolve_sma_windows, summary_meta_json_from_equity,
};
use kairos_domain::entities::risk::RiskLimits;
use kairos_domain::repositories::agent::AgentClient as AgentPort;
use kairos_domain::repositories::artifacts::ArtifactWriter;
use kairos_domain::repositories::market_data::{MarketDataRepository, OhlcvQuery};
use kairos_domain::repositories::sentiment::{
    SentimentFormat, SentimentQuery, SentimentRepository,
};
use kairos_domain::services::audit::AuditEvent;
use kairos_domain::services::engine::backtest::{BacktestResults, BacktestRunner};
use kairos_domain::services::features;
use kairos_domain::services::market_data_source::VecBarSource;
use kairos_domain::services::ohlcv::{data_quality_from_bars, resample_bars};
use kairos_domain::services::sentiment;
use kairos_domain::services::strategy::{
    AgentStrategy, BuyAndHold, HoldStrategy, SimpleSma, StrategyKind,
};
use std::path::PathBuf;
use std::time::Instant;
use tracing::info_span;

pub fn run_backtest(
    config: &Config,
    config_toml: &str,
    out: Option<PathBuf>,
    market_data: &dyn MarketDataRepository,
    sentiment_repo: &dyn SentimentRepository,
    artifacts: &dyn ArtifactWriter,
    remote_agent: Option<Box<dyn AgentPort>>,
) -> Result<PathBuf, String> {
    let _span = info_span!(
        "run_backtest",
        run_id = %config.run.run_id,
        symbol = %config.run.symbol,
        timeframe = %config.run.timeframe
    )
    .entered();

    let mut audit_extras: Vec<AuditEvent> = Vec::new();

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

    let stage_start = Instant::now();
    let (source_bars, source_report) = market_data.load_ohlcv(&OhlcvQuery {
        exchange: config.db.exchange.to_lowercase(),
        market: config.db.market.to_lowercase(),
        symbol: config.run.symbol.clone(),
        timeframe: source_timeframe_label.clone(),
        expected_step_seconds: Some(source_step),
    })?;
    metrics::histogram!("kairos.backtest.load_ohlcv_ms")
        .record(stage_start.elapsed().as_millis() as f64);

    let (bars, data_report, resampled) = if source_timeframe_label != timeframe_label {
        if source_step > expected_step {
            return Err(format!(
                "cannot resample OHLCV: source timeframe ({}) is larger than run timeframe ({})",
                source_timeframe_label, timeframe_label
            ));
        }

        let resample_start = Instant::now();
        let resampled_bars = resample_bars(&source_bars, expected_step)?;
        let report = data_quality_from_bars(&resampled_bars, Some(expected_step));
        metrics::histogram!("kairos.backtest.resample_ms")
            .record(resample_start.elapsed().as_millis() as f64);
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
        let format = if ext == "json" {
            SentimentFormat::Json
        } else {
            SentimentFormat::Csv
        };
        let missing_policy = resolve_sentiment_missing_policy(config);
        let (points, report) = sentiment_repo.load_sentiment(&SentimentQuery {
            path: path_buf,
            format,
            missing_policy,
        })?;
        metrics::histogram!("kairos.backtest.load_sentiment_ms")
            .record(stage_start.elapsed().as_millis() as f64);

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
    metrics::histogram!("kairos.backtest.align_sentiment_ms")
        .record(stage_start.elapsed().as_millis() as f64);
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
        return_mode: config.features.return_mode,
        sma_windows: config
            .features
            .sma_windows
            .iter()
            .map(|w| *w as usize)
            .collect(),
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

    let size_mode = resolve_size_mode(config);

    let strategy = match config.agent.mode {
        AgentMode::Remote => {
            let Some(agent) = remote_agent else {
                return Err("agent.mode=remote requires a remote_agent client".to_string());
            };
            let fallback_action = config.agent.fallback_action;
            let agent_url = config.agent.url.clone();
            StrategyKind::Agent(AgentStrategy::new(
                config.run.run_id.clone(),
                config.run.symbol.clone(),
                config.run.timeframe.clone(),
                config.agent.api_version.clone(),
                config.agent.feature_version.clone(),
                agent_url,
                fallback_action,
                agent,
                builder,
                aligned_sentiment,
            ))
        }
        AgentMode::Baseline => {
            let baseline = config
                .strategy
                .as_ref()
                .map(|strategy| strategy.baseline.as_str())
                .unwrap_or("buy_and_hold");
            match baseline {
                "sma" => {
                    let (short, long) = resolve_sma_windows(config);
                    StrategyKind::SimpleSma(SimpleSma::new(short, long))
                }
                _ => StrategyKind::BuyAndHold(BuyAndHold::new(1.0)),
            }
        }
        AgentMode::Hold => StrategyKind::Hold(HoldStrategy),
    };

    let metrics_config = build_metrics_config(config);
    let execution = resolve_execution_config(config)?;

    let data = VecBarSource::new(bars);
    let stage_start = Instant::now();
    let mut runner = BacktestRunner::new_with_execution(
        config.run.run_id.clone(),
        strategy,
        data,
        risk_limits,
        config.run.initial_capital,
        metrics_config,
        config.costs.fee_bps,
        config.run.symbol.clone(),
        size_mode,
        execution.clone(),
    );
    let results = runner.run();
    let engine_ms = stage_start.elapsed().as_millis() as f64;
    metrics::histogram!("kairos.backtest.engine_ms").record(engine_ms);
    metrics::gauge!("kairos.backtest.bars_processed").set(results.summary.bars_processed as f64);
    metrics::gauge!("kairos.backtest.trades").set(results.summary.trades as f64);
    metrics::gauge!("kairos.backtest.engine_bars_per_sec").set(if engine_ms > 0.0 {
        (results.summary.bars_processed as f64) / (engine_ms / 1000.0)
    } else {
        0.0
    });
    audit_extras.push(timing_event(
        &config.run.run_id,
        0,
        "timing",
        Some(&config.run.symbol),
        "run_engine",
        stage_start.elapsed().as_millis() as u64,
        serde_json::json!({}),
    ));

    write_outputs(
        config,
        config_toml,
        out,
        results,
        &execution,
        artifacts,
        audit_extras,
    )
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

fn write_outputs(
    config: &Config,
    config_toml: &str,
    out: Option<PathBuf>,
    results: BacktestResults,
    execution: &kairos_domain::services::engine::execution::ExecutionConfig,
    artifacts: &dyn ArtifactWriter,
    mut audit_extras: Vec<AuditEvent>,
) -> Result<PathBuf, String> {
    let base_dir = out.unwrap_or_else(|| PathBuf::from(&config.paths.out_dir));
    let run_dir = base_dir.join(&config.run.run_id);
    artifacts.ensure_dir(&run_dir)?;

    artifacts.write_trades_csv(run_dir.join("trades.csv").as_path(), &results.trades)?;
    artifacts.write_equity_csv(run_dir.join("equity.csv").as_path(), &results.equity)?;
    let meta = summary_meta_json_from_equity(config, &results.equity);
    let config_snapshot = config_snapshot_json(config, execution);
    artifacts.write_summary_json(
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
    artifacts.write_audit_jsonl(run_dir.join("logs.jsonl").as_path(), &audit_events)?;

    if config
        .report
        .as_ref()
        .and_then(|report| report.html)
        .unwrap_or(false)
    {
        artifacts.write_summary_html(
            run_dir.join("summary.html").as_path(),
            &results.summary,
            meta.as_ref(),
        )?;
        artifacts.write_dashboard_html(
            run_dir.join("dashboard.html").as_path(),
            &results.summary,
            meta.as_ref(),
            &results.trades,
            &results.equity,
        )?;
    }

    artifacts
        .write_config_snapshot_toml(run_dir.join("config_snapshot.toml").as_path(), config_toml)?;

    Ok(run_dir)
}
