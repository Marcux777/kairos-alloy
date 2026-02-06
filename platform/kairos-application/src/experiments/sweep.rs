use crate::config::Config;
use crate::shared::{normalize_timeframe_label, parse_duration_like};
use kairos_domain::repositories::agent::AgentClient as AgentPort;
use kairos_domain::repositories::artifacts::ArtifactWriter;
use kairos_domain::repositories::market_data::{MarketDataRepository, OhlcvQuery};
use kairos_domain::repositories::sentiment::SentimentRepository;
use kairos_domain::services::ohlcv::data_quality_from_bars;
use kairos_domain::value_objects::bar::Bar;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SweepMode {
    Backtest,
    Paper,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SweepFile {
    pub base: SweepBase,
    pub sweep: SweepMeta,
    #[serde(default)]
    pub params: Vec<SweepParam>,
    pub leaderboard: Option<LeaderboardConfig>,
    pub splits: Option<Vec<SweepSplit>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SweepBase {
    pub config: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SweepMeta {
    pub id: String,
    pub mode: SweepMode,
    pub parallelism: Option<usize>,
    pub resume: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SweepParam {
    pub path: String,
    pub values: Vec<toml::Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LeaderboardConfig {
    pub sort_by: Option<String>,
    pub descending: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SweepSplit {
    pub id: String,
    pub start: Option<String>,
    pub end: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SweepRunEntry {
    pub run_id: String,
    pub split_id: String,
    pub params: BTreeMap<String, toml::Value>,
    pub status: String,
    pub error: Option<String>,
    pub metrics: Option<RunMetrics>,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct RunMetrics {
    pub bars_processed: u64,
    pub trades: u64,
    pub win_rate: f64,
    pub net_profit: f64,
    pub sharpe: f64,
    pub max_drawdown: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SweepResult {
    pub sweep_id: String,
    pub sweep_dir: PathBuf,
    pub mode: SweepMode,
    pub base_config: String,
    pub runs: Vec<SweepRunEntry>,
}

#[derive(Debug, Clone)]
pub struct SweepProgress {
    pub total_runs: usize,
    pub completed_runs: usize,
    pub ok_runs: usize,
    pub skipped_runs: usize,
    pub error_runs: usize,
    pub last_run_id: Option<String>,
    pub last_error: Option<String>,
}

pub type AgentFactoryResult = Result<Option<Box<dyn AgentPort>>, String>;
pub type AgentFactory<'a> = dyn Fn(&Config) -> AgentFactoryResult + Sync + 'a;

pub fn run_sweep(
    sweep_path: &Path,
    agent_factory: &AgentFactory<'_>,
    market_data: &dyn MarketDataRepository,
    sentiment_repo: &(dyn SentimentRepository + Sync),
    artifacts: &(dyn ArtifactWriter + Sync),
) -> Result<SweepResult, String> {
    run_sweep_with_hooks(
        sweep_path,
        agent_factory,
        market_data,
        sentiment_repo,
        artifacts,
        None,
        None,
    )
}

pub fn run_sweep_with_hooks(
    sweep_path: &Path,
    agent_factory: &AgentFactory<'_>,
    market_data: &dyn MarketDataRepository,
    sentiment_repo: &(dyn SentimentRepository + Sync),
    artifacts: &(dyn ArtifactWriter + Sync),
    mut on_progress: Option<&mut dyn FnMut(SweepProgress)>,
    should_cancel: Option<&(dyn Fn() -> bool + Sync)>,
) -> Result<SweepResult, String> {
    let raw = std::fs::read_to_string(sweep_path).map_err(|err| {
        format!(
            "failed to read sweep config {}: {err}",
            sweep_path.display()
        )
    })?;
    let sweep: SweepFile = toml::from_str(&raw)
        .map_err(|err| format!("failed to parse sweep TOML {}: {err}", sweep_path.display()))?;

    validate_param_paths(&sweep.params)?;

    let base_config_path = resolve_base_config_path(sweep_path, &sweep.base.config);
    let (base_config, base_toml_str) =
        crate::config::load_config_with_source(base_config_path.as_path())?;
    let base_toml_value: toml::Value = toml::from_str(&base_toml_str)
        .map_err(|err| format!("failed to parse base config TOML as value: {err}"))?;

    let out_dir = PathBuf::from(&base_config.paths.out_dir);
    let sweep_dir = out_dir.join("sweeps").join(&sweep.sweep.id);
    std::fs::create_dir_all(&sweep_dir)
        .map_err(|err| format!("failed to create sweep dir {}: {err}", sweep_dir.display()))?;

    let resume = sweep.sweep.resume.unwrap_or(false);
    let splits = sweep.splits.clone().unwrap_or_else(|| {
        vec![SweepSplit {
            id: "full".to_string(),
            start: None,
            end: None,
        }]
    });

    let timeframe_label = normalize_timeframe_label(&base_config.run.timeframe)?;
    let source_timeframe_label = normalize_timeframe_label(
        base_config
            .db
            .source_timeframe
            .as_deref()
            .unwrap_or(&timeframe_label),
    )?;
    let source_step = parse_duration_like(&source_timeframe_label)?;

    let (source_bars, _source_report) = market_data.load_ohlcv(&OhlcvQuery {
        exchange: base_config.db.exchange.to_lowercase(),
        market: base_config.db.market.to_lowercase(),
        symbol: base_config.run.symbol.clone(),
        timeframe: source_timeframe_label.clone(),
        expected_step_seconds: Some(source_step),
    })?;

    let mut runs: Vec<SweepRunEntry> = Vec::new();
    let grid = expand_grid(&sweep.params);
    let requested_parallelism = normalize_parallelism(sweep.sweep.parallelism);
    let total_runs = grid.len().saturating_mul(splits.len());
    let mut progress = SweepProgress {
        total_runs,
        completed_runs: 0,
        ok_runs: 0,
        skipped_runs: 0,
        error_runs: 0,
        last_run_id: None,
        last_error: None,
    };
    emit_progress(&mut on_progress, &progress);

    for split in &splits {
        if should_cancelled(should_cancel) {
            return Err("cancelled".to_string());
        }

        let (bars_for_split, report_for_split) =
            filter_bars_for_split(&source_bars, source_step, split)?;
        let in_memory_market = InMemoryMarketDataRepository {
            bars: bars_for_split,
            report: report_for_split,
        };

        let mut split_entries: Vec<Option<SweepRunEntry>> = vec![None; grid.len()];
        let mut plans: Vec<SweepRunPlan> = Vec::new();

        for (order_idx, assignment) in grid.iter().enumerate() {
            let mut toml_value = base_toml_value.clone();
            apply_assignment(&mut toml_value, assignment)?;

            let run_hash = assignment_hash(&split.id, assignment);
            let run_id = format!("{}__{}__{}", sweep.sweep.id, run_hash, split.id);
            set_run_id(&mut toml_value, &run_id)?;

            let config_toml = toml::to_string_pretty(&toml_value)
                .map_err(|err| format!("failed to serialize sweep config TOML: {err}"))?;
            let config: Config = toml::from_str(&config_toml)
                .map_err(|err| format!("failed to parse generated config TOML: {err}"))?;

            let run_dir = out_dir.join(&run_id);
            let summary_path = run_dir.join("summary.json");
            if resume && summary_path.exists() {
                let entry = SweepRunEntry {
                    run_id,
                    split_id: split.id.clone(),
                    params: assignment.clone(),
                    status: "skipped".to_string(),
                    error: None,
                    metrics: read_metrics_from_summary(&summary_path).ok(),
                };
                update_progress(&mut progress, &entry);
                emit_progress(&mut on_progress, &progress);
                split_entries[order_idx] = Some(entry);
                continue;
            }

            plans.push(SweepRunPlan {
                order_idx,
                run_id,
                split_id: split.id.clone(),
                params: assignment.clone(),
                config,
                config_toml,
                summary_path,
            });
        }

        let mut on_entry = |entry: &SweepRunEntry| {
            update_progress(&mut progress, entry);
            emit_progress(&mut on_progress, &progress);
        };

        let mut executed = if requested_parallelism <= 1 || plans.len() <= 1 {
            execute_plans_serial(
                &plans,
                sweep.sweep.mode,
                &in_memory_market,
                sentiment_repo,
                artifacts,
                agent_factory,
                should_cancel,
                &mut on_entry,
            )?
        } else {
            execute_plans_parallel(
                &plans,
                requested_parallelism,
                sweep.sweep.mode,
                &in_memory_market,
                sentiment_repo,
                artifacts,
                agent_factory,
                should_cancel,
                &mut on_entry,
            )?
        };

        executed.sort_by_key(|(order_idx, _)| *order_idx);
        for (order_idx, entry) in executed {
            split_entries[order_idx] = Some(entry);
        }

        for entry in split_entries {
            runs.push(entry.ok_or_else(|| {
                format!(
                    "internal sweep error: missing run entry for split '{}' (sweep '{}')",
                    split.id, sweep.sweep.id
                )
            })?);
        }
    }

    let result = SweepResult {
        sweep_id: sweep.sweep.id.clone(),
        sweep_dir: sweep_dir.clone(),
        mode: sweep.sweep.mode,
        base_config: base_config_path.display().to_string(),
        runs,
    };

    write_manifest(&sweep_dir, &result)?;
    write_results_csv(&sweep_dir, &result)?;
    write_leaderboard_csv(&sweep_dir, &result, sweep.leaderboard.as_ref())?;

    Ok(result)
}

#[derive(Debug, Clone)]
struct SweepRunPlan {
    order_idx: usize,
    run_id: String,
    split_id: String,
    params: BTreeMap<String, toml::Value>,
    config: Config,
    config_toml: String,
    summary_path: PathBuf,
}

enum WorkerMessage {
    Entry {
        order_idx: usize,
        entry: SweepRunEntry,
    },
    Fatal(String),
}

fn normalize_parallelism(value: Option<usize>) -> usize {
    value.unwrap_or(1).max(1)
}

fn execute_plans_serial(
    plans: &[SweepRunPlan],
    mode: SweepMode,
    market_data: &(dyn MarketDataRepository + Sync),
    sentiment_repo: &(dyn SentimentRepository + Sync),
    artifacts: &(dyn ArtifactWriter + Sync),
    agent_factory: &AgentFactory<'_>,
    should_cancel: Option<&(dyn Fn() -> bool + Sync)>,
    on_entry: &mut dyn FnMut(&SweepRunEntry),
) -> Result<Vec<(usize, SweepRunEntry)>, String> {
    let mut out = Vec::with_capacity(plans.len());
    for plan in plans {
        if should_cancelled(should_cancel) {
            return Err("cancelled".to_string());
        }
        let entry = execute_run_plan(
            plan,
            mode,
            market_data,
            sentiment_repo,
            artifacts,
            agent_factory,
        )?;
        on_entry(&entry);
        out.push((plan.order_idx, entry));
    }
    Ok(out)
}

fn execute_plans_parallel(
    plans: &[SweepRunPlan],
    parallelism: usize,
    mode: SweepMode,
    market_data: &(dyn MarketDataRepository + Sync),
    sentiment_repo: &(dyn SentimentRepository + Sync),
    artifacts: &(dyn ArtifactWriter + Sync),
    agent_factory: &AgentFactory<'_>,
    should_cancel: Option<&(dyn Fn() -> bool + Sync)>,
    on_entry: &mut dyn FnMut(&SweepRunEntry),
) -> Result<Vec<(usize, SweepRunEntry)>, String> {
    let worker_count = parallelism.max(1).min(plans.len());
    let next_index = AtomicUsize::new(0);
    let cancelled = AtomicBool::new(false);
    let (tx, rx) = mpsc::channel::<WorkerMessage>();

    std::thread::scope(|scope| {
        for _ in 0..worker_count {
            let tx = tx.clone();
            let next_index_ref = &next_index;
            let cancelled_ref = &cancelled;
            scope.spawn(move || loop {
                if cancelled_ref.load(Ordering::Relaxed) || should_cancelled(should_cancel) {
                    cancelled_ref.store(true, Ordering::Relaxed);
                    let _ = tx.send(WorkerMessage::Fatal("cancelled".to_string()));
                    break;
                }

                let plan_idx = next_index_ref.fetch_add(1, Ordering::Relaxed);
                if plan_idx >= plans.len() {
                    break;
                }

                match execute_run_plan(
                    &plans[plan_idx],
                    mode,
                    market_data,
                    sentiment_repo,
                    artifacts,
                    agent_factory,
                ) {
                    Ok(entry) => {
                        if tx
                            .send(WorkerMessage::Entry {
                                order_idx: plans[plan_idx].order_idx,
                                entry,
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(err) => {
                        cancelled_ref.store(true, Ordering::Relaxed);
                        let _ = tx.send(WorkerMessage::Fatal(err));
                        break;
                    }
                }
            });
        }

        drop(tx);

        let mut entries: Vec<(usize, SweepRunEntry)> = Vec::with_capacity(plans.len());
        let mut fatal_error: Option<String> = None;
        while let Ok(message) = rx.recv() {
            match message {
                WorkerMessage::Entry { order_idx, entry } => {
                    if fatal_error.is_none() {
                        on_entry(&entry);
                        entries.push((order_idx, entry));
                    }
                }
                WorkerMessage::Fatal(err) => {
                    if fatal_error.is_none() {
                        fatal_error = Some(err);
                    }
                }
            }
        }

        if let Some(err) = fatal_error {
            return Err(err);
        }
        if entries.len() != plans.len() {
            return Err(format!(
                "internal sweep error: expected {} results, got {}",
                plans.len(),
                entries.len()
            ));
        }

        Ok(entries)
    })
}

fn execute_run_plan(
    plan: &SweepRunPlan,
    mode: SweepMode,
    market_data: &(dyn MarketDataRepository + Sync),
    sentiment_repo: &(dyn SentimentRepository + Sync),
    artifacts: &(dyn ArtifactWriter + Sync),
    agent_factory: &AgentFactory<'_>,
) -> Result<SweepRunEntry, String> {
    let remote_agent = agent_factory(&plan.config)?;
    let result = match mode {
        SweepMode::Backtest => crate::backtesting::run_backtest(
            &plan.config,
            &plan.config_toml,
            None,
            market_data,
            sentiment_repo,
            artifacts,
            remote_agent,
        )
        .map(|_| ()),
        SweepMode::Paper => crate::paper_trading::run_paper(
            &plan.config,
            &plan.config_toml,
            None,
            market_data,
            sentiment_repo,
            artifacts,
            remote_agent,
        )
        .map(|_| ()),
    };

    let entry = match result {
        Ok(()) => SweepRunEntry {
            run_id: plan.run_id.clone(),
            split_id: plan.split_id.clone(),
            params: plan.params.clone(),
            status: "ok".to_string(),
            error: None,
            metrics: read_metrics_from_summary(&plan.summary_path).ok(),
        },
        Err(err) => SweepRunEntry {
            run_id: plan.run_id.clone(),
            split_id: plan.split_id.clone(),
            params: plan.params.clone(),
            status: "error".to_string(),
            error: Some(err),
            metrics: None,
        },
    };

    Ok(entry)
}

fn should_cancelled(should_cancel: Option<&(dyn Fn() -> bool + Sync)>) -> bool {
    should_cancel.map(|f| f()).unwrap_or(false)
}

fn update_progress(progress: &mut SweepProgress, entry: &SweepRunEntry) {
    progress.completed_runs = progress.completed_runs.saturating_add(1);
    progress.last_run_id = Some(entry.run_id.clone());
    progress.last_error = entry.error.clone();
    match entry.status.as_str() {
        "ok" => progress.ok_runs = progress.ok_runs.saturating_add(1),
        "skipped" => progress.skipped_runs = progress.skipped_runs.saturating_add(1),
        "error" => progress.error_runs = progress.error_runs.saturating_add(1),
        _ => {}
    }
}

fn emit_progress(
    on_progress: &mut Option<&mut dyn FnMut(SweepProgress)>,
    progress: &SweepProgress,
) {
    if let Some(callback) = on_progress.as_mut() {
        (callback)(progress.clone());
    }
}

fn resolve_base_config_path(sweep_path: &Path, base: &str) -> PathBuf {
    let p = PathBuf::from(base);
    if p.is_absolute() {
        p
    } else {
        sweep_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(p)
    }
}

fn validate_param_paths(params: &[SweepParam]) -> Result<(), String> {
    for p in params {
        let path = p.path.trim();
        if path.is_empty() {
            return Err("sweep param path cannot be empty".to_string());
        }
        let forbidden_prefixes = [
            "run.symbol",
            "run.timeframe",
            "db.exchange",
            "db.market",
            "db.ohlcv_table",
            "db.source_timeframe",
            "paths.out_dir",
            "paths.sentiment_path",
        ];
        if forbidden_prefixes.iter().any(|pre| path.starts_with(pre)) {
            return Err(format!("sweep param path not allowed: {}", p.path));
        }
        if p.values.is_empty() {
            return Err(format!("sweep param has no values: {}", p.path));
        }
    }
    Ok(())
}

fn expand_grid(params: &[SweepParam]) -> Vec<BTreeMap<String, toml::Value>> {
    let mut out: Vec<BTreeMap<String, toml::Value>> = vec![BTreeMap::new()];
    for p in params {
        let mut next: Vec<BTreeMap<String, toml::Value>> = Vec::new();
        for base in &out {
            for v in &p.values {
                let mut m = base.clone();
                m.insert(p.path.clone(), v.clone());
                next.push(m);
            }
        }
        out = next;
    }
    out
}

fn assignment_hash(split_id: &str, assignment: &BTreeMap<String, toml::Value>) -> String {
    let canonical = serde_json::to_string(assignment)
        .unwrap_or_else(|_| "{\"error\":\"assignment\"}".to_string());
    let mut hasher = Sha256::new();
    hasher.update(split_id.as_bytes());
    hasher.update(b"\n");
    hasher.update(canonical.as_bytes());
    let bytes = hasher.finalize();
    to_hex_short(&bytes[..], 12)
}

fn to_hex_short(bytes: &[u8], chars: usize) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(chars);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        if out.len() >= chars {
            break;
        }
        out.push(HEX[(b & 0x0f) as usize] as char);
        if out.len() >= chars {
            break;
        }
    }
    out
}

fn set_run_id(root: &mut toml::Value, run_id: &str) -> Result<(), String> {
    set_path_value(root, "run.run_id", toml::Value::String(run_id.to_string()))
}

fn apply_assignment(
    root: &mut toml::Value,
    assignment: &BTreeMap<String, toml::Value>,
) -> Result<(), String> {
    for (path, value) in assignment {
        set_path_value(root, path, value.clone())?;
    }
    Ok(())
}

fn set_path_value(root: &mut toml::Value, path: &str, value: toml::Value) -> Result<(), String> {
    let parts: Vec<&str> = path
        .split('.')
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect();
    if parts.is_empty() {
        return Err("empty path".to_string());
    }
    let mut cur = root;
    for key in &parts[..parts.len() - 1] {
        cur = cur
            .get_mut(*key)
            .ok_or_else(|| format!("path not found: {}", path))?;
        if !cur.is_table() {
            return Err(format!("path is not a table: {}", path));
        }
    }
    let last = parts[parts.len() - 1];
    let table = cur
        .as_table_mut()
        .ok_or_else(|| format!("path is not a table: {}", path))?;
    if !table.contains_key(last) {
        return Err(format!("path not found: {}", path));
    }
    table.insert(last.to_string(), value);
    Ok(())
}

fn filter_bars_for_split(
    source: &[Bar],
    step_seconds: i64,
    split: &SweepSplit,
) -> Result<(Vec<Bar>, kairos_domain::services::ohlcv::DataQualityReport), String> {
    let start = split
        .start
        .as_deref()
        .map(parse_timestamp_seconds)
        .transpose()?;
    let end = split
        .end
        .as_deref()
        .map(parse_timestamp_seconds)
        .transpose()?;

    let bars: Vec<Bar> = source
        .iter()
        .filter(|b| start.map(|s| b.timestamp >= s).unwrap_or(true))
        .filter(|b| end.map(|e| b.timestamp <= e).unwrap_or(true))
        .cloned()
        .collect();
    let report = data_quality_from_bars(&bars, Some(step_seconds));
    Ok((bars, report))
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

fn read_metrics_from_summary(path: &Path) -> Result<RunMetrics, String> {
    let raw = std::fs::read_to_string(path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    let value: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|err| format!("failed to parse {}: {err}", path.display()))?;
    let summary = value.get("summary").unwrap_or(&value);
    Ok(RunMetrics {
        bars_processed: summary
            .get("bars_processed")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        trades: summary.get("trades").and_then(|v| v.as_u64()).unwrap_or(0),
        win_rate: summary
            .get("win_rate")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
        net_profit: summary
            .get("net_profit")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
        sharpe: summary
            .get("sharpe")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
        max_drawdown: summary
            .get("max_drawdown")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
    })
}

fn write_manifest(dir: &Path, result: &SweepResult) -> Result<(), String> {
    let path = dir.join("manifest.json");
    let json = serde_json::to_string_pretty(result)
        .map_err(|err| format!("failed to serialize manifest: {err}"))?;
    std::fs::write(&path, json)
        .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    Ok(())
}

fn write_results_csv(dir: &Path, result: &SweepResult) -> Result<(), String> {
    let path = dir.join("results.csv");
    let mut wtr = csv::Writer::from_path(&path)
        .map_err(|err| format!("failed to create {}: {err}", path.display()))?;
    wtr.write_record([
        "run_id",
        "split_id",
        "status",
        "bars_processed",
        "trades",
        "win_rate",
        "net_profit",
        "sharpe",
        "max_drawdown",
        "error",
    ])
    .map_err(|err| format!("failed to write results header: {err}"))?;

    for r in &result.runs {
        let m = r.metrics;
        let record = vec![
            r.run_id.clone(),
            r.split_id.clone(),
            r.status.clone(),
            m.map(|m| m.bars_processed.to_string()).unwrap_or_default(),
            m.map(|m| m.trades.to_string()).unwrap_or_default(),
            m.map(|m| format!("{}", m.win_rate)).unwrap_or_default(),
            m.map(|m| format!("{}", m.net_profit)).unwrap_or_default(),
            m.map(|m| format!("{}", m.sharpe)).unwrap_or_default(),
            m.map(|m| format!("{}", m.max_drawdown)).unwrap_or_default(),
            r.error.clone().unwrap_or_default(),
        ];
        wtr.write_record(record)
            .map_err(|err| format!("failed to write results row: {err}"))?;
    }
    wtr.flush()
        .map_err(|err| format!("failed to flush {}: {err}", path.display()))?;
    Ok(())
}

fn write_leaderboard_csv(
    dir: &Path,
    result: &SweepResult,
    cfg: Option<&LeaderboardConfig>,
) -> Result<(), String> {
    let sort_by = cfg
        .and_then(|c| c.sort_by.as_deref())
        .unwrap_or("sharpe")
        .trim()
        .to_lowercase();
    let descending = cfg.and_then(|c| c.descending).unwrap_or(true);

    let mut rows: Vec<&SweepRunEntry> = result
        .runs
        .iter()
        .filter(|r| r.status == "ok" && r.metrics.is_some())
        .collect();
    rows.sort_by(|a, b| {
        let av = metric_value(a.metrics.unwrap(), &sort_by);
        let bv = metric_value(b.metrics.unwrap(), &sort_by);
        let ord = bv.partial_cmp(&av).unwrap_or(std::cmp::Ordering::Equal);
        if descending {
            ord
        } else {
            ord.reverse()
        }
    });

    let path = dir.join("leaderboard.csv");
    let mut wtr = csv::Writer::from_path(&path)
        .map_err(|err| format!("failed to create {}: {err}", path.display()))?;
    wtr.write_record([
        "rank",
        "run_id",
        "split_id",
        "bars_processed",
        "trades",
        "win_rate",
        "net_profit",
        "sharpe",
        "max_drawdown",
    ])
    .map_err(|err| format!("failed to write leaderboard header: {err}"))?;

    for (idx, r) in rows.iter().enumerate() {
        let m = r.metrics.unwrap();
        let record = vec![
            (idx + 1).to_string(),
            r.run_id.clone(),
            r.split_id.clone(),
            m.bars_processed.to_string(),
            m.trades.to_string(),
            format!("{}", m.win_rate),
            format!("{}", m.net_profit),
            format!("{}", m.sharpe),
            format!("{}", m.max_drawdown),
        ];
        wtr.write_record(record)
            .map_err(|err| format!("failed to write leaderboard row: {err}"))?;
    }
    wtr.flush()
        .map_err(|err| format!("failed to flush {}: {err}", path.display()))?;
    Ok(())
}

fn metric_value(m: RunMetrics, key: &str) -> f64 {
    match key {
        "net_profit" => m.net_profit,
        "max_drawdown" | "max_dd" | "max_drawdown_pct" => m.max_drawdown,
        "trades" => m.trades as f64,
        "bars_processed" => m.bars_processed as f64,
        "win_rate" => m.win_rate,
        _ => m.sharpe,
    }
}

#[derive(Default)]
struct InMemoryMarketDataRepository {
    bars: Vec<Bar>,
    report: kairos_domain::services::ohlcv::DataQualityReport,
}

impl MarketDataRepository for InMemoryMarketDataRepository {
    fn load_ohlcv(
        &self,
        _query: &OhlcvQuery,
    ) -> Result<(Vec<Bar>, kairos_domain::services::ohlcv::DataQualityReport), String> {
        Ok((self.bars.clone(), self.report.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kairos_domain::repositories::sentiment::SentimentQuery;
    use kairos_domain::services::sentiment::{SentimentPoint, SentimentReport};
    use kairos_infrastructure::artifacts::FilesystemArtifactWriter;
    use std::sync::atomic::AtomicUsize;
    use std::time::Duration;

    #[test]
    fn expand_grid_is_deterministic() {
        let params = vec![
            SweepParam {
                path: "strategy.sma_short".to_string(),
                values: vec![toml::Value::Integer(1), toml::Value::Integer(2)],
            },
            SweepParam {
                path: "strategy.sma_long".to_string(),
                values: vec![toml::Value::Integer(10), toml::Value::Integer(20)],
            },
        ];
        let grid = expand_grid(&params);
        assert_eq!(grid.len(), 4);
        assert_eq!(
            grid[0].get("strategy.sma_short").unwrap().as_integer(),
            Some(1)
        );
        assert_eq!(
            grid[0].get("strategy.sma_long").unwrap().as_integer(),
            Some(10)
        );
        assert_eq!(
            grid[3].get("strategy.sma_short").unwrap().as_integer(),
            Some(2)
        );
        assert_eq!(
            grid[3].get("strategy.sma_long").unwrap().as_integer(),
            Some(20)
        );
    }

    #[test]
    fn set_path_value_rejects_unknown_path() {
        let mut v: toml::Value = toml::from_str("[a]\nb=1\n").unwrap();
        let err = set_path_value(&mut v, "a.c", toml::Value::Integer(2)).unwrap_err();
        assert!(err.contains("path not found"));
    }

    #[test]
    fn normalize_parallelism_guards_invalid_values() {
        assert_eq!(normalize_parallelism(None), 1);
        assert_eq!(normalize_parallelism(Some(0)), 1);
        assert_eq!(normalize_parallelism(Some(4)), 4);
    }

    struct EmptySentimentRepo;

    impl SentimentRepository for EmptySentimentRepo {
        fn load_sentiment(
            &self,
            _query: &SentimentQuery,
        ) -> Result<(Vec<SentimentPoint>, SentimentReport), String> {
            Ok((Vec::new(), SentimentReport::default()))
        }
    }

    fn test_temp_dir(prefix: &str) -> PathBuf {
        let unique = format!(
            "{}_{}_{}",
            prefix,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock before UNIX_EPOCH")
                .as_nanos()
        );
        std::env::temp_dir().join(unique)
    }

    fn sample_bars(symbol: &str, count: usize) -> Vec<Bar> {
        (0..count)
            .map(|index| {
                let ts = 60_i64 * (index as i64 + 1);
                let close = 100.0 + index as f64;
                Bar {
                    symbol: symbol.to_string(),
                    timestamp: ts,
                    open: close,
                    high: close + 1.0,
                    low: close - 1.0,
                    close,
                    volume: 1.0,
                }
            })
            .collect()
    }

    #[test]
    fn run_sweep_parallelism_executes_concurrently_and_keeps_order() {
        let temp_dir = test_temp_dir("kairos_sweep_parallel");
        std::fs::create_dir_all(&temp_dir).expect("temp dir");

        let out_dir = temp_dir.join("runs_out");
        let base_config = format!(
            r#"
[run]
run_id = "base_run"
symbol = "BTCUSDT"
timeframe = "1min"
initial_capital = 1000.0

[db]
ohlcv_table = "ohlcv_candles"
exchange = "kucoin"
market = "spot"

[paths]
out_dir = "{}"

[costs]
fee_bps = 0.0
slippage_bps = 0.0

[risk]
max_position_qty = 1.0
max_drawdown_pct = 1.0
max_exposure_pct = 1.0

[features]
return_mode = "pct"
sma_windows = [2]
rsi_enabled = false
sentiment_lag = "0s"

[agent]
mode = "baseline"
url = "http://127.0.0.1:8000"
timeout_ms = 100
retries = 0
fallback_action = "HOLD"
api_version = "v1"
feature_version = "v1"
"#,
            out_dir.display()
        );
        let base_path = temp_dir.join("base.toml");
        std::fs::write(&base_path, base_config).expect("write base config");

        let sweep_path = temp_dir.join("sweep.toml");
        std::fs::write(
            &sweep_path,
            r#"
[base]
config = "base.toml"

[sweep]
id = "parallel_demo"
mode = "backtest"
parallelism = 4
resume = false

[[params]]
path = "costs.slippage_bps"
values = [0.0, 1.0, 2.0]
"#,
        )
        .expect("write sweep config");

        let bars = sample_bars("BTCUSDT", 128);
        let source_market = InMemoryMarketDataRepository {
            bars: bars.clone(),
            report: data_quality_from_bars(&bars, Some(60)),
        };
        let sentiment = EmptySentimentRepo;
        let artifacts = FilesystemArtifactWriter::new();

        let active_calls = AtomicUsize::new(0);
        let max_active_calls = AtomicUsize::new(0);
        let agent_factory = |_: &Config| -> AgentFactoryResult {
            let current = active_calls.fetch_add(1, Ordering::Relaxed) + 1;
            max_active_calls.fetch_max(current, Ordering::Relaxed);
            std::thread::sleep(Duration::from_millis(35));
            active_calls.fetch_sub(1, Ordering::Relaxed);
            Ok(None)
        };

        let result = run_sweep(
            &sweep_path,
            &agent_factory,
            &source_market,
            &sentiment,
            &artifacts,
        )
        .expect("run sweep");

        assert_eq!(result.runs.len(), 3);
        assert!(result.runs.iter().all(|run| run.status == "ok"));
        assert!(
            max_active_calls.load(Ordering::Relaxed) > 1,
            "parallel sweep should call agent_factory concurrently"
        );

        let expected_assignments = expand_grid(&[SweepParam {
            path: "costs.slippage_bps".to_string(),
            values: vec![
                toml::Value::Float(0.0),
                toml::Value::Float(1.0),
                toml::Value::Float(2.0),
            ],
        }]);
        let expected_run_ids: Vec<String> = expected_assignments
            .iter()
            .map(|assignment| {
                format!(
                    "parallel_demo__{}__full",
                    assignment_hash("full", assignment)
                )
            })
            .collect();
        let actual_run_ids: Vec<String> =
            result.runs.iter().map(|run| run.run_id.clone()).collect();
        assert_eq!(actual_run_ids, expected_run_ids);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
