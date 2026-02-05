use kairos_domain::entities::metrics::MetricsConfig;
use kairos_domain::entities::risk::RiskLimits;
use kairos_domain::repositories::agent::AgentClient as AgentPort;
use kairos_domain::services::agent::{
    ActionBatchRequest, ActionBatchResponse, ActionRequest, ActionResponse,
};
use kairos_domain::services::engine::backtest::{BacktestResults, BacktestRunner, OrderSizeMode};
use kairos_domain::services::features::{FeatureBuilder, FeatureConfig, ReturnMode};
use kairos_domain::services::market_data_source::VecBarSource;
use kairos_domain::services::strategy::{AgentStrategy, BuyAndHold, SimpleSma, Strategy};
use kairos_domain::value_objects::action_type::ActionType;
use kairos_domain::value_objects::bar::Bar;
use kairos_domain::value_objects::side::Side;
use std::cell::Cell;

fn make_bar(symbol: &str, ts: i64, close: f64) -> Bar {
    Bar {
        symbol: symbol.to_string(),
        timestamp: ts,
        open: close,
        high: close,
        low: close,
        close,
        volume: 1000.0,
    }
}

fn bars_trend_cross(symbol: &str) -> Vec<Bar> {
    let mut bars = Vec::with_capacity(30);
    for ts in 1..=10 {
        bars.push(make_bar(symbol, ts, 100.0));
    }
    for i in 0..10 {
        let ts = 11 + i;
        let close = 100.0 + (i as f64) * (10.0 / 9.0);
        bars.push(make_bar(symbol, ts, close));
    }
    for i in 0..10 {
        let ts = 21 + i;
        let close = 110.0 - (i as f64) * (20.0 / 9.0);
        bars.push(make_bar(symbol, ts, close));
    }
    bars
}

fn run_with_strategy<S: Strategy>(run_id: &str, strategy: S, bars: Vec<Bar>) -> BacktestResults {
    let data = VecBarSource::new(bars);
    let mut runner = BacktestRunner::new(
        run_id.to_string(),
        strategy,
        data,
        RiskLimits::default(),
        10_000.0,
        MetricsConfig::default(),
        0.0,
        0.0,
        "BTCUSD".to_string(),
        OrderSizeMode::Quantity,
    );
    runner.run()
}

fn resp(action_type: &str, size: f64) -> ActionResponse {
    ActionResponse {
        action_type: action_type.to_string(),
        size,
        confidence: None,
        model_version: None,
        latency_ms: None,
        reason: None,
    }
}

struct ScriptedAgent {
    script: Vec<ActionResponse>,
    idx: Cell<usize>,
}

impl ScriptedAgent {
    fn new(script: Vec<ActionResponse>) -> Self {
        Self {
            script,
            idx: Cell::new(0),
        }
    }
}

impl AgentPort for ScriptedAgent {
    fn act(&self, _request: &ActionRequest) -> Result<ActionResponse, String> {
        let i = self.idx.get();
        self.idx.set(i.saturating_add(1));
        let response = self
            .script
            .get(i)
            .cloned()
            .unwrap_or_else(|| resp("HOLD", 0.0));
        Ok(response)
    }

    fn act_batch(&self, _request: &ActionBatchRequest) -> Result<ActionBatchResponse, String> {
        Err("not implemented".to_string())
    }
}

#[test]
fn buy_and_hold_regression_single_buy() {
    let bars = bars_trend_cross("BTCUSD");
    let bar_count = bars.len();
    let expected_buy_ts = bars[1].timestamp;
    let strategy = BuyAndHold::new(1.0);
    let results = run_with_strategy("reg_bnh", strategy, bars);

    assert_eq!(results.trades.len(), 1);
    assert_eq!(results.trades[0].side, Side::Buy);
    assert_eq!(results.trades[0].timestamp, expected_buy_ts);
    assert_eq!(results.summary.bars_processed, bar_count);
}

#[test]
fn simple_sma_regression_cross_buy_then_sell() {
    let bars = bars_trend_cross("BTCUSD");
    let bar_count = bars.len();
    let strategy = SimpleSma::new(3, 5);
    let results = run_with_strategy("reg_sma", strategy, bars);

    let buys: Vec<_> = results
        .trades
        .iter()
        .filter(|t| t.side == Side::Buy)
        .collect();
    let sells: Vec<_> = results
        .trades
        .iter()
        .filter(|t| t.side == Side::Sell)
        .collect();

    assert_eq!(buys.len(), 1);
    assert!(!sells.is_empty());
    assert!(buys[0].timestamp < sells[0].timestamp);
    assert_eq!(results.summary.bars_processed, bar_count);
}

#[test]
fn agent_strategy_regression_scripted_actions() {
    let bars: Vec<Bar> = (1..=6).map(|ts| make_bar("BTCUSD", ts, 100.0)).collect();
    let script = vec![
        resp("HOLD", 0.0),
        resp("BUY", 1.0),
        resp("HOLD", 0.0),
        resp("SELL", 1.0),
        resp("HOLD", 0.0),
        resp("HOLD", 0.0),
    ];

    let agent: Box<dyn AgentPort> = Box::new(ScriptedAgent::new(script));
    let features = FeatureBuilder::new(FeatureConfig {
        return_mode: ReturnMode::Pct,
        sma_windows: vec![2],
        volatility_windows: vec![],
        rsi_enabled: false,
    });

    let sentiment = vec![None; bars.len()];
    let strategy = AgentStrategy::new(
        "reg_agent".to_string(),
        "BTCUSD".to_string(),
        "1m".to_string(),
        "v1".to_string(),
        "v1".to_string(),
        "http://example".to_string(),
        ActionType::Hold,
        agent,
        features,
        sentiment,
    );

    let results = run_with_strategy("reg_agent_run", strategy, bars);

    let buys: Vec<_> = results
        .trades
        .iter()
        .filter(|t| t.side == Side::Buy)
        .collect();
    let sells: Vec<_> = results
        .trades
        .iter()
        .filter(|t| t.side == Side::Sell)
        .collect();
    assert_eq!(buys.len(), 1);
    assert_eq!(sells.len(), 1);
    assert_eq!(buys[0].timestamp, 3);
    assert_eq!(sells[0].timestamp, 5);

    assert!(results
        .audit_events
        .iter()
        .any(|e| e.stage == "agent" && e.action == "call"));
}
