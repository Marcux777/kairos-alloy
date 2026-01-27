use crate::types::Bar;

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

#[derive(Debug, Clone, Copy)]
pub enum ReturnMode {
    Log,
    Pct,
}

pub struct FeatureBuilder {
    config: FeatureConfig,
    prices: Vec<f64>,
}

impl FeatureBuilder {
    pub fn new(config: FeatureConfig) -> Self {
        Self {
            config,
            prices: Vec::new(),
        }
    }

    pub fn update(&mut self, bar: &Bar, sentiment: Option<&[f64]>) -> Observation {
        let prev = self.prices.last().copied();
        self.prices.push(bar.close);

        let mut values = Vec::new();
        let ret = match prev {
            Some(prev_price) if prev_price > 0.0 => match self.config.return_mode {
                ReturnMode::Log => (bar.close / prev_price).ln(),
                ReturnMode::Pct => bar.close / prev_price - 1.0,
            },
            _ => 0.0,
        };
        values.push(ret);

        for window in &self.config.sma_windows {
            let sma = self.sma(*window).unwrap_or(0.0);
            values.push(sma);
        }

        for window in &self.config.volatility_windows {
            let vol = self.volatility(*window).unwrap_or(0.0);
            values.push(vol);
        }

        if self.config.rsi_enabled {
            values.push(self.rsi(14).unwrap_or(0.0));
        }

        if let Some(sentiment_values) = sentiment {
            values.extend_from_slice(sentiment_values);
        }

        Observation { values }
    }

    fn sma(&self, window: usize) -> Option<f64> {
        if window == 0 || self.prices.len() < window {
            return None;
        }
        let slice = &self.prices[self.prices.len() - window..];
        Some(slice.iter().sum::<f64>() / window as f64)
    }

    fn rsi(&self, window: usize) -> Option<f64> {
        if window == 0 || self.prices.len() <= window {
            return None;
        }
        let slice = &self.prices[self.prices.len() - window - 1..];
        let mut gains = 0.0;
        let mut losses = 0.0;
        for pair in slice.windows(2) {
            let diff = pair[1] - pair[0];
            if diff > 0.0 {
                gains += diff;
            } else {
                losses -= diff;
            }
        }
        if gains + losses == 0.0 {
            return Some(50.0);
        }
        let rs = gains / losses.max(1e-9);
        Some(100.0 - (100.0 / (1.0 + rs)))
    }

    fn volatility(&self, window: usize) -> Option<f64> {
        if window == 0 || self.prices.len() <= window {
            return None;
        }
        let slice = &self.prices[self.prices.len() - window - 1..];
        let mut returns = Vec::with_capacity(window);
        for pair in slice.windows(2) {
            let prev = pair[0];
            let curr = pair[1];
            if prev <= 0.0 {
                returns.push(0.0);
                continue;
            }
            let r = match self.config.return_mode {
                ReturnMode::Log => (curr / prev).ln(),
                ReturnMode::Pct => curr / prev - 1.0,
            };
            returns.push(r);
        }
        if returns.is_empty() {
            return None;
        }
        let mean = returns.iter().sum::<f64>() / returns.len() as f64;
        let var = returns
            .iter()
            .map(|ret| {
                let diff = ret - mean;
                diff * diff
            })
            .sum::<f64>()
            / returns.len() as f64;
        Some(var.sqrt())
    }
}

#[cfg(test)]
mod tests {
    use super::{FeatureBuilder, FeatureConfig, ReturnMode};
    use crate::types::Bar;

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
}
