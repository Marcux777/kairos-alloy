use crate::reporting;
use kairos_domain::entities::metrics::MetricsSummary;
use kairos_domain::repositories::artifacts::{ArtifactReader, ArtifactWriter};
use kairos_domain::services::audit::AuditEvent;
use kairos_domain::value_objects::equity_point::EquityPoint;
use kairos_domain::value_objects::trade::Trade;
use std::fs;
use std::path::Path;
use std::time::Instant;

#[derive(Debug, Default, Clone, Copy)]
pub struct FilesystemArtifactWriter;

impl FilesystemArtifactWriter {
    pub fn new() -> Self {
        Self
    }
}

fn parse_summary_meta(meta: &serde_json::Value) -> Option<reporting::SummaryMeta> {
    Some(reporting::SummaryMeta {
        run_id: meta.get("run_id")?.as_str()?.to_string(),
        symbol: meta.get("symbol")?.as_str()?.to_string(),
        timeframe: meta.get("timeframe")?.as_str()?.to_string(),
        start: meta.get("start")?.as_i64()?,
        end: meta.get("end")?.as_i64()?,
    })
}

fn record_write_metrics(kind: &'static str, start: Instant, result: &Result<(), String>) {
    let result_label = if result.is_ok() { "ok" } else { "err" };
    metrics::counter!(
        "kairos.infra.artifacts.write.calls_total",
        "kind" => kind,
        "result" => result_label
    )
    .increment(1);
    metrics::histogram!("kairos.infra.artifacts.write_ms", "kind" => kind, "result" => result_label)
        .record(start.elapsed().as_millis() as f64);
}

fn record_read_metrics<T>(kind: &'static str, start: Instant, result: &Result<T, String>) {
    let result_label = if result.is_ok() { "ok" } else { "err" };
    metrics::counter!(
        "kairos.infra.artifacts.read.calls_total",
        "kind" => kind,
        "result" => result_label
    )
    .increment(1);
    metrics::histogram!("kairos.infra.artifacts.read_ms", "kind" => kind, "result" => result_label)
        .record(start.elapsed().as_millis() as f64);
}

impl ArtifactWriter for FilesystemArtifactWriter {
    fn ensure_dir(&self, path: &Path) -> Result<(), String> {
        let start = Instant::now();
        let result = fs::create_dir_all(path)
            .map_err(|err| format!("failed to create dir {}: {}", path.display(), err));
        record_write_metrics("ensure_dir", start, &result);
        result
    }

    fn write_trades_csv(&self, path: &Path, trades: &[Trade]) -> Result<(), String> {
        let start = Instant::now();
        let result = reporting::write_trades_csv(path, trades);
        record_write_metrics("trades_csv", start, &result);
        result
    }

    fn write_equity_csv(&self, path: &Path, points: &[EquityPoint]) -> Result<(), String> {
        let start = Instant::now();
        let result = reporting::write_equity_csv(path, points);
        record_write_metrics("equity_csv", start, &result);
        result
    }

    fn write_summary_json(
        &self,
        path: &Path,
        summary: &MetricsSummary,
        meta: Option<&serde_json::Value>,
        config_snapshot: Option<&serde_json::Value>,
    ) -> Result<(), String> {
        let parsed = meta.and_then(parse_summary_meta);
        let start = Instant::now();
        let result = reporting::write_summary_json(path, summary, parsed.as_ref(), config_snapshot);
        record_write_metrics("summary_json", start, &result);
        result
    }

    fn write_analyzer_json(&self, path: &Path, value: &serde_json::Value) -> Result<(), String> {
        let start = Instant::now();
        let result = serde_json::to_string_pretty(value)
            .map_err(|err| format!("failed to serialize analyzer json: {err}"))
            .and_then(|json| {
                fs::write(path, json).map_err(|err| {
                    format!("failed to write analyzer json {}: {}", path.display(), err)
                })
            });
        record_write_metrics("analyzer_json", start, &result);
        result
    }

    fn write_summary_html(
        &self,
        path: &Path,
        summary: &MetricsSummary,
        meta: Option<&serde_json::Value>,
    ) -> Result<(), String> {
        let parsed = meta.and_then(parse_summary_meta);
        let start = Instant::now();
        let result = reporting::write_summary_html(path, summary, parsed.as_ref());
        record_write_metrics("summary_html", start, &result);
        result
    }

    fn write_dashboard_html(
        &self,
        path: &Path,
        summary: &MetricsSummary,
        meta: Option<&serde_json::Value>,
        trades: &[Trade],
        equity: &[EquityPoint],
    ) -> Result<(), String> {
        let parsed = meta.and_then(parse_summary_meta);
        let start = Instant::now();
        let result =
            reporting::write_dashboard_html(path, summary, parsed.as_ref(), trades, equity);
        record_write_metrics("dashboard_html", start, &result);
        result
    }

    fn write_audit_jsonl(&self, path: &Path, events: &[AuditEvent]) -> Result<(), String> {
        let start = Instant::now();
        let result = reporting::write_audit_jsonl(path, events);
        record_write_metrics("logs_jsonl", start, &result);
        result
    }

    fn write_config_snapshot_toml(&self, path: &Path, contents: &str) -> Result<(), String> {
        let start = Instant::now();
        let result = fs::write(path, contents).map_err(|err| {
            format!(
                "failed to write config snapshot {}: {}",
                path.display(),
                err
            )
        });
        record_write_metrics("config_snapshot_toml", start, &result);
        result
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct FilesystemArtifactReader;

impl FilesystemArtifactReader {
    pub fn new() -> Self {
        Self
    }
}

impl ArtifactReader for FilesystemArtifactReader {
    fn read_trades_csv(&self, path: &Path) -> Result<Vec<Trade>, String> {
        let start = Instant::now();
        let result = reporting::read_trades_csv(path);
        record_read_metrics("trades_csv", start, &result);
        result
    }

    fn read_equity_csv(&self, path: &Path) -> Result<Vec<EquityPoint>, String> {
        let start = Instant::now();
        let result = reporting::read_equity_csv(path);
        record_read_metrics("equity_csv", start, &result);
        result
    }

    fn read_config_snapshot_toml(&self, path: &Path) -> Result<Option<String>, String> {
        let start = Instant::now();
        if !path.exists() {
            record_read_metrics(
                "config_snapshot_toml",
                start,
                &Ok::<Option<String>, String>(None),
            );
            return Ok(None);
        }
        let result = fs::read_to_string(path)
            .map(Some)
            .map_err(|err| format!("failed to read config snapshot {}: {}", path.display(), err));
        record_read_metrics("config_snapshot_toml", start, &result);
        result
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }
}
