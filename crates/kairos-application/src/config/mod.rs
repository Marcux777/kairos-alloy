use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum AgentMode {
    Remote,
    Baseline,
    Hold,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub run: RunConfig,
    pub db: DbConfig,
    pub paths: PathsConfig,
    pub costs: CostsConfig,
    pub risk: RiskConfig,
    pub orders: Option<OrdersConfig>,
    pub execution: Option<ExecutionConfig>,
    pub features: FeaturesConfig,
    pub agent: AgentConfig,
    pub strategy: Option<StrategyConfig>,
    pub metrics: Option<MetricsConfig>,
    pub data_quality: Option<DataQualityConfig>,
    pub paper: Option<PaperConfig>,
    pub report: Option<ReportConfig>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct RunConfig {
    pub run_id: String,
    pub symbol: String,
    pub timeframe: String,
    pub initial_capital: f64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct DbConfig {
    pub url: Option<String>,
    pub ohlcv_table: String,
    pub exchange: String,
    pub market: String,
    pub source_timeframe: Option<String>,
    pub pool_max_size: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct PathsConfig {
    pub sentiment_path: Option<String>,
    pub out_dir: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct CostsConfig {
    pub fee_bps: f64,
    pub slippage_bps: f64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct RiskConfig {
    pub max_position_qty: f64,
    pub max_drawdown_pct: f64,
    pub max_exposure_pct: f64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct OrdersConfig {
    pub size_mode: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct ExecutionConfig {
    pub model: Option<String>,
    pub latency_bars: Option<u64>,
    pub buy_kind: Option<String>,
    pub sell_kind: Option<String>,
    pub price_reference: Option<String>,
    pub limit_offset_bps: Option<f64>,
    pub stop_offset_bps: Option<f64>,
    pub spread_bps: Option<f64>,
    pub max_fill_pct_of_volume: Option<f64>,
    pub tif: Option<String>,
    pub expire_after_bars: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct FeaturesConfig {
    pub return_mode: kairos_domain::services::features::ReturnMode,
    pub sma_windows: Vec<u64>,
    pub volatility_windows: Option<Vec<u64>>,
    pub rsi_enabled: bool,
    pub sentiment_lag: String,
    pub sentiment_missing: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct AgentConfig {
    pub mode: AgentMode,
    pub url: String,
    pub timeout_ms: u64,
    pub retries: u32,
    pub fallback_action: kairos_domain::value_objects::action_type::ActionType,
    pub api_version: String,
    pub feature_version: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct StrategyConfig {
    pub baseline: String,
    pub sma_short: Option<u64>,
    pub sma_long: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct MetricsConfig {
    pub risk_free_rate: Option<f64>,
    pub annualization_factor: Option<f64>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct DataQualityConfig {
    pub max_gaps: Option<usize>,
    pub max_missing_bars: Option<usize>,
    pub max_duplicates: Option<usize>,
    pub max_out_of_order: Option<usize>,
    pub max_invalid_close: Option<usize>,
    pub max_sentiment_missing: Option<usize>,
    pub max_sentiment_invalid: Option<usize>,
    pub max_sentiment_dropped: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct PaperConfig {
    pub replay_scale: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct ReportConfig {
    pub html: Option<bool>,
}

pub fn load_config(path: &Path) -> Result<Config, String> {
    let (config, _source) = load_config_with_source(path)?;
    Ok(config)
}

pub fn load_config_with_source(path: &Path) -> Result<(Config, String), String> {
    let contents = fs::read_to_string(path)
        .map_err(|err| format!("failed to read config {}: {}", path.display(), err))?;
    let config = toml::from_str(&contents)
        .map_err(|err| format!("failed to parse TOML {}: {}", path.display(), err))?;
    Ok((config, contents))
}

pub fn to_toml_pretty(config: &Config) -> Result<String, String> {
    toml::to_string_pretty(config)
        .map_err(|err| format!("failed to serialize config as TOML: {err}"))
}

#[cfg(test)]
mod tests {
    use super::Config;

    fn parse_config(toml_str: &str) -> Config {
        toml::from_str(toml_str).expect("config should parse")
    }

    #[test]
    fn parse_config_rejects_malformed_toml() {
        let err = toml::from_str::<Config>("[run\nrun_id = 1").expect_err("malformed");
        let msg = err.to_string();
        assert!(!msg.is_empty());
    }

    #[test]
    fn parse_config_rejects_unknown_fields() {
        let toml_str = r#"
[run]
run_id = "x"
symbol = "BTCUSD"
timeframe = "1m"
initial_capital = 100.0

[db]
ohlcv_table = "ohlcv_candles"
exchange = "kucoin"
market = "spot"

[paths]
out_dir = "runs/"

[costs]
fee_bps = 0.0
slippage_bps = 0.0

[risk]
max_position_qty = 1.0
max_drawdown_pct = 1.0
max_exposure_pct = 1.0

[features]
return_mode = "pct"
sma_windows = [2]
rsi_enabled = false
sentiment_lag = "0s"

[agent]
mode = "baseline"
url = "http://127.0.0.1:8000"
timeout_ms = 200
retries = 0
fallback_action = "HOLD"
api_version = "v1"
feature_version = "v1"

unknown_field = 123
"#;

        let err = toml::from_str::<Config>(toml_str).expect_err("unknown field should fail");
        assert!(err.to_string().to_lowercase().contains("unknown field"));
    }

    #[test]
    fn parse_minimal_config() {
        let toml_str = r#"
[run]
run_id = "btc_1m_2024q1"
symbol = "BTCUSD"
timeframe = "1m"
initial_capital = 10000.0

[db]
url = "postgres://kairos:CHANGE_ME@localhost:5432/kairos"
ohlcv_table = "ohlcv_candles"
exchange = "kucoin"
market = "spot"

[paths]
sentiment_path = "data/sentiment.csv"
out_dir = "runs/"

[costs]
fee_bps = 10.0
slippage_bps = 5.0

[risk]
max_position_qty = 1.0
max_drawdown_pct = 0.30
max_exposure_pct = 1.00

[features]
return_mode = "log"
sma_windows = [10, 50]
rsi_enabled = false
sentiment_lag = "5m"

[agent]
mode = "remote"
url = "http://127.0.0.1:8000"
timeout_ms = 200
retries = 1
fallback_action = "HOLD"
api_version = "v1"
feature_version = "v1"

[data_quality]
max_gaps = 0
max_duplicates = 0
max_out_of_order = 0

[paper]
replay_scale = 60
"#;

        let config = parse_config(toml_str);
        assert_eq!(config.run.symbol, "BTCUSD");
        assert_eq!(config.features.sma_windows, vec![10, 50]);
    }

    #[test]
    fn parse_config_allows_db_url_omitted() {
        let toml_str = r#"
[run]
run_id = "btc_1m_2024q1"
symbol = "BTCUSD"
timeframe = "1m"
initial_capital = 10000.0

[db]
ohlcv_table = "ohlcv_candles"
exchange = "kucoin"
market = "spot"

[paths]
sentiment_path = "data/sentiment.csv"
out_dir = "runs/"

[costs]
fee_bps = 10.0
slippage_bps = 5.0

[risk]
max_position_qty = 1.0
max_drawdown_pct = 0.30
max_exposure_pct = 1.00

[features]
return_mode = "log"
sma_windows = [10, 50]
rsi_enabled = false
sentiment_lag = "5m"

[agent]
mode = "remote"
url = "http://127.0.0.1:8000"
timeout_ms = 200
retries = 1
fallback_action = "HOLD"
api_version = "v1"
feature_version = "v1"
"#;

        let config = parse_config(toml_str);
        assert!(config.db.url.is_none());
    }

    #[test]
    fn parse_config_allows_db_pool_max_size() {
        let toml_str = r#"
[run]
run_id = "x"
symbol = "BTCUSD"
timeframe = "1m"
initial_capital = 100.0

[db]
ohlcv_table = "ohlcv_candles"
exchange = "kucoin"
market = "spot"
pool_max_size = 4

[paths]
out_dir = "runs/"

[costs]
fee_bps = 0.0
slippage_bps = 0.0

[risk]
max_position_qty = 1.0
max_drawdown_pct = 1.0
max_exposure_pct = 1.0

[features]
return_mode = "pct"
sma_windows = [2]
rsi_enabled = false
sentiment_lag = "0s"

[agent]
mode = "baseline"
url = "http://127.0.0.1:8000"
timeout_ms = 200
retries = 0
fallback_action = "HOLD"
api_version = "v1"
feature_version = "v1"
"#;

        let config = parse_config(toml_str);
        assert_eq!(config.db.pool_max_size, Some(4));
    }
}
