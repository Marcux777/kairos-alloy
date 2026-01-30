use crate::entities::portfolio::Portfolio;
use crate::entities::risk::RiskLimits;

#[derive(Debug)]
pub struct TradingAccount {
    pub portfolio: Portfolio,
    pub risk_limits: RiskLimits,
}

impl TradingAccount {
    pub fn new(portfolio: Portfolio, risk_limits: RiskLimits) -> Self {
        Self {
            portfolio,
            risk_limits,
        }
    }
}

