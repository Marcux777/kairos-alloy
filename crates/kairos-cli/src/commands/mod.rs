use crate::config::{load_config, load_config_with_source, AgentMode, Config};
use kairos_application::meta::engine_name;
use kairos_domain::entities::metrics::MetricsConfig;
use kairos_domain::entities::risk::RiskLimits;
use kairos_domain::services::audit::AuditEvent;
use kairos_domain::services::engine::backtest::BacktestRunner;
use kairos_domain::services::features;
use kairos_domain::services::market_data_source::VecBarSource;
use kairos_domain::services::strategy::BuyAndHold;
use kairos_domain::value_objects::equity_point::EquityPoint;
use kairos_infrastructure::agents::AgentClient as InfraAgentClient;
use kairos_infrastructure::artifacts::FilesystemArtifactWriter;
use kairos_infrastructure::persistence::postgres_ohlcv::PostgresMarketDataRepository;
use kairos_infrastructure::reporting::{
    read_equity_csv, read_trades_csv, recompute_summary, write_audit_jsonl, write_summary_html,
    write_summary_json, SummaryMeta,
};
use kairos_infrastructure::sentiment::FilesystemSentimentRepository;
use std::env;
use std::path::PathBuf;
use std::time::Instant;

mod backtest;
mod bench;
mod paper;
mod report;
mod validate;

pub enum Command {
    Backtest {
        config: PathBuf,
        out: Option<PathBuf>,
    },
    Bench {
        bars: usize,
        step_seconds: i64,
        mode: String,
        json: bool,
    },
    Paper {
        config: PathBuf,
        out: Option<PathBuf>,
    },
    Validate {
        config: PathBuf,
        strict: bool,
        out: Option<PathBuf>,
    },
    Report {
        input: PathBuf,
    },
}

pub fn run(command: Command) -> Result<(), String> {
    match command {
        Command::Backtest { config, out } => backtest::run_backtest(config, out),
        Command::Bench {
            bars,
            step_seconds,
            mode,
            json,
        } => bench::run_bench(bars, step_seconds, mode, json),
        Command::Paper { config, out } => paper::run_paper(config, out),
        Command::Validate {
            config,
            strict,
            out,
        } => validate::run_validate(config, strict, out),
        Command::Report { input } => report::run_report(input),
    }
}

fn resolve_db_url(config: &Config) -> Result<String, String> {
    match config.db.url.as_deref() {
        Some(url) if !url.trim().is_empty() => Ok(url.to_string()),
        _ => env::var("KAIROS_DB_URL")
            .map_err(|_| "missing db.url in config and env KAIROS_DB_URL is not set".to_string()),
    }
}

fn run_validate(config_path: PathBuf, strict: bool, out: Option<PathBuf>) -> Result<(), String> {
    let (config, _config_toml) = load_config_with_source(&config_path)?;
    print_config_summary("validate", &config, None)?;

    let db_url = resolve_db_url(&config)?;
    let market_data = PostgresMarketDataRepository::new(db_url, config.db.ohlcv_table.to_string());
    let sentiment_repo = FilesystemSentimentRepository;

    let report =
        kairos_application::validation::validate(&config, strict, &market_data, &sentiment_repo)?;

    if let Some(out_path) = out {
        std::fs::write(&out_path, report.to_string())
            .map_err(|err| format!("failed to write report {}: {}", out_path.display(), err))?;
    }

    Ok(())
}

fn run_backtest(config_path: PathBuf, out: Option<PathBuf>) -> Result<(), String> {
    let (config, config_toml) = load_config_with_source(&config_path)?;
    print_config_summary("backtest", &config, out.as_ref())?;

    let overall_start = Instant::now();
    let db_url = resolve_db_url(&config)?;
    let market_data = PostgresMarketDataRepository::new(db_url, config.db.ohlcv_table.to_string());
    let sentiment_repo = FilesystemSentimentRepository;
    let artifacts = FilesystemArtifactWriter::new();

    let remote_agent: Option<Box<dyn kairos_domain::repositories::agent::AgentClient>> =
        match config.agent.mode {
            AgentMode::Remote => {
                let agent = InfraAgentClient::new(
                    config.agent.url.clone(),
                    config.agent.timeout_ms,
                    config.agent.api_version.clone(),
                    config.agent.feature_version.clone(),
                    config.agent.retries,
                    config.agent.fallback_action,
                )
                .map_err(|err| {
                    format!(
                        "failed to init remote agent client (url={}): {err}",
                        config.agent.url
                    )
                })?;
                Some(Box::new(agent))
            }
            _ => None,
        };

    let run_dir = kairos_application::backtesting::run_backtest(
        &config,
        &config_toml,
        out,
        &market_data,
        &sentiment_repo,
        &artifacts,
        remote_agent,
    )?;

    println!("run output: {}", run_dir.display());
    println!(
        "{} cli: backtest total_ms={}",
        engine_name(),
        overall_start.elapsed().as_millis()
    );
    Ok(())
}

fn run_paper(config_path: PathBuf, out: Option<PathBuf>) -> Result<(), String> {
    let (config, config_toml) = load_config_with_source(&config_path)?;
    print_config_summary("paper", &config, out.as_ref())?;

    let overall_start = Instant::now();
    let db_url = resolve_db_url(&config)?;
    let market_data = PostgresMarketDataRepository::new(db_url, config.db.ohlcv_table.to_string());
    let sentiment_repo = FilesystemSentimentRepository;
    let artifacts = FilesystemArtifactWriter::new();

    let remote_agent: Option<Box<dyn kairos_domain::repositories::agent::AgentClient>> =
        match config.agent.mode {
            AgentMode::Remote => {
                let agent = InfraAgentClient::new(
                    config.agent.url.clone(),
                    config.agent.timeout_ms,
                    config.agent.api_version.clone(),
                    config.agent.feature_version.clone(),
                    config.agent.retries,
                    config.agent.fallback_action,
                )
                .map_err(|err| {
                    format!(
                        "failed to init remote agent client (url={}): {err}",
                        config.agent.url
                    )
                })?;
                Some(Box::new(agent))
            }
            _ => None,
        };

    let run_dir = kairos_application::paper_trading::run_paper(
        &config,
        &config_toml,
        out,
        &market_data,
        &sentiment_repo,
        &artifacts,
        remote_agent,
    )?;

    println!("run output: {}", run_dir.display());
    println!(
        "{} cli: paper total_ms={}",
        engine_name(),
        overall_start.elapsed().as_millis()
    );
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
                let execution = resolve_execution_config(&config)?;
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
                    "execution": {
                        "model": match execution.model {
                            kairos_domain::services::engine::execution::ExecutionModel::Simple => "simple",
                            kairos_domain::services::engine::execution::ExecutionModel::Complete => "complete",
                        },
                        "latency_bars": execution.latency_bars,
                        "buy_kind": format!("{:?}", execution.buy_kind).to_lowercase(),
                        "sell_kind": format!("{:?}", execution.sell_kind).to_lowercase(),
                        "price_reference": match execution.price_reference {
                            kairos_domain::services::engine::execution::PriceReference::Close => "close",
                            kairos_domain::services::engine::execution::PriceReference::Open => "open",
                        },
                        "limit_offset_bps": execution.limit_offset_bps,
                        "stop_offset_bps": execution.stop_offset_bps,
                        "spread_bps": execution.spread_bps,
                        "max_fill_pct_of_volume": execution.max_fill_pct_of_volume,
                        "tif": format!("{:?}", execution.tif).to_lowercase(),
                        "expire_after_bars": execution.expire_after_bars,
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
            Err(_) => (None, None, false, "unknown".to_string()),
        }
    } else {
        (None, None, false, "unknown".to_string())
    };

    write_summary_json(
        input.join("summary.json").as_path(),
        &summary,
        meta.as_ref(),
        config_snapshot.as_ref(),
    )?;
    if report_html {
        write_summary_html(
            input.join("summary.html").as_path(),
            &summary,
            meta.as_ref(),
        )?;
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

fn print_config_summary(
    command: &str,
    config: &Config,
    out: Option<&PathBuf>,
) -> Result<(), String> {
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
        config
            .db
            .url
            .as_deref()
            .unwrap_or("$KAIROS_DB_URL"),
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
        config.risk.max_position_qty, config.risk.max_drawdown_pct, config.risk.max_exposure_pct
    );
    println!(
        "orders: size_mode={}",
        config
            .orders
            .as_ref()
            .and_then(|orders| orders.size_mode.as_deref())
            .unwrap_or("qty")
    );

    let exec = resolve_execution_config(config)?;
    println!(
        "execution: model={} latency_bars={} buy_kind={} sell_kind={} tif={} max_fill_pct_of_volume={} spread_bps={} slippage_bps={}",
        match exec.model {
            kairos_domain::services::engine::execution::ExecutionModel::Simple => "simple",
            kairos_domain::services::engine::execution::ExecutionModel::Complete => "complete",
        },
        exec.latency_bars,
        format!("{:?}", exec.buy_kind).to_lowercase(),
        format!("{:?}", exec.sell_kind).to_lowercase(),
        format!("{:?}", exec.tif).to_lowercase(),
        exec.max_fill_pct_of_volume,
        exec.spread_bps,
        exec.slippage_bps
    );
    println!(
        "features: return_mode={}, sma_windows={:?}, rsi_enabled={}, sentiment_lag={}, sentiment_missing={}",
        match config.features.return_mode {
            features::ReturnMode::Log => "log",
            features::ReturnMode::Pct => "pct",
        },
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
        match config.agent.mode {
            AgentMode::Remote => "remote",
            AgentMode::Baseline => "baseline",
            AgentMode::Hold => "hold",
        },
        config.agent.url,
        config.agent.timeout_ms,
        config.agent.retries,
        match config.agent.fallback_action {
            kairos_domain::value_objects::action_type::ActionType::Buy => "BUY",
            kairos_domain::value_objects::action_type::ActionType::Sell => "SELL",
            kairos_domain::value_objects::action_type::ActionType::Hold => "HOLD",
        },
        config.agent.api_version,
        config.agent.feature_version
    );
    if let Some(out_dir) = out {
        println!("output dir: {}", out_dir.display());
    }

    Ok(())
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

fn resolve_execution_config(
    config: &Config,
) -> Result<kairos_domain::services::engine::execution::ExecutionConfig, String> {
    use kairos_domain::services::engine::execution as core_exec;

    let slippage_bps = config.costs.slippage_bps;
    if !slippage_bps.is_finite() || slippage_bps < 0.0 {
        return Err("costs.slippage_bps must be finite and >= 0".to_string());
    }

    let Some(exec) = config.execution.as_ref() else {
        return Ok(core_exec::ExecutionConfig::simple(slippage_bps));
    };

    let model = exec
        .model
        .as_deref()
        .unwrap_or("simple")
        .trim()
        .to_lowercase();

    let mut cfg = match model.as_str() {
        "simple" => core_exec::ExecutionConfig::simple(slippage_bps),
        "complete" => core_exec::ExecutionConfig::complete_defaults(slippage_bps),
        _ => return Err("execution.model must be: simple | complete".to_string()),
    };

    cfg.model = match model.as_str() {
        "simple" => core_exec::ExecutionModel::Simple,
        "complete" => core_exec::ExecutionModel::Complete,
        _ => cfg.model,
    };

    if let Some(latency_bars) = exec.latency_bars {
        cfg.latency_bars = latency_bars.max(1);
    }

    if let Some(value) = exec.buy_kind.as_deref() {
        cfg.buy_kind = match value.trim().to_lowercase().as_str() {
            "market" => core_exec::OrderKind::Market,
            "limit" => core_exec::OrderKind::Limit,
            "stop" => core_exec::OrderKind::Stop,
            _ => return Err("execution.buy_kind must be: market | limit | stop".to_string()),
        };
    }

    if let Some(value) = exec.sell_kind.as_deref() {
        cfg.sell_kind = match value.trim().to_lowercase().as_str() {
            "market" => core_exec::OrderKind::Market,
            "limit" => core_exec::OrderKind::Limit,
            "stop" => core_exec::OrderKind::Stop,
            _ => return Err("execution.sell_kind must be: market | limit | stop".to_string()),
        };
    }

    if let Some(value) = exec.price_reference.as_deref() {
        cfg.price_reference = match value.trim().to_lowercase().as_str() {
            "close" => core_exec::PriceReference::Close,
            "open" => core_exec::PriceReference::Open,
            _ => return Err("execution.price_reference must be: close | open".to_string()),
        };
    }

    if let Some(value) = exec.limit_offset_bps {
        cfg.limit_offset_bps = value;
    }

    if let Some(value) = exec.stop_offset_bps {
        cfg.stop_offset_bps = value;
    }

    if let Some(value) = exec.spread_bps {
        cfg.spread_bps = value;
    }

    if let Some(value) = exec.max_fill_pct_of_volume {
        cfg.max_fill_pct_of_volume = value;
    }

    if let Some(value) = exec.tif.as_deref() {
        cfg.tif = match value.trim().to_lowercase().as_str() {
            "ioc" => core_exec::TimeInForce::Ioc,
            "gtc" => core_exec::TimeInForce::Gtc,
            _ => return Err("execution.tif must be: ioc | gtc".to_string()),
        };
    }

    if let Some(value) = exec.expire_after_bars {
        cfg.expire_after_bars = Some(value.max(1));
    }

    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::{run_backtest, run_validate};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_tmp_dir(prefix: &str) -> PathBuf {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("kairos_{prefix}_{}_{}", std::process::id(), now))
    }

    fn write_file(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::write(path, contents).expect("write file");
    }

    fn sample_config(tmp_dir: &Path, db_url: &str) -> PathBuf {
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
        let parse = kairos_domain::value_objects::timeframe::parse_duration_like_seconds;
        assert_eq!(parse("5s").unwrap(), 5);
        assert_eq!(parse("2m").unwrap(), 120);
        assert_eq!(parse("1h").unwrap(), 3600);
        assert_eq!(parse("1min").unwrap(), 60);
    }

    #[test]
    fn normalize_timeframe_label_handles_aliases() {
        let parse = kairos_domain::value_objects::timeframe::Timeframe::parse;
        assert_eq!(parse("1m").unwrap().label, "1min");
        assert_eq!(parse("1hour").unwrap().label, "1hour");
        assert_eq!(parse("1d").unwrap().label, "1day");
    }

    #[test]
    fn run_validate_reads_postgres() {
        if std::env::var("KAIROS_DB_RUN_TESTS").ok().as_deref() != Some("1") {
            return;
        }
        let db_url = std::env::var("KAIROS_DB_URL").expect("KAIROS_DB_URL must be set");
        let tmp_dir = unique_tmp_dir("cli_validate");
        let config_path = sample_config(&tmp_dir, &db_url);
        run_validate(config_path, false, None).expect("validate");
    }

    #[test]
    fn run_backtest_writes_outputs() {
        if std::env::var("KAIROS_DB_RUN_TESTS").ok().as_deref() != Some("1") {
            return;
        }
        let db_url = std::env::var("KAIROS_DB_URL").expect("KAIROS_DB_URL must be set");
        let tmp_dir = unique_tmp_dir("cli_backtest");
        let config_path = sample_config(&tmp_dir, &db_url);
        run_backtest(config_path.clone(), None).expect("backtest");
        let run_dir = tmp_dir.join("test_run");
        assert!(run_dir.join("summary.json").exists());
        assert!(run_dir.join("trades.csv").exists());
        assert!(run_dir.join("equity.csv").exists());
        assert!(run_dir.join("config_snapshot.toml").exists());
    }
}
