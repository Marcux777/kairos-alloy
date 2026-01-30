use crate::config::{AgentMode, Config};
use kairos_application::meta::engine_name;
use kairos_domain::services::features;
use std::path::PathBuf;

pub(super) fn print_config_summary(
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
        config.db.url.as_deref().unwrap_or("$KAIROS_DB_URL"),
        config.db.ohlcv_table,
        config.db.exchange,
        config.db.market,
        config.db.source_timeframe.as_deref().unwrap_or("same_as_run"),
        config.paths.sentiment_path.as_deref().unwrap_or("none"),
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
