use crate::types::{Action, ActionType};
use reqwest::blocking::Client;
use reqwest::StatusCode;
use std::time::Duration;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct PortfolioState {
    pub cash: f64,
    pub position_qty: f64,
    pub position_avg_price: f64,
    pub equity: f64,
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
pub struct ActionResponse {
    pub action_type: String,
    pub size: f64,
    pub confidence: Option<f64>,
    pub model_version: Option<String>,
    pub latency_ms: Option<u64>,
}

pub struct AgentClient {
    pub url: String,
    pub timeout_ms: u64,
    pub api_version: String,
    pub feature_version: String,
    pub retries: u32,
    pub fallback_action: ActionType,
}

impl AgentClient {
    pub fn new(
        url: String,
        timeout_ms: u64,
        api_version: String,
        feature_version: String,
        retries: u32,
        fallback_action: ActionType,
    ) -> Self {
        Self {
            url,
            timeout_ms,
            api_version,
            feature_version,
            retries,
            fallback_action,
        }
    }

    pub fn act(&self, request: &ActionRequest) -> Result<ActionResponse, String> {
        let client = Client::builder()
            .timeout(Duration::from_millis(self.timeout_ms))
            .build()
            .map_err(|err| format!("failed to build http client: {}", err))?;

        let endpoint = format!("{}/v1/act", self.url.trim_end_matches('/'));
        let mut attempts = 0;
        while attempts <= self.retries {
            attempts += 1;
            let response = client.post(&endpoint).json(request).send();
            match response {
                Ok(resp) => {
                    if resp.status() == StatusCode::OK {
                        return resp
                            .json::<ActionResponse>()
                            .map_err(|err| format!("failed to parse agent response: {}", err));
                    }
                    if resp.status().is_server_error() && attempts <= self.retries {
                        continue;
                    }
                    return Err(format!(
                        "agent http error: status {}",
                        resp.status().as_u16()
                    ));
                }
                Err(err) => {
                    if attempts <= self.retries {
                        continue;
                    }
                    return Err(format!("agent request failed: {}", err));
                }
            }
        }

        Err("agent request failed after retries".to_string())
    }

    pub fn to_action(response: &ActionResponse) -> Action {
        match response.action_type.as_str() {
            "BUY" => Action {
                action_type: ActionType::Buy,
                size: response.size,
            },
            "SELL" => Action {
                action_type: ActionType::Sell,
                size: response.size,
            },
            _ => Action::hold(),
        }
    }

    pub fn fallback_response(&self) -> ActionResponse {
        let action_type = match self.fallback_action {
            ActionType::Buy => "BUY",
            ActionType::Sell => "SELL",
            ActionType::Hold => "HOLD",
        };
        ActionResponse {
            action_type: action_type.to_string(),
            size: 0.0,
            confidence: None,
            model_version: None,
            latency_ms: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AgentClient;
    use crate::types::ActionType;

    #[test]
    fn agent_client_fallback_is_hold() {
        let client = AgentClient::new(
            "http://127.0.0.1:8000".to_string(),
            200,
            "v1".to_string(),
            "v1".to_string(),
            0,
            ActionType::Hold,
        );
        let response = client.fallback_response();
        assert_eq!(response.action_type, "HOLD");
    }
}
