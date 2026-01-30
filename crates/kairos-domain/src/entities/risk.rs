#[derive(Debug, Clone, Copy)]
pub struct RiskLimits {
    pub max_position_qty: f64,
    pub max_drawdown_pct: f64,
    pub max_exposure_pct: f64,
}

impl Default for RiskLimits {
    fn default() -> Self {
        Self {
            max_position_qty: 0.0,
            max_drawdown_pct: 1.0,
            max_exposure_pct: 1.0,
        }
    }
}

impl RiskLimits {
    pub fn allows_position(&self, current_qty: f64, add_qty: f64) -> bool {
        if self.max_position_qty <= 0.0 {
            return true;
        }
        current_qty + add_qty <= self.max_position_qty
    }

    pub fn allows_exposure(&self, equity: f64, next_exposure: f64) -> bool {
        if self.max_exposure_pct <= 0.0 {
            return true;
        }
        if equity <= 0.0 {
            return false;
        }
        next_exposure / equity <= self.max_exposure_pct
    }

    pub fn allows_drawdown(&self, drawdown_pct: f64) -> bool {
        if self.max_drawdown_pct <= 0.0 {
            return true;
        }
        drawdown_pct <= self.max_drawdown_pct
    }
}

#[cfg(test)]
mod tests {
    use super::RiskLimits;

    #[test]
    fn allows_position_respects_limit() {
        let limits = RiskLimits {
            max_position_qty: 2.0,
            max_drawdown_pct: 1.0,
            max_exposure_pct: 1.0,
        };
        assert!(limits.allows_position(1.0, 1.0));
        assert!(!limits.allows_position(1.5, 1.0));
    }

    #[test]
    fn allows_exposure_respects_limit() {
        let limits = RiskLimits {
            max_position_qty: 0.0,
            max_drawdown_pct: 1.0,
            max_exposure_pct: 0.5,
        };
        assert!(limits.allows_exposure(100.0, 40.0));
        assert!(!limits.allows_exposure(100.0, 60.0));
    }
}
