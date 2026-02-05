use crate::value_objects::bar::Bar;

mod rolling;

use rolling::{RollingRsi, RollingSma, RollingVar};

#[derive(Debug, Clone)]
pub struct Observation {
    pub values: Vec<f64>,
}

#[derive(Debug)]
pub struct FeatureConfig {
    pub return_mode: ReturnMode,
    pub sma_windows: Vec<usize>,
    pub volatility_windows: Vec<usize>,
    pub rsi_enabled: bool,
}

#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ReturnMode {
    Log,
    Pct,
}

pub struct FeatureBuilder {
    config: FeatureConfig,
    prev_close: Option<f64>,
    smas: Vec<RollingSma>,
    vols: Vec<RollingVar>,
    rsi: Option<RollingRsi>,
}

impl FeatureBuilder {
    pub fn new(config: FeatureConfig) -> Self {
        let smas = config
            .sma_windows
            .iter()
            .copied()
            .map(RollingSma::new)
            .collect();
        let vols = config
            .volatility_windows
            .iter()
            .copied()
            .map(RollingVar::new)
            .collect();
        let rsi = config
            .rsi_enabled
            .then_some(RollingRsi::new(14, config.return_mode));

        Self {
            config,
            prev_close: None,
            smas,
            vols,
            rsi,
        }
    }

    pub fn update(&mut self, bar: &Bar, sentiment: Option<&[f64]>) -> Observation {
        let mut values = Vec::new();
        let prev_close = self.prev_close;
        self.prev_close = Some(bar.close);

        let (ret, has_prev) = match prev_close {
            Some(prev_price) if prev_price > 0.0 => {
                let r = match self.config.return_mode {
                    ReturnMode::Log => (bar.close / prev_price).ln(),
                    ReturnMode::Pct => bar.close / prev_price - 1.0,
                };
                (r, true)
            }
            _ => (0.0, false),
        };
        values.push(ret);

        for sma in &mut self.smas {
            let sma = sma.update(bar.close).unwrap_or(0.0);
            values.push(sma);
        }

        if has_prev {
            for vol in &mut self.vols {
                values.push(vol.update(ret).unwrap_or(0.0));
            }
        } else {
            values.extend(std::iter::repeat_n(0.0, self.vols.len()));
        }

        if let Some(rsi) = &mut self.rsi {
            values.push(rsi.update(bar.close).unwrap_or(0.0));
        }

        if let Some(sentiment_values) = sentiment {
            values.extend_from_slice(sentiment_values);
        }

        Observation { values }
    }
}

#[cfg(test)]
mod tests {
    use super::{FeatureBuilder, FeatureConfig, ReturnMode};
    use crate::value_objects::bar::Bar;

    fn bar(price: f64) -> Bar {
        Bar {
            symbol: "BTCUSD".to_string(),
            timestamp: 0,
            open: price,
            high: price,
            low: price,
            close: price,
            volume: 1.0,
        }
    }

    #[test]
    fn returns_zero_for_first_bar() {
        let mut builder = FeatureBuilder::new(FeatureConfig {
            return_mode: ReturnMode::Pct,
            sma_windows: vec![],
            volatility_windows: vec![],
            rsi_enabled: false,
        });
        let obs = builder.update(&bar(100.0), None);
        assert_eq!(obs.values[0], 0.0);
    }

    #[test]
    fn computes_sma() {
        let mut builder = FeatureBuilder::new(FeatureConfig {
            return_mode: ReturnMode::Pct,
            sma_windows: vec![2],
            volatility_windows: vec![],
            rsi_enabled: false,
        });
        builder.update(&bar(10.0), None);
        let obs = builder.update(&bar(20.0), None);
        assert!((obs.values[1] - 15.0).abs() < 1e-6);
    }

    #[test]
    fn appends_sentiment_values() {
        let mut builder = FeatureBuilder::new(FeatureConfig {
            return_mode: ReturnMode::Pct,
            sma_windows: vec![],
            volatility_windows: vec![],
            rsi_enabled: false,
        });
        let obs = builder.update(&bar(10.0), Some(&[0.1, 0.2]));
        assert_eq!(obs.values.len(), 3);
    }

    #[test]
    fn computes_volatility() {
        let mut builder = FeatureBuilder::new(FeatureConfig {
            return_mode: ReturnMode::Pct,
            sma_windows: vec![],
            volatility_windows: vec![3],
            rsi_enabled: false,
        });
        builder.update(&bar(10.0), None);
        builder.update(&bar(11.0), None);
        builder.update(&bar(9.0), None);
        let obs = builder.update(&bar(10.0), None);
        assert!(obs.values[1] >= 0.0);
    }

    #[test]
    fn return_mode_log_vs_pct() {
        let mut pct = FeatureBuilder::new(FeatureConfig {
            return_mode: ReturnMode::Pct,
            sma_windows: vec![],
            volatility_windows: vec![],
            rsi_enabled: false,
        });
        let mut log = FeatureBuilder::new(FeatureConfig {
            return_mode: ReturnMode::Log,
            sma_windows: vec![],
            volatility_windows: vec![],
            rsi_enabled: false,
        });

        pct.update(&bar(100.0), None);
        log.update(&bar(100.0), None);
        let pct_obs = pct.update(&bar(110.0), None);
        let log_obs = log.update(&bar(110.0), None);

        assert!((pct_obs.values[0] - 0.1).abs() < 1e-12);
        assert!((log_obs.values[0] - (1.1f64).ln()).abs() < 1e-12);
    }

    #[test]
    fn rsi_is_bounded_and_finite() {
        let mut builder = FeatureBuilder::new(FeatureConfig {
            return_mode: ReturnMode::Pct,
            sma_windows: vec![],
            volatility_windows: vec![],
            rsi_enabled: true,
        });

        let mut last = None;
        for i in 0..20 {
            let price = 100.0 + i as f64;
            let obs = builder.update(&bar(price), None);
            last = Some(obs);
        }
        let last = last.expect("last obs");
        let rsi = *last.values.last().expect("rsi value");
        assert!(rsi.is_finite());
        assert!((0.0..=100.0).contains(&rsi));
    }
}
