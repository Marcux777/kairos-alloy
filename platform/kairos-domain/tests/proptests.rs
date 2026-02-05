use kairos_domain::entities::metrics::{MetricsConfig, MetricsState};
use kairos_domain::entities::risk::RiskLimits;
use kairos_domain::services::engine::backtest::{BacktestRunner, OrderSizeMode};
use kairos_domain::services::features::{FeatureBuilder, FeatureConfig, ReturnMode};
use kairos_domain::services::market_data_source::VecBarSource;
use kairos_domain::services::strategy::BuyAndHold;
use kairos_domain::value_objects::bar::Bar;
use kairos_domain::value_objects::equity_point::EquityPoint;
use proptest::prelude::*;

fn bar(ts: i64, close: f64) -> Bar {
    Bar {
        symbol: "BTCUSD".to_string(),
        timestamp: ts,
        open: close,
        high: close,
        low: close,
        close,
        volume: 1.0,
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 64,
        .. ProptestConfig::default()
    })]

    #[test]
    fn feature_pipeline_outputs_are_finite(prices in prop::collection::vec(0.01f64..10_000.0, 2..80)) {
        let mut builder = FeatureBuilder::new(FeatureConfig {
            return_mode: ReturnMode::Log,
            sma_windows: vec![2, 5],
            volatility_windows: vec![3],
            rsi_enabled: true,
        });

        for (idx, close) in prices.iter().copied().enumerate() {
            let obs = builder.update(&bar(idx as i64, close), Some(&[0.1, 0.2]));
            prop_assert!(!obs.values.is_empty());
            prop_assert!(obs.values.iter().all(|v| v.is_finite()));
        }
    }

    #[test]
    fn metrics_drawdown_is_bounded_for_positive_equity(equity in prop::collection::vec(0.01f64..100_000.0, 2..200)) {
        let mut state = MetricsState::new(MetricsConfig::default());
        for (idx, e) in equity.iter().copied().enumerate() {
            state.record_equity(EquityPoint {
                timestamp: idx as i64,
                equity: e,
                cash: e,
                position_qty: 0.0,
                unrealized_pnl: 0.0,
                realized_pnl: 0.0,
            });
        }
        let summary = state.summary();
        prop_assert!(summary.sharpe.is_finite());
        prop_assert!((0.0..=1.0).contains(&summary.max_drawdown));
    }

    #[test]
    fn engine_never_records_negative_cash(prices in prop::collection::vec(0.01f64..10_000.0, 2..80)) {
        let bars: Vec<Bar> = prices
            .iter()
            .copied()
            .enumerate()
            .map(|(idx, close)| bar(idx as i64 + 1, close))
            .collect();

        let data = VecBarSource::new(bars);
        let strategy = BuyAndHold::new(1.0);
        let mut runner = BacktestRunner::new(
            "prop_cash".to_string(),
            strategy,
            data,
            RiskLimits::default(),
            10_000.0,
            MetricsConfig::default(),
            0.0,
            0.0,
            "BTCUSD".to_string(),
            OrderSizeMode::PctEquity,
        );
        let result = runner.run();

        prop_assert!(!result.equity.is_empty());
        prop_assert!(result.equity.iter().all(|p| p.cash.is_finite() && p.cash >= -1e-9));
    }
}
