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
                error: last_error
                    .or_else(|| Some("agent request failed after retries".to_string())),
            },
            response: None,
        }
    }

    pub fn act_batch(&self, batch: &ActionBatchRequest) -> Result<ActionBatchResponse, String> {
        let result = self.act_batch_detailed(batch);
        match result.responses {
            Some(items) => Ok(ActionBatchResponse { items }),
            None => Err(result
                .info
                .error
                .unwrap_or_else(|| "agent batch request failed".to_string())),
        }
    }

    pub fn act_batch_detailed(&self, batch: &ActionBatchRequest) -> AgentBatchCallResult {
        if batch.items.is_empty() {
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

        while attempts <= self.retries {
            attempts += 1;
            let response = self.client.post(&endpoint).json(batch).send();
            match response {
                Ok(resp) => {
                    last_status = Some(resp.status().as_u16());
                    if resp.status() == StatusCode::OK {
                        match resp.json::<ActionBatchResponse>() {
                            Ok(parsed) => {
                                if parsed.items.len() != batch.items.len() {
                                    last_error = Some(format!(
                                        "agent batch size mismatch: expected {} items, got {}",
                                        batch.items.len(),
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
                error: last_error
                    .or_else(|| Some("agent request failed after retries".to_string())),
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

impl kairos_domain::repositories::agent::AgentClient for AgentClient {
    fn act(&self, request: &ActionRequest) -> Result<ActionResponse, String> {
        AgentClient::act(self, request)
    }

    fn act_batch(&self, request: &ActionBatchRequest) -> Result<ActionBatchResponse, String> {
        AgentClient::act_batch(self, request)
    }
}

#[cfg(test)]
mod tests {
    use super::{ActionBatchItem, ActionBatchRequest, ActionRequest, AgentClient, PortfolioState};
    use kairos_domain::value_objects::action_type::ActionType;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn http_response(status: u16, reason: &str, content_type: &str, body: &str) -> String {
        format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    fn spawn_server(responses: Vec<String>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
        let addr = listener.local_addr().expect("local addr");

        thread::spawn(move || {
            for response in responses {
                let (mut stream, _) = listener.accept().expect("accept");
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf);
                stream
                    .write_all(response.as_bytes())
                    .expect("write response");
            }
        });

        format!("http://{}", addr)
    }

    fn sample_request() -> ActionRequest {
        ActionRequest {
            api_version: "v1".to_string(),
            feature_version: "v1".to_string(),
            run_id: "run_1".to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            symbol: "BTCUSD".to_string(),
            timeframe: "1m".to_string(),
            observation: vec![1.0, 2.0, 3.0],
            portfolio_state: PortfolioState {
                cash: 1000.0,
                position_qty: 0.0,
                position_avg_price: 0.0,
                equity: 1000.0,
            },
        }
    }

    fn sample_batch() -> ActionBatchRequest {
        ActionBatchRequest {
            api_version: "v1".to_string(),
            feature_version: "v1".to_string(),
            run_id: "run_1".to_string(),
            symbol: "BTCUSD".to_string(),
            timeframe: "1m".to_string(),
            items: vec![
                ActionBatchItem {
                    timestamp: "2026-01-01T00:00:00Z".to_string(),
                    observation: vec![1.0],
                    portfolio_state: PortfolioState {
                        cash: 1000.0,
                        position_qty: 0.0,
                        position_avg_price: 0.0,
                        equity: 1000.0,
                    },
                },
                ActionBatchItem {
                    timestamp: "2026-01-01T00:00:01Z".to_string(),
                    observation: vec![2.0],
                    portfolio_state: PortfolioState {
                        cash: 1000.0,
                        position_qty: 0.0,
                        position_avg_price: 0.0,
                        equity: 1000.0,
                    },
                },
            ],
        }
    }

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

    #[test]
    fn act_retries_on_server_error_then_succeeds() {
        let ok_body = r#"{"action_type":"HOLD","size":0.0,"confidence":null,"model_version":null,"latency_ms":null}"#;
        let base_url = spawn_server(vec![
            http_response(500, "Internal Server Error", "text/plain", "oops"),
            http_response(200, "OK", "application/json", ok_body),
        ]);

        let client = AgentClient::new(
            base_url,
            500,
            "v1".to_string(),
            "v1".to_string(),
            3,
            ActionType::Hold,
        )
        .expect("agent client");

        let detailed = client.act_detailed(&sample_request());
        assert_eq!(detailed.info.attempts, 2);
        assert_eq!(detailed.info.status, Some(200));
        assert_eq!(detailed.response.as_ref().unwrap().action_type, "HOLD");
    }

    #[test]
    fn act_does_not_retry_on_client_error() {
        let base_url = spawn_server(vec![http_response(
            400,
            "Bad Request",
            "text/plain",
            "nope",
        )]);

        let client = AgentClient::new(
            base_url,
            500,
            "v1".to_string(),
            "v1".to_string(),
            3,
            ActionType::Hold,
        )
        .expect("agent client");

        let detailed = client.act_detailed(&sample_request());
        assert_eq!(detailed.info.attempts, 1);
        assert!(detailed.response.is_none());
        assert!(detailed
            .info
            .error
            .unwrap_or_default()
            .contains("status 400"));
    }

    #[test]
    fn act_stops_on_invalid_action_response() {
        let invalid_body = r#"{"action_type":"NOPE","size":0.0,"confidence":null,"model_version":null,"latency_ms":null}"#;
        let base_url = spawn_server(vec![http_response(
            200,
            "OK",
            "application/json",
            invalid_body,
        )]);

        let client = AgentClient::new(
            base_url,
            500,
            "v1".to_string(),
            "v1".to_string(),
            3,
            ActionType::Hold,
        )
        .expect("agent client");

        let detailed = client.act_detailed(&sample_request());
        assert_eq!(detailed.info.attempts, 1);
        assert!(detailed.response.is_none());
        assert!(detailed
            .info
            .error
            .unwrap_or_default()
            .contains("invalid action_type"));
    }

    #[test]
    fn act_batch_retries_on_server_error_then_succeeds() {
        let ok_body = r#"{"items":[{"action_type":"HOLD","size":0.0,"confidence":null,"model_version":null,"latency_ms":null},{"action_type":"HOLD","size":0.0,"confidence":null,"model_version":null,"latency_ms":null}]}"#;
        let base_url = spawn_server(vec![
            http_response(500, "Internal Server Error", "text/plain", "oops"),
            http_response(200, "OK", "application/json", ok_body),
        ]);

        let client = AgentClient::new(
            base_url,
            500,
            "v1".to_string(),
            "v1".to_string(),
            3,
            ActionType::Hold,
        )
        .expect("agent client");

        let detailed = client.act_batch_detailed(&sample_batch());
        assert_eq!(detailed.info.attempts, 2);
        assert_eq!(detailed.info.status, Some(200));
        assert_eq!(detailed.responses.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn act_batch_errors_on_size_mismatch() {
        let ok_body = r#"{"items":[{"action_type":"HOLD","size":0.0,"confidence":null,"model_version":null,"latency_ms":null}]}"#;
        let base_url = spawn_server(vec![http_response(200, "OK", "application/json", ok_body)]);

        let client = AgentClient::new(
            base_url,
            500,
            "v1".to_string(),
            "v1".to_string(),
            0,
            ActionType::Hold,
        )
        .expect("agent client");

        let detailed = client.act_batch_detailed(&sample_batch());
        assert_eq!(detailed.info.attempts, 1);
        assert!(detailed.responses.is_none());
        assert!(detailed
            .info
            .error
            .unwrap_or_default()
            .contains("batch size mismatch"));
    }
}
