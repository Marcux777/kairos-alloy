use crate::reporting;
use kairos_domain::entities::metrics::MetricsSummary;
use kairos_domain::repositories::artifacts::{ArtifactReader, ArtifactWriter};
use kairos_domain::services::audit::AuditEvent;
use kairos_domain::value_objects::equity_point::EquityPoint;
use kairos_domain::value_objects::trade::Trade;
use std::fs;
use std::path::Path;

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

impl ArtifactWriter for FilesystemArtifactWriter {
    fn ensure_dir(&self, path: &Path) -> Result<(), String> {
        fs::create_dir_all(path)
            .map_err(|err| format!("failed to create dir {}: {}", path.display(), err))
    }

    fn write_trades_csv(&self, path: &Path, trades: &[Trade]) -> Result<(), String> {
        reporting::write_trades_csv(path, trades)
    }

    fn write_equity_csv(&self, path: &Path, points: &[EquityPoint]) -> Result<(), String> {
        reporting::write_equity_csv(path, points)
    }

    fn write_summary_json(
        &self,
        path: &Path,
        summary: &MetricsSummary,
        meta: Option<&serde_json::Value>,
        config_snapshot: Option<&serde_json::Value>,
    ) -> Result<(), String> {
        let parsed = meta.and_then(parse_summary_meta);
        reporting::write_summary_json(path, summary, parsed.as_ref(), config_snapshot)
    }

    fn write_summary_html(
        &self,
        path: &Path,
        summary: &MetricsSummary,
        meta: Option<&serde_json::Value>,
    ) -> Result<(), String> {
        let parsed = meta.and_then(parse_summary_meta);
        reporting::write_summary_html(path, summary, parsed.as_ref())
    }

    fn write_audit_jsonl(&self, path: &Path, events: &[AuditEvent]) -> Result<(), String> {
        reporting::write_audit_jsonl(path, events)
    }

    fn write_config_snapshot_toml(&self, path: &Path, contents: &str) -> Result<(), String> {
        fs::write(path, contents).map_err(|err| {
            format!(
                "failed to write config snapshot {}: {}",
                path.display(),
                err
            )
        })
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
        reporting::read_trades_csv(path)
    }

    fn read_equity_csv(&self, path: &Path) -> Result<Vec<EquityPoint>, String> {
        reporting::read_equity_csv(path)
    }

    fn read_config_snapshot_toml(&self, path: &Path) -> Result<Option<String>, String> {
        if !path.exists() {
            return Ok(None);
        }
        fs::read_to_string(path)
            .map(Some)
            .map_err(|err| format!("failed to read config snapshot {}: {}", path.display(), err))
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }
}
