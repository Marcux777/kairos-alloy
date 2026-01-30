pub use kairos_domain::services::agent::{
    ActionBatchItem, ActionBatchRequest, ActionBatchResponse, ActionRequest, ActionResponse,
    PortfolioState,
};
use kairos_domain::value_objects::action::Action;
use kairos_domain::value_objects::action_type::ActionType;
use reqwest::blocking::Client;
use reqwest::StatusCode;
use serde::Serialize;
use std::time::Duration;
use std::time::Instant;

#[derive(Debug, Clone, Serialize)]
pub struct AgentCallInfo {
    pub attempts: u32,
    pub duration_ms: u64,
    pub status: Option<u16>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentCallResult {
    pub info: AgentCallInfo,
    pub response: Option<ActionResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentBatchCallResult {
    pub info: AgentCallInfo,
    pub responses: Option<Vec<ActionResponse>>,
}

pub struct AgentClient {
    pub url: String,
    pub timeout_ms: u64,
    pub api_version: String,
    pub feature_version: String,
    pub retries: u32,
    pub fallback_action: ActionType,
    client: Client,
}

impl AgentClient {
    pub fn new(
        url: String,
        timeout_ms: u64,
        api_version: String,
        feature_version: String,
        retries: u32,
        fallback_action: ActionType,
    ) -> Result<Self, String> {
        let client = Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .pool_idle_timeout(Duration::from_secs(90))
            .build()
            .map_err(|err| format!("failed to build http client: {err}"))?;
        Ok(Self {
            url,
            timeout_ms,
            api_version,
            feature_version,
            retries,
            fallback_action,
            client,
        })
    }

    pub fn act(&self, request: &ActionRequest) -> Result<ActionResponse, String> {
        let result = self.act_detailed(request);
        match result.response {
            Some(response) => Ok(response),
            None => Err(result
                .info
                .error
                .unwrap_or_else(|| "agent request failed".to_string())),
        }
    }

    pub fn act_detailed(&self, request: &ActionRequest) -> AgentCallResult {
        let endpoint = format!("{}/v1/act", self.url.trim_end_matches('/'));
        let start = Instant::now();
        let mut attempts = 0u32;
        let mut last_status: Option<u16> = None;
        let mut last_error: Option<String> = None;

        while attempts <= self.retries {
            attempts += 1;
            let response = self.client.post(&endpoint).json(request).send();
            match response {
                Ok(resp) => {
                    last_status = Some(resp.status().as_u16());
                    if resp.status() == StatusCode::OK {
                        match resp.json::<ActionResponse>() {
                            Ok(parsed) => match validate_action_response(&parsed) {
                                Ok(()) => {
                                    return AgentCallResult {
                                        info: AgentCallInfo {
                                            attempts,
                                            duration_ms: start.elapsed().as_millis() as u64,
                                            status: last_status,
                                            error: None,
                                        },
                                        response: Some(parsed),
                                    };
                                }
                                Err(err) => {
                                    last_error = Some(err);
                                    break;
                                }
                            },
                            Err(err) => {
                                last_error = Some(format!("failed to parse agent response: {err}"));
                                break;
                            }
                        }
                    }

                    if resp.status().is_server_error() && attempts <= self.retries {
                        continue;
                    }
                    last_error = Some(format!(
                        "agent http error: status {}",
                        resp.status().as_u16()
                    ));
                    break;
                }
                Err(err) => {
                    last_error = Some(format!("agent request failed: {err}"));
                    if attempts <= self.retries {
                        continue;
                    }
                    break;
                }
            }
        }

        AgentCallResult {
            info: AgentCallInfo {
                attempts,
                duration_ms: start.elapsed().as_millis() as u64,
                status: last_status,
                error: last_error.or_else(|| Some("agent request failed after retries".to_string())),
            },
            response: None,
        }
    }

    pub fn act_batch(&self, requests: &[ActionRequest]) -> Result<Vec<ActionResponse>, String> {
        let result = self.act_batch_detailed(requests);
        match result.responses {
            Some(responses) => Ok(responses),
            None => Err(result
                .info
                .error
                .unwrap_or_else(|| "agent batch request failed".to_string())),
        }
    }

    pub fn act_batch_detailed(&self, requests: &[ActionRequest]) -> AgentBatchCallResult {
        if requests.is_empty() {
            return AgentBatchCallResult {
                info: AgentCallInfo {
                    attempts: 0,
                    duration_ms: 0,
                    status: None,
                    error: None,
                },
                responses: Some(Vec::new()),
            };
        }

        let endpoint = format!("{}/v1/act_batch", self.url.trim_end_matches('/'));
        let start = Instant::now();
        let mut attempts = 0u32;
        let mut last_status: Option<u16> = None;
        let mut last_error: Option<String> = None;

        let first = &requests[0];
        if requests.iter().any(|r| {
            r.api_version != first.api_version
                || r.feature_version != first.feature_version
                || r.run_id != first.run_id
                || r.symbol != first.symbol
                || r.timeframe != first.timeframe
        }) {
            return AgentBatchCallResult {
                info: AgentCallInfo {
                    attempts: 0,
                    duration_ms: 0,
                    status: None,
                    error: Some(
                        "batch requests must share api_version/feature_version/run_id/symbol/timeframe"
                            .to_string(),
                    ),
                },
                responses: None,
            };
        }

        let batch = ActionBatchRequest {
            api_version: first.api_version.clone(),
            feature_version: first.feature_version.clone(),
            run_id: first.run_id.clone(),
            symbol: first.symbol.clone(),
            timeframe: first.timeframe.clone(),
            items: requests
                .iter()
                .map(|r| kairos_domain::services::agent::ActionBatchItem {
                    timestamp: r.timestamp.clone(),
                    observation: r.observation.clone(),
                    portfolio_state: PortfolioState {
                        cash: r.portfolio_state.cash,
                        position_qty: r.portfolio_state.position_qty,
                        position_avg_price: r.portfolio_state.position_avg_price,
                        equity: r.portfolio_state.equity,
                    },
                })
                .collect(),
        };

        while attempts <= self.retries {
            attempts += 1;
            let response = self.client.post(&endpoint).json(&batch).send();
            match response {
                Ok(resp) => {
                    last_status = Some(resp.status().as_u16());
                    if resp.status() == StatusCode::OK {
                        match resp.json::<ActionBatchResponse>() {
                            Ok(parsed) => {
                                if parsed.items.len() != requests.len() {
                                    last_error = Some(format!(
                                        "agent batch size mismatch: expected {} items, got {}",
                                        requests.len(),
                                        parsed.items.len()
                                    ));
                                    break;
                                }
                                for item in &parsed.items {
                                    if let Err(err) = validate_action_response(item) {
                                        last_error = Some(err);
                                        break;
                                    }
                                }
                                if last_error.is_none() {
                                    return AgentBatchCallResult {
                                        info: AgentCallInfo {
                                            attempts,
                                            duration_ms: start.elapsed().as_millis() as u64,
                                            status: last_status,
                                            error: None,
                                        },
                                        responses: Some(parsed.items),
                                    };
                                }
                                break;
                            }
                            Err(err) => {
                                last_error =
                                    Some(format!("failed to parse agent batch response: {err}"));
                                break;
                            }
                        }
                    }

                    if resp.status().is_server_error() && attempts <= self.retries {
                        continue;
                    }
                    last_error = Some(format!(
                        "agent http error: status {}",
                        resp.status().as_u16()
                    ));
                    break;
                }
                Err(err) => {
                    last_error = Some(format!("agent request failed: {err}"));
                    if attempts <= self.retries {
                        continue;
                    }
                    break;
                }
            }
        }

        AgentBatchCallResult {
            info: AgentCallInfo {
                attempts,
                duration_ms: start.elapsed().as_millis() as u64,
                status: last_status,
                error: last_error.or_else(|| Some("agent request failed after retries".to_string())),
            },
            responses: None,
        }
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

fn validate_action_response(response: &ActionResponse) -> Result<(), String> {
    let action_type = response.action_type.to_uppercase();
    if action_type != "BUY" && action_type != "SELL" && action_type != "HOLD" {
        return Err(format!("invalid action_type: {}", response.action_type));
    }
    if !response.size.is_finite() || response.size < 0.0 {
        return Err(format!("invalid size: {}", response.size));
    }
    if let Some(confidence) = response.confidence {
        if !confidence.is_finite() || !(0.0..=1.0).contains(&confidence) {
            return Err(format!("invalid confidence: {}", confidence));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::AgentClient;
    use kairos_domain::value_objects::action_type::ActionType;

    #[test]
    fn agent_client_fallback_is_hold() {
        let client = AgentClient::new(
            "http://127.0.0.1:8000".to_string(),
            200,
            "v1".to_string(),
            "v1".to_string(),
            0,
            ActionType::Hold,
        )
        .expect("agent client");
        let response = client.fallback_response();
        assert_eq!(response.action_type, "HOLD");
    }
}
