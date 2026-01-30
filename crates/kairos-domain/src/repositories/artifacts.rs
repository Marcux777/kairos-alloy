use crate::entities::metrics::MetricsSummary;
use crate::services::audit::AuditEvent;
use crate::value_objects::equity_point::EquityPoint;
use crate::value_objects::trade::Trade;
use std::path::Path;

pub trait ArtifactWriter {
    fn ensure_dir(&self, path: &Path) -> Result<(), String>;
    fn write_trades_csv(&self, path: &Path, trades: &[Trade]) -> Result<(), String>;
    fn write_equity_csv(&self, path: &Path, points: &[EquityPoint]) -> Result<(), String>;
    fn write_summary_json(
        &self,
        path: &Path,
        summary: &MetricsSummary,
        meta: Option<&serde_json::Value>,
        config_snapshot: Option<&serde_json::Value>,
    ) -> Result<(), String>;
    fn write_summary_html(
        &self,
        path: &Path,
        summary: &MetricsSummary,
        meta: Option<&serde_json::Value>,
    ) -> Result<(), String>;
    fn write_audit_jsonl(&self, path: &Path, events: &[AuditEvent]) -> Result<(), String>;
    fn write_config_snapshot_toml(&self, path: &Path, contents: &str) -> Result<(), String>;
}

pub trait ArtifactReader {
    fn read_trades_csv(&self, path: &Path) -> Result<Vec<Trade>, String>;
    fn read_equity_csv(&self, path: &Path) -> Result<Vec<EquityPoint>, String>;
    fn read_config_snapshot_toml(&self, path: &Path) -> Result<Option<String>, String>;
    fn exists(&self, path: &Path) -> bool;
}
