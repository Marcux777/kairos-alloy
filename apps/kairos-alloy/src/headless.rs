use kairos_domain::repositories::agent::AgentClient as AgentPort;
use kairos_domain::repositories::market_data::MarketDataRepository;
use kairos_domain::repositories::sentiment::SentimentRepository;
use kairos_domain::services::ohlcv::{data_quality_from_bars, resample_bars};
use kairos_domain::value_objects::timeframe::Timeframe;
use kairos_infrastructure::agents::AgentClient as InfraAgentClient;
use kairos_infrastructure::artifacts::{FilesystemArtifactReader, FilesystemArtifactWriter};
use kairos_infrastructure::persistence::postgres_ohlcv::PostgresMarketDataRepository;
use kairos_infrastructure::sentiment::FilesystemSentimentRepository;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeadlessMode {
    Validate,
    Backtest,
    Paper,
    Report,
    Sweep,
    Cpcv,
}

pub struct HeadlessArgs {
    pub mode: HeadlessMode,
    pub config_path: Option<PathBuf>,
    pub strict: bool,
    pub run_dir: Option<PathBuf>,
    pub sweep_config: Option<PathBuf>,
    pub cpcv_out: Option<PathBuf>,
    pub cpcv_n_groups: usize,
    pub cpcv_k_test: usize,
    pub cpcv_horizon_bars: usize,
    pub cpcv_purge_bars: usize,
    pub cpcv_embargo_bars: usize,
    pub cpcv_start: Option<String>,
    pub cpcv_end: Option<String>,
}

pub fn run_headless(args: HeadlessArgs) -> Result<serde_json::Value, String> {
    match args.mode {
        HeadlessMode::Sweep => run_sweep(args.sweep_config.as_deref()),
        mode => {
            let config_path = args
                .config_path
                .as_deref()
                .ok_or_else(|| "--config is required for this mode".to_string())?;
            let (config, config_toml) =
                kairos_application::config::load_config_with_source(config_path)?;
            match mode {
                HeadlessMode::Validate => run_validate(&config, args.strict),
                HeadlessMode::Backtest => run_backtest(&config, &config_toml),
                HeadlessMode::Paper => run_paper(&config, &config_toml),
                HeadlessMode::Report => run_report(&config, args.run_dir.as_deref()),
                HeadlessMode::Sweep => unreachable!("handled above"),
                HeadlessMode::Cpcv => run_cpcv(&config, &args),
            }
        }
    }
}

fn resolve_db_url(config: &kairos_application::config::Config) -> Result<String, String> {
    match config.db.url.as_deref() {
        Some(url) if !url.trim().is_empty() => Ok(url.to_string()),
        _ => std::env::var("KAIROS_DB_URL")
            .map_err(|_| "missing db.url in config and env KAIROS_DB_URL is not set".to_string()),
    }
}

fn build_market_data_repo(
    config: &kairos_application::config::Config,
) -> Result<Box<dyn MarketDataRepository>, String> {
    let db_url = resolve_db_url(config)?;
    let pool_max_size = config.db.pool_max_size.unwrap_or(8);
    Ok(Box::new(PostgresMarketDataRepository::new(
        db_url,
        config.db.ohlcv_table.to_string(),
        pool_max_size,
    )?))
}

fn build_sentiment_repo() -> Box<dyn SentimentRepository> {
    Box::new(FilesystemSentimentRepository)
}

fn build_remote_agent(
    config: &kairos_application::config::Config,
) -> Result<Option<Box<dyn AgentPort>>, String> {
    match config.agent.mode {
        kairos_application::config::AgentMode::Remote => {
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
            Ok(Some(Box::new(agent)))
        }
        _ => Ok(None),
    }
}

fn artifacts_for_run(run_dir: &Path) -> serde_json::Value {
    serde_json::json!({
        "run_dir": run_dir.display().to_string(),
        "trades_csv": run_dir.join("trades.csv").display().to_string(),
        "equity_csv": run_dir.join("equity.csv").display().to_string(),
        "summary_json": run_dir.join("summary.json").display().to_string(),
        "logs_jsonl": run_dir.join("logs.jsonl").display().to_string(),
        "config_snapshot_toml": run_dir.join("config_snapshot.toml").display().to_string(),
        "summary_html": run_dir.join("summary.html").display().to_string(),
        "dashboard_html": run_dir.join("dashboard.html").display().to_string(),
        "analyzers_dir": run_dir.join("analyzers").display().to_string(),
    })
}

fn run_validate(
    config: &kairos_application::config::Config,
    strict: bool,
) -> Result<serde_json::Value, String> {
    let market_data = build_market_data_repo(config)?;
    let sentiment_repo = build_sentiment_repo();
    let report = kairos_application::validation::validate(
        config,
        strict,
        market_data.as_ref(),
        sentiment_repo.as_ref(),
    )?;
    Ok(serde_json::json!({
        "status": "ok",
        "mode": "validate",
        "strict": strict,
        "run_id": config.run.run_id,
        "out_dir": config.paths.out_dir,
        "report": report,
    }))
}

fn run_backtest(
    config: &kairos_application::config::Config,
    config_toml: &str,
) -> Result<serde_json::Value, String> {
    let market_data = build_market_data_repo(config)?;
    let sentiment_repo = build_sentiment_repo();
    let artifacts = FilesystemArtifactWriter::new();
    let remote_agent = build_remote_agent(config)?;

    let run_dir = kairos_application::backtesting::run_backtest(
        config,
        config_toml,
        None,
        market_data.as_ref(),
        sentiment_repo.as_ref(),
        &artifacts,
        remote_agent,
    )?;
    Ok(serde_json::json!({
        "status": "ok",
        "mode": "backtest",
        "run_id": config.run.run_id,
        "out_dir": config.paths.out_dir,
        "artifacts": artifacts_for_run(&run_dir),
    }))
}

fn run_paper(
    config: &kairos_application::config::Config,
    config_toml: &str,
) -> Result<serde_json::Value, String> {
    let market_data = build_market_data_repo(config)?;
    let sentiment_repo = build_sentiment_repo();
    let artifacts = FilesystemArtifactWriter::new();
    let remote_agent = build_remote_agent(config)?;

    let run_dir = kairos_application::paper_trading::run_paper(
        config,
        config_toml,
        None,
        market_data.as_ref(),
        sentiment_repo.as_ref(),
        &artifacts,
        remote_agent,
    )?;
    Ok(serde_json::json!({
        "status": "ok",
        "mode": "paper",
        "run_id": config.run.run_id,
        "out_dir": config.paths.out_dir,
        "artifacts": artifacts_for_run(&run_dir),
    }))
}

fn run_report(
    config: &kairos_application::config::Config,
    run_dir: Option<&Path>,
) -> Result<serde_json::Value, String> {
    let input_dir = run_dir
        .map(|p| p.to_path_buf())
        .ok_or_else(|| "--run-dir is required for --mode report".to_string())?;

    let reader = FilesystemArtifactReader::new();
    let writer = FilesystemArtifactWriter::new();
    let result =
        kairos_application::reporting::generate_report(input_dir.as_path(), &reader, &writer)?;

    Ok(serde_json::json!({
        "status": "ok",
        "mode": "report",
        "run_id": result.run_id,
        "out_dir": config.paths.out_dir,
        "input_dir": result.input_dir.display().to_string(),
        "wrote_html": result.wrote_html,
        "summary": {
            "bars_processed": result.summary.bars_processed,
            "trades": result.summary.trades,
            "win_rate": result.summary.win_rate,
            "net_profit": result.summary.net_profit,
            "sharpe": result.summary.sharpe,
            "max_drawdown": result.summary.max_drawdown,
        },
    }))
}

fn run_sweep(sweep_config: Option<&Path>) -> Result<serde_json::Value, String> {
    let sweep_path = sweep_config
        .map(|p| p.to_path_buf())
        .ok_or_else(|| "--sweep-config is required for --mode sweep".to_string())?;

    let raw = std::fs::read_to_string(&sweep_path).map_err(|err| {
        format!(
            "failed to read sweep config {}: {err}",
            sweep_path.display()
        )
    })?;
    let sweep_file: kairos_application::experiments::sweep::SweepFile = toml::from_str(&raw)
        .map_err(|err| format!("failed to parse sweep TOML {}: {err}", sweep_path.display()))?;

    let base_config_path = {
        let p = PathBuf::from(&sweep_file.base.config);
        if p.is_absolute() {
            p
        } else {
            sweep_path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(p)
        }
    };
    let (base_config, _toml) =
        kairos_application::config::load_config_with_source(base_config_path.as_path())?;

    let market_data = build_market_data_repo(&base_config)?;
    let sentiment_repo = build_sentiment_repo();
    let artifacts = FilesystemArtifactWriter::new();

    let mut agent_factory =
        |cfg: &kairos_application::config::Config| -> Result<Option<Box<dyn AgentPort>>, String> {
            build_remote_agent(cfg)
        };

    let result = kairos_application::experiments::sweep::run_sweep(
        sweep_path.as_path(),
        &mut agent_factory,
        market_data.as_ref(),
        sentiment_repo.as_ref(),
        &artifacts,
    )?;

    Ok(serde_json::json!({
        "status": "ok",
        "mode": "sweep",
        "sweep_id": result.sweep_id,
        "sweep_dir": result.sweep_dir.display().to_string(),
        "manifest_json": result.sweep_dir.join("manifest.json").display().to_string(),
        "results_csv": result.sweep_dir.join("results.csv").display().to_string(),
        "leaderboard_csv": result.sweep_dir.join("leaderboard.csv").display().to_string(),
        "runs_total": result.runs.len(),
    }))
}

fn run_cpcv(
    config: &kairos_application::config::Config,
    args: &HeadlessArgs,
) -> Result<serde_json::Value, String> {
    let market_data = build_market_data_repo(config)?;

    let timeframe = Timeframe::parse_or_seconds(&config.run.timeframe)?;
    let expected_step = timeframe.step_seconds;
    let timeframe_label = timeframe.label;

    let source_label = config
        .db
        .source_timeframe
        .as_deref()
        .unwrap_or(&timeframe_label);
    let source_timeframe = Timeframe::parse_or_seconds(source_label)?;
    let source_step = source_timeframe.step_seconds;
    let source_timeframe_label = source_timeframe.label;

    let (source_bars, _source_report) =
        market_data.load_ohlcv(&kairos_domain::repositories::market_data::OhlcvQuery {
            exchange: config.db.exchange.to_lowercase(),
            market: config.db.market.to_lowercase(),
            symbol: config.run.symbol.clone(),
            timeframe: source_timeframe_label.clone(),
            expected_step_seconds: Some(source_step),
        })?;

    let bars = if source_timeframe_label != timeframe_label {
        if source_step > expected_step {
            return Err(format!(
                "cannot resample OHLCV: source timeframe ({}) is larger than run timeframe ({})",
                source_timeframe_label, timeframe_label
            ));
        }
        resample_bars(&source_bars, expected_step)?
    } else {
        source_bars
    };

    let mut bars = bars;
    bars.sort_by_key(|b| b.timestamp);
    bars.dedup_by_key(|b| b.timestamp);

    let start = args
        .cpcv_start
        .as_deref()
        .map(parse_timestamp_seconds)
        .transpose()?;
    let end = args
        .cpcv_end
        .as_deref()
        .map(parse_timestamp_seconds)
        .transpose()?;
    let bars: Vec<kairos_domain::value_objects::bar::Bar> = bars
        .into_iter()
        .filter(|b| start.map(|s| b.timestamp >= s).unwrap_or(true))
        .filter(|b| end.map(|e| b.timestamp <= e).unwrap_or(true))
        .collect();

    let cfg = kairos_application::experiments::cpcv::CpcvConfig {
        n_groups: args.cpcv_n_groups,
        k_test: args.cpcv_k_test,
        horizon_bars: args.cpcv_horizon_bars,
        purge_bars: args.cpcv_purge_bars,
        embargo_bars: args.cpcv_embargo_bars,
    };
    let cpcv = kairos_application::experiments::cpcv::generate_cpcv(&bars, cfg)?;

    let out_path = args.cpcv_out.clone().unwrap_or_else(|| {
        PathBuf::from(&config.paths.out_dir)
            .join("cpcv")
            .join(format!("{}__cpcv.csv", config.run.run_id))
    });
    kairos_application::experiments::cpcv::write_cpcv_csv(out_path.as_path(), &cpcv)?;

    let report = data_quality_from_bars(&bars, Some(expected_step));

    Ok(serde_json::json!({
        "status": "ok",
        "mode": "cpcv",
        "run_id": config.run.run_id,
        "symbol": config.run.symbol,
        "timeframe": config.run.timeframe,
        "source_timeframe": source_timeframe_label,
        "rows": bars.len(),
        "folds": cpcv.folds.len(),
        "out_csv": out_path.display().to_string(),
        "data_quality": {
            "duplicates": report.duplicates,
            "gaps": report.gaps,
            "out_of_order": report.out_of_order,
            "invalid_close": report.invalid_close,
        },
        "cpcv": {
            "n_groups": cfg.n_groups,
            "k_test": cfg.k_test,
            "horizon_bars": cfg.horizon_bars,
            "purge_bars": cfg.purge_bars,
            "embargo_bars": cfg.embargo_bars,
        }
    }))
}

fn parse_timestamp_seconds(raw: &str) -> Result<i64, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("timestamp cannot be empty".to_string());
    }
    if let Ok(v) = trimmed.parse::<i64>() {
        return Ok(v);
    }
    let dt = chrono::DateTime::parse_from_rfc3339(trimmed)
        .map_err(|err| format!("invalid timestamp (expected epoch seconds or RFC3339): {err}"))?;
    Ok(dt.timestamp())
}
