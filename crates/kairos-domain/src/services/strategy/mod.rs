use crate::entities::portfolio::Portfolio;
use crate::repositories::agent as agent_port;
use crate::services::agent::{ActionRequest, ActionResponse, PortfolioState};
use crate::services::audit::AuditEvent;
use crate::services::features::{FeatureBuilder, Observation};
use crate::services::sentiment::SentimentPoint;
use crate::value_objects::action::Action;
use crate::value_objects::action_type::ActionType;
use crate::value_objects::bar::Bar;
use crate::value_objects::tick::Tick;
use chrono::{DateTime, TimeZone, Utc};
use chrono::{NaiveDate, NaiveDateTime};
use serde_json::json;

pub trait Strategy {
    fn name(&self) -> &str;

    fn on_bar(&mut self, _bar: &Bar, _portfolio: &Portfolio) -> Action {
        Action::hold()
    }

    fn on_tick(&mut self, _tick: &Tick) {}

    fn drain_audit_events(&mut self) -> Vec<AuditEvent> {
        Vec::new()
    }
}

pub struct BuyAndHold {
    has_bought: bool,
    size: f64,
}

impl BuyAndHold {
    pub fn new(size: f64) -> Self {
        Self {
            has_bought: false,
            size,
        }
    }
}

impl Strategy for BuyAndHold {
    fn name(&self) -> &str {
        "buy_and_hold"
    }

    fn on_bar(&mut self, _bar: &Bar, _portfolio: &Portfolio) -> Action {
        if self.has_bought {
            return Action::hold();
        }
        self.has_bought = true;
        Action {
            action_type: ActionType::Buy,
            size: self.size,
        }
    }
}

pub struct SimpleSma {
    short_window: usize,
    long_window: usize,
    prices: Vec<f64>,
}

impl SimpleSma {
    pub fn new(short_window: usize, long_window: usize) -> Self {
        Self {
            short_window,
            long_window,
            prices: Vec::new(),
        }
    }

    fn sma(&self, window: usize) -> Option<f64> {
        if self.prices.len() < window || window == 0 {
            return None;
        }
        let slice = &self.prices[self.prices.len() - window..];
        Some(slice.iter().sum::<f64>() / window as f64)
    }
}

impl Strategy for SimpleSma {
    fn name(&self) -> &str {
        "simple_sma"
    }

    fn on_bar(&mut self, bar: &Bar, portfolio: &Portfolio) -> Action {
        self.prices.push(bar.close);
        if self.prices.len() < self.long_window {
            return Action::hold();
        }

        let (Some(short), Some(long)) = (self.sma(self.short_window), self.sma(self.long_window))
        else {
            return Action::hold();
        };

        if short > long && portfolio.position_qty(&bar.symbol) <= 0.0 {
            return Action {
                action_type: ActionType::Buy,
                size: 1.0,
            };
        }

        if short < long && portfolio.position_qty(&bar.symbol) > 0.0 {
            return Action {
                action_type: ActionType::Sell,
                size: portfolio.position_qty(&bar.symbol),
            };
        }

        Action::hold()
    }
}

pub struct HoldStrategy;

impl Strategy for HoldStrategy {
    fn name(&self) -> &str {
        "hold"
    }
}

pub struct AgentStrategy {
    pub run_id: String,
    pub symbol: String,
    pub timeframe: String,
    pub api_version: String,
    pub feature_version: String,
    pub agent_url: String,
    pub fallback_action: ActionType,
    pub agent: Box<dyn agent_port::AgentClient>,
    pub features: FeatureBuilder,
    pub sentiment: Vec<Option<SentimentPoint>>,
    index: usize,
    audit_events: Vec<AuditEvent>,
}

impl AgentStrategy {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        run_id: String,
        symbol: String,
        timeframe: String,
        api_version: String,
        feature_version: String,
        agent_url: String,
        fallback_action: ActionType,
        agent: Box<dyn agent_port::AgentClient>,
        features: FeatureBuilder,
        sentiment: Vec<Option<SentimentPoint>>,
    ) -> Self {
        Self {
            run_id,
            symbol,
            timeframe,
            api_version,
            feature_version,
            agent_url,
            fallback_action,
            agent,
            features,
            sentiment,
            index: 0,
            audit_events: Vec::new(),
        }
    }

    fn build_request(
        &self,
        bar: &Bar,
        observation: &Observation,
        portfolio: &Portfolio,
    ) -> ActionRequest {
        let dt: DateTime<Utc> = match Utc.timestamp_opt(bar.timestamp, 0) {
            chrono::LocalResult::Single(dt) => dt,
            _ => {
                let naive = NaiveDate::from_ymd_opt(1970, 1, 1)
                    .and_then(|d| d.and_hms_opt(0, 0, 0))
                    .unwrap_or(NaiveDateTime::MIN);
                DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc)
            }
        };
        ActionRequest {
            api_version: self.api_version.clone(),
            feature_version: self.feature_version.clone(),
            run_id: self.run_id.clone(),
            timestamp: dt.to_rfc3339(),
            symbol: self.symbol.clone(),
            timeframe: self.timeframe.clone(),
            observation: observation.values.clone(),
            portfolio_state: PortfolioState {
                cash: portfolio.cash(),
                position_qty: portfolio.position_qty(&bar.symbol),
                position_avg_price: portfolio.position_avg_price(&bar.symbol),
                equity: portfolio.equity(&bar.symbol, bar.close),
            },
        }
    }

    fn to_action(response: &ActionResponse) -> Action {
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

    fn fallback_response(&self) -> ActionResponse {
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

impl Strategy for AgentStrategy {
    fn name(&self) -> &str {
        "agent_remote"
    }

    fn on_bar(&mut self, bar: &Bar, portfolio: &Portfolio) -> Action {
        let sentiment_values = self
            .sentiment
            .get(self.index)
            .and_then(|point| point.as_ref())
            .map(|point| point.values.as_slice());
        let observation = self.features.update(bar, sentiment_values);
        let request = self.build_request(bar, &observation, portfolio);

        let result = self.agent.act(&request);
        let (response, used_fallback) = match result {
            Ok(response) => (response, false),
            Err(err) => {
                self.audit_events.push(AuditEvent {
                    run_id: self.run_id.clone(),
                    timestamp: bar.timestamp,
                    stage: "agent".to_string(),
                    symbol: Some(self.symbol.clone()),
                    action: "error".to_string(),
                    error: Some(err),
                    details: json!({
                        "url": self.agent_url.clone(),
                    }),
                });
                (self.fallback_response(), true)
            }
        };

        self.audit_events.push(AuditEvent {
            run_id: self.run_id.clone(),
            timestamp: bar.timestamp,
            stage: "agent".to_string(),
            symbol: Some(self.symbol.clone()),
            action: if used_fallback {
                "fallback".to_string()
            } else {
                "call".to_string()
            },
            error: None,
            details: json!({
                "url": self.agent_url.clone(),
                "used_fallback": used_fallback,
                "response_action_type": response.action_type,
                "response_size": response.size,
                "portfolio_state": {
                    "cash": portfolio.cash(),
                    "position_qty": portfolio.position_qty(&bar.symbol),
                    "position_avg_price": portfolio.position_avg_price(&bar.symbol),
                    "equity": portfolio.equity(&bar.symbol, bar.close),
                },
                "observation_len": observation.values.len(),
            }),
        });

        self.index += 1;
        Self::to_action(&response)
    }

    fn drain_audit_events(&mut self) -> Vec<AuditEvent> {
        std::mem::take(&mut self.audit_events)
    }
}

#[allow(clippy::large_enum_variant)]
pub enum StrategyKind {
    BuyAndHold(BuyAndHold),
    SimpleSma(SimpleSma),
    Agent(AgentStrategy),
    Hold(HoldStrategy),
}

impl Strategy for StrategyKind {
    fn name(&self) -> &str {
        match self {
            StrategyKind::BuyAndHold(strategy) => strategy.name(),
            StrategyKind::SimpleSma(strategy) => strategy.name(),
            StrategyKind::Agent(strategy) => strategy.name(),
            StrategyKind::Hold(strategy) => strategy.name(),
        }
    }

    fn on_bar(&mut self, bar: &Bar, portfolio: &Portfolio) -> Action {
        match self {
            StrategyKind::BuyAndHold(strategy) => strategy.on_bar(bar, portfolio),
            StrategyKind::SimpleSma(strategy) => strategy.on_bar(bar, portfolio),
            StrategyKind::Agent(strategy) => strategy.on_bar(bar, portfolio),
            StrategyKind::Hold(strategy) => strategy.on_bar(bar, portfolio),
        }
    }

    fn drain_audit_events(&mut self) -> Vec<AuditEvent> {
        match self {
            StrategyKind::BuyAndHold(strategy) => strategy.drain_audit_events(),
            StrategyKind::SimpleSma(strategy) => strategy.drain_audit_events(),
            StrategyKind::Agent(strategy) => strategy.drain_audit_events(),
            StrategyKind::Hold(strategy) => strategy.drain_audit_events(),
        }
    }
}
