use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DomainEvent {
    EngineStarted { run_id: String, timestamp: i64 },
    TradeExecuted { run_id: String, timestamp: i64 },
}
