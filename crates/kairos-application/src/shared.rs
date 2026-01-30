use crate::config::Config;
use kairos_domain::entities::metrics::MetricsConfig;
use kairos_domain::services::engine::backtest::OrderSizeMode;
use kairos_domain::services::engine::execution as core_exec;
use kairos_domain::services::sentiment::MissingValuePolicy;
use kairos_domain::value_objects::equity_point::EquityPoint;

pub fn parse_duration_like(value: &str) -> Result<i64, String> {
    kairos_domain::value_objects::timeframe::parse_duration_like_seconds(value)
}

pub fn normalize_timeframe_label(value: &str) -> Result<String, String> {
    kairos_domain::value_objects::timeframe::Timeframe::parse(value).map(|tf| tf.label)
}

pub fn resolve_size_mode(config: &Config) -> OrderSizeMode {
    match config
        .orders
        .as_ref()
        .and_then(|orders| orders.size_mode.as_deref())
        .map(|s| s.trim().to_lowercase())
        .as_deref()
    {
        Some("pct_equity") | Some("equity_pct") | Some("pct") => OrderSizeMode::PctEquity,
        _ => OrderSizeMode::Quantity,
    }
}

pub fn resolve_execution_config(config: &Config) -> Result<core_exec::ExecutionConfig, String> {
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

pub fn build_metrics_config(config: &Config) -> MetricsConfig {
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

pub fn resolve_sma_windows(config: &Config) -> (usize, usize) {
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

pub fn resolve_sentiment_missing_policy(config: &Config) -> MissingValuePolicy {
    match config
        .features
        .sentiment_missing
        .as_deref()
        .unwrap_or("error")
        .trim()
        .to_lowercase()
        .as_str()
    {
        "zero" | "zero_fill" | "zero-fill" => MissingValuePolicy::ZeroFill,
        "forward" | "forward_fill" | "forward-fill" => MissingValuePolicy::ForwardFill,
        "drop" | "drop_row" => MissingValuePolicy::DropRow,
        _ => MissingValuePolicy::Error,
    }
}

pub fn summary_meta_json_from_equity(
    config: &Config,
    equity: &[EquityPoint],
) -> Option<serde_json::Value> {
    let start = equity.first()?.timestamp;
    let end = equity.last()?.timestamp;
    Some(serde_json::json!({
        "run_id": config.run.run_id,
        "symbol": config.run.symbol,
        "timeframe": config.run.timeframe,
        "start": start,
        "end": end,
    }))
}

pub fn config_snapshot_json(
    config: &Config,
    execution: &core_exec::ExecutionConfig,
) -> serde_json::Value {
    serde_json::json!({
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
                core_exec::ExecutionModel::Simple => "simple",
                core_exec::ExecutionModel::Complete => "complete",
            },
            "latency_bars": execution.latency_bars,
            "buy_kind": format!("{:?}", execution.buy_kind).to_lowercase(),
            "sell_kind": format!("{:?}", execution.sell_kind).to_lowercase(),
            "price_reference": match execution.price_reference {
                core_exec::PriceReference::Close => "close",
                core_exec::PriceReference::Open => "open",
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
    })
}
