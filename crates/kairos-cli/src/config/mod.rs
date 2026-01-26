use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub run: RunConfig,
    pub paths: PathsConfig,
    pub costs: CostsConfig,
    pub risk: RiskConfig,
    pub features: FeaturesConfig,
    pub agent: AgentConfig,
}

#[derive(Debug, Deserialize)]
pub struct RunConfig {
    pub run_id: String,
    pub symbol: String,
    pub timeframe: String,
    pub initial_capital: f64,
}

#[derive(Debug, Deserialize)]
pub struct PathsConfig {
    pub ohlcv_csv: String,
    pub sentiment_path: Option<String>,
    pub out_dir: String,
}

#[derive(Debug, Deserialize)]
pub struct CostsConfig {
    pub fee_bps: f64,
    pub slippage_bps: f64,
}

#[derive(Debug, Deserialize)]
pub struct RiskConfig {
    pub max_position_qty: f64,
    pub max_drawdown_pct: f64,
    pub max_exposure_pct: f64,
}

#[derive(Debug, Deserialize)]
pub struct FeaturesConfig {
    pub return_mode: String,
    pub sma_windows: Vec<u64>,
    pub rsi_enabled: bool,
    pub sentiment_lag: String,
}

#[derive(Debug, Deserialize)]
pub struct AgentConfig {
    pub mode: String,
    pub url: String,
    pub timeout_ms: u64,
    pub retries: u32,
    pub fallback_action: String,
    pub api_version: String,
    pub feature_version: String,
}

pub fn load_config(path: &Path) -> Result<Config, String> {
    let contents = fs::read_to_string(path)
        .map_err(|err| format!("failed to read config {}: {}", path.display(), err))?;
    toml::from_str(&contents)
        .map_err(|err| format!("failed to parse TOML {}: {}", path.display(), err))
}

#[cfg(test)]
mod tests {
    use super::{load_config, Config};
    use std::path::Path;

    fn parse_config(toml_str: &str) -> Config {
        toml::from_str(toml_str).expect("config should parse")
    }

    #[test]
    fn parse_minimal_config() {
        let toml_str = r#"
[run]
run_id = "btc_1m_2024q1"
symbol = "BTCUSD"
timeframe = "1m"
initial_capital = 10000.0

[paths]
ohlcv_csv = "data/btcusd_1m.csv"
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
        assert_eq!(config.run.symbol, "BTCUSD");
        assert_eq!(config.features.sma_windows, vec![10, 50]);
    }

    #[test]
    fn load_config_missing_file_returns_error() {
        let path = Path::new("/tmp/kairos-alloy-missing-config.toml");
        let err = load_config(path).expect_err("expected load to fail");
        assert!(err.contains("failed to read config"));
    }
}
