pub mod agents;
pub mod data;
pub mod engine;
pub mod features;
pub mod metrics;
pub mod portfolio;
pub mod report;
pub mod risk;
pub mod strategy;
pub mod types;

pub use engine::backtest;
pub use engine::market_data;

pub fn engine_name() -> &'static str {
    "kairos-alloy"
}
