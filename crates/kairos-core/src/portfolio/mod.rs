use crate::types::{Position, Side};

#[derive(Debug, Default)]
pub struct Portfolio {
    positions: Vec<Position>,
    cash: f64,
    realized_pnl: f64,
}

impl Portfolio {
    pub fn new_with_cash(initial_cash: f64) -> Self {
        Self {
            positions: Vec::new(),
            cash: initial_cash,
            realized_pnl: 0.0,
        }
    }

    pub fn new() -> Self {
        Self::new_with_cash(0.0)
    }

    pub fn positions(&self) -> &[Position] {
        &self.positions
    }

    pub fn cash(&self) -> f64 {
        self.cash
    }

    pub fn realized_pnl(&self) -> f64 {
        self.realized_pnl
    }

    pub fn position_qty(&self, symbol: &str) -> f64 {
        self.positions
            .iter()
            .find(|pos| pos.symbol == symbol)
            .map(|pos| pos.quantity)
            .unwrap_or(0.0)
    }

    pub fn position_avg_price(&self, symbol: &str) -> f64 {
        self.positions
            .iter()
            .find(|pos| pos.symbol == symbol)
            .map(|pos| pos.avg_price)
            .unwrap_or(0.0)
    }

    pub fn apply_fill(&mut self, symbol: &str, side: Side, quantity: f64, price: f64, fee: f64) {
        if quantity <= 0.0 {
            return;
        }

        let position = self.positions.iter_mut().find(|pos| pos.symbol == symbol);

        match side {
            Side::Buy => {
                let cost = quantity * price + fee;
                self.cash -= cost;
                if self.cash < 0.0 && self.cash > -1e-9 {
                    self.cash = 0.0;
                }

                match position {
                    Some(pos) => {
                        let total_qty = pos.quantity + quantity;
                        if total_qty > 0.0 {
                            let weighted_cost = pos.avg_price * pos.quantity + price * quantity;
                            pos.avg_price = weighted_cost / total_qty;
                        }
                        pos.quantity = total_qty;
                    }
                    None => {
                        self.positions.push(Position {
                            symbol: symbol.to_string(),
                            quantity,
                            avg_price: price,
                        });
                    }
                }
            }
            Side::Sell => {
                if let Some(pos) = position {
                    let sell_qty = quantity.min(pos.quantity);
                    let proceeds = sell_qty * price - fee;
                    self.cash += proceeds;
                    self.realized_pnl += (price - pos.avg_price) * sell_qty - fee;
                    pos.quantity -= sell_qty;
                    if pos.quantity <= 0.0 {
                        pos.quantity = 0.0;
                        pos.avg_price = 0.0;
                    }
                }
            }
        }
    }

    pub fn equity(&self, symbol: &str, price: f64) -> f64 {
        self.cash + self.position_qty(symbol) * price
    }

    pub fn unrealized_pnl(&self, symbol: &str, price: f64) -> f64 {
        let qty = self.position_qty(symbol);
        if qty <= 0.0 {
            return 0.0;
        }
        (price - self.position_avg_price(symbol)) * qty
    }
}

#[cfg(test)]
mod tests {
    use super::Portfolio;
    use crate::types::Side;

    #[test]
    fn buy_and_sell_updates_cash_and_position() {
        let mut portfolio = Portfolio::new_with_cash(1000.0);
        portfolio.apply_fill("BTCUSD", Side::Buy, 1.0, 100.0, 1.0);
        assert_eq!(portfolio.position_qty("BTCUSD"), 1.0);
        assert!((portfolio.cash() - 899.0).abs() < 1e-6);

        portfolio.apply_fill("BTCUSD", Side::Sell, 1.0, 110.0, 1.0);
        assert_eq!(portfolio.position_qty("BTCUSD"), 0.0);
        assert!((portfolio.cash() - 1008.0).abs() < 1e-6);
        assert!(portfolio.realized_pnl() > 0.0);
    }
}
