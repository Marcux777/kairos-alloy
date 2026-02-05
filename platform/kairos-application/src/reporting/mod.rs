use crate::config::Config;
use crate::shared::{
    config_snapshot_json, resolve_execution_config, summary_meta_json_from_equity,
};
use kairos_domain::entities::metrics::{recompute_summary, MetricsSummary};
use kairos_domain::repositories::artifacts::{ArtifactReader, ArtifactWriter};
use kairos_domain::services::audit::AuditEvent;
use kairos_domain::value_objects::equity_point::EquityPoint;
use kairos_domain::value_objects::trade::Trade;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tracing::info_span;

pub struct GenerateReportResult {
    pub input_dir: PathBuf,
    pub run_id: String,
    pub summary: MetricsSummary,
    pub wrote_html: bool,
}

pub fn generate_report(
    input_dir: &Path,
    reader: &dyn ArtifactReader,
    writer: &dyn ArtifactWriter,
) -> Result<GenerateReportResult, String> {
    let _span = info_span!("generate_report", input_dir = %input_dir.display()).entered();

    let stage_start = Instant::now();
    let trades_path = input_dir.join("trades.csv");
    let equity_path = input_dir.join("equity.csv");
    let config_path = input_dir.join("config_snapshot.toml");

    if !reader.exists(&trades_path) || !reader.exists(&equity_path) {
        return Err(format!(
            "missing trades.csv or equity.csv in {}",
            input_dir.display()
        ));
    }

    let trades = reader.read_trades_csv(&trades_path)?;
    let equity = reader.read_equity_csv(&equity_path)?;
    let summary = recompute_summary(&trades, &equity);
    metrics::histogram!("kairos.report.generate_ms")
        .record(stage_start.elapsed().as_millis() as f64);
    metrics::gauge!("kairos.report.trades").set(trades.len() as f64);
    metrics::gauge!("kairos.report.bars_processed").set(summary.bars_processed as f64);

    let config_toml = reader.read_config_snapshot_toml(&config_path)?;
    let (run_id, meta, config_snapshot, wrote_html) = match config_toml
        .as_deref()
        .and_then(|raw| load_config_from_str(raw).ok())
    {
        Some(config) => {
            let meta = summary_meta_json_from_equity(&config, &equity);
            let execution = resolve_execution_config(&config)?;
            let snapshot = config_snapshot_json(&config, &execution);
            let run_id = meta
                .as_ref()
                .and_then(|m| {
                    m.get("run_id")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })
                .unwrap_or_else(|| config.run.run_id.clone());
            let html = config
                .report
                .as_ref()
                .and_then(|report| report.html)
                .unwrap_or(false);
            (run_id, meta, Some(snapshot), html)
        }
        None => ("unknown".to_string(), None, None, false),
    };

    writer.write_summary_json(
        input_dir.join("summary.json").as_path(),
        &summary,
        meta.as_ref(),
        config_snapshot.as_ref(),
    )?;

    if wrote_html {
        writer.write_summary_html(
            input_dir.join("summary.html").as_path(),
            &summary,
            meta.as_ref(),
        )?;
        writer.write_dashboard_html(
            input_dir.join("dashboard.html").as_path(),
            &summary,
            meta.as_ref(),
            &trades,
            &equity,
        )?;
    }

    let events = build_report_events(
        &run_id,
        &trades,
        &summary,
        &equity,
        meta.as_ref(),
        input_dir,
    );
    writer.write_audit_jsonl(input_dir.join("logs.jsonl").as_path(), &events)?;

    Ok(GenerateReportResult {
        input_dir: input_dir.to_path_buf(),
        run_id,
        summary,
        wrote_html,
    })
}

fn load_config_from_str(raw: &str) -> Result<Config, String> {
    toml::from_str(raw).map_err(|err| format!("failed to parse config snapshot TOML: {err}"))
}

fn build_report_events(
    run_id: &str,
    trades: &[Trade],
    summary: &MetricsSummary,
    equity: &[EquityPoint],
    meta: Option<&serde_json::Value>,
    input_dir: &Path,
) -> Vec<AuditEvent> {
    let end_ts = equity.last().map(|p| p.timestamp).unwrap_or(0);
    let mut events = Vec::with_capacity(trades.len() + 2);

    for trade in trades {
        events.push(AuditEvent {
            run_id: run_id.to_string(),
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
        run_id: run_id.to_string(),
        timestamp: end_ts,
        stage: "report".to_string(),
        symbol: None,
        action: "recompute".to_string(),
        error: None,
        details: serde_json::json!({
            "input_dir": input_dir.display().to_string(),
            "trades": trades.len(),
            "bars_processed": summary.bars_processed,
        }),
    });

    events.push(AuditEvent {
        run_id: run_id.to_string(),
        timestamp: end_ts,
        stage: "summary".to_string(),
        symbol: meta
            .and_then(|m| m.get("symbol"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        action: "complete".to_string(),
        error: None,
        details: serde_json::json!({
            "meta": meta,
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
    events
}
