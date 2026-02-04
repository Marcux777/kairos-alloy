use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct PortfolioState {
    pub cash: f64,
    pub position_qty: f64,
    pub position_avg_price: f64,
    pub equity: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ActionBatchItem {
    pub timestamp: String,
    pub observation: Vec<f64>,
    pub portfolio_state: PortfolioState,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ActionRequest {
    pub api_version: String,
    pub feature_version: String,
    pub run_id: String,
    pub timestamp: String,
    pub symbol: String,
    pub timeframe: String,
    pub observation: Vec<f64>,
    pub portfolio_state: PortfolioState,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ActionBatchRequest {
    pub api_version: String,
    pub feature_version: String,
    pub run_id: String,
    pub symbol: String,
    pub timeframe: String,
    pub items: Vec<ActionBatchItem>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ActionResponse {
    pub action_type: String,
    pub size: f64,
    pub confidence: Option<f64>,
    pub model_version: Option<String>,
    pub latency_ms: Option<u64>,
    pub reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ActionBatchResponse {
    pub items: Vec<ActionResponse>,
}
