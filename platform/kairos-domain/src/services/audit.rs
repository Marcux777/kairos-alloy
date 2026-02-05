use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct AuditEvent {
    pub run_id: String,
    pub timestamp: i64,
    pub stage: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub details: serde_json::Value,
}
