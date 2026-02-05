use crate::entities::metrics::MetricsSummary;
use crate::value_objects::equity_point::EquityPoint;
use crate::value_objects::side::Side;
use crate::value_objects::trade::Trade;

pub trait Analyzer {
    fn name(&self) -> &'static str;
    fn analyze(&self, input: &AnalyzerInput) -> Result<serde_json::Value, String>;
}

pub struct AnalyzerInput<'a> {
    pub trades: &'a [Trade],
    pub equity: &'a [EquityPoint],
    pub summary: &'a MetricsSummary,
    pub config_snapshot: Option<&'a serde_json::Value>,
}

pub fn built_in_analyzers() -> Vec<Box<dyn Analyzer>> {
    vec![
        Box::new(TradeStatsAnalyzer),
        Box::new(DrawdownAnalyzer { top_n: 5 }),
    ]
}

#[derive(Debug, Clone, Copy)]
pub struct TradeStatsAnalyzer;

impl Analyzer for TradeStatsAnalyzer {
    fn name(&self) -> &'static str {
        "trade_stats"
    }

    fn analyze(&self, input: &AnalyzerInput) -> Result<serde_json::Value, String> {
        let mut buy_count = 0u64;
        let mut sell_count = 0u64;
        let mut total_fee = 0.0;
        let mut total_slippage = 0.0;

        let mut position_qty = 0.0f64;
        let mut position_cost = 0.0f64; // includes buy fees
        let mut entry_ts: Option<i64> = None;

        let mut realized_pnls: Vec<f64> = Vec::new();
        let mut closed_durations: Vec<i64> = Vec::new();

        for t in input.trades {
            total_fee += t.fee;
            total_slippage += t.slippage;

            match t.side {
                Side::Buy => {
                    buy_count += 1;
                    if position_qty <= 0.0 {
                        entry_ts = Some(t.timestamp);
                    }
                    position_cost += t.quantity * t.price + t.fee;
                    position_qty += t.quantity;
                }
                Side::Sell => {
                    sell_count += 1;
                    if position_qty <= 0.0 {
                        realized_pnls.push(0.0);
                        continue;
                    }

                    let qty = t.quantity.min(position_qty);
                    let cost_per_unit = if position_qty > 0.0 {
                        position_cost / position_qty
                    } else {
                        0.0
                    };

                    let proceeds = qty * t.price - t.fee;
                    let cost = qty * cost_per_unit;
                    let pnl = proceeds - cost;
                    realized_pnls.push(pnl);

                    position_qty -= qty;
                    position_cost -= cost;
                    if position_qty <= 0.0 {
                        position_qty = 0.0;
                        position_cost = 0.0;
                        if let Some(entry) = entry_ts.take() {
                            closed_durations.push(t.timestamp.saturating_sub(entry));
                        }
                    }
                }
            }
        }

        let mut wins = 0u64;
        let mut losses = 0u64;
        let mut sum_win = 0.0;
        let mut sum_loss = 0.0;
        for pnl in &realized_pnls {
            if *pnl > 0.0 {
                wins += 1;
                sum_win += pnl;
            } else if *pnl < 0.0 {
                losses += 1;
                sum_loss += pnl;
            }
        }

        let avg_win = if wins > 0 { sum_win / wins as f64 } else { 0.0 };
        let avg_loss = if losses > 0 {
            sum_loss / losses as f64
        } else {
            0.0
        };
        let payoff = if avg_loss < 0.0 {
            avg_win / avg_loss.abs()
        } else {
            0.0
        };
        let win_rate = if wins + losses > 0 {
            wins as f64 / (wins + losses) as f64
        } else {
            input.summary.win_rate
        };

        let avg_holding_seconds = if closed_durations.is_empty() {
            None
        } else {
            let total: i64 = closed_durations.iter().sum();
            Some(total as f64 / closed_durations.len() as f64)
        };

        Ok(serde_json::json!({
            "name": self.name(),
            "trades": {
                "buy_count": buy_count,
                "sell_count": sell_count,
                "round_trips_closed": closed_durations.len(),
            },
            "pnl": {
                "wins": wins,
                "losses": losses,
                "avg_win": avg_win,
                "avg_loss": avg_loss,
                "payoff": payoff,
                "win_rate": win_rate,
            },
            "costs": {
                "total_fee": total_fee,
                "total_slippage": total_slippage,
            },
            "holding": {
                "avg_holding_seconds": avg_holding_seconds,
            },
            "meta": {
                "bars_processed": input.summary.bars_processed,
                "trades_total": input.summary.trades,
            }
        }))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DrawdownAnalyzer {
    pub top_n: usize,
}

impl Analyzer for DrawdownAnalyzer {
    fn name(&self) -> &'static str {
        "drawdown"
    }

    fn analyze(&self, input: &AnalyzerInput) -> Result<serde_json::Value, String> {
        if input.equity.is_empty() {
            return Ok(serde_json::json!({
                "name": self.name(),
                "max_drawdown_pct": 0.0,
                "top": [],
            }));
        }

        #[derive(Clone)]
        struct Segment {
            peak_ts: i64,
            trough_ts: i64,
            recovery_ts: Option<i64>,
            depth_pct: f64,
            duration_bars: u64,
        }

        let mut segments: Vec<Segment> = Vec::new();

        let mut peak_equity = input.equity[0].equity;
        let mut peak_ts = input.equity[0].timestamp;
        let mut peak_index = 0usize;

        let mut current_trough_equity = input.equity[0].equity;
        let mut current_trough_ts = input.equity[0].timestamp;
        let mut current_trough_index = 0usize;
        let mut in_drawdown = false;

        for (i, p) in input.equity.iter().enumerate().skip(1) {
            let equity = p.equity;
            if equity >= peak_equity {
                if in_drawdown {
                    let depth = if peak_equity > 0.0 {
                        (peak_equity - current_trough_equity) / peak_equity
                    } else {
                        0.0
                    };
                    segments.push(Segment {
                        peak_ts,
                        trough_ts: current_trough_ts,
                        recovery_ts: Some(p.timestamp),
                        depth_pct: depth,
                        duration_bars: (i - peak_index) as u64,
                    });
                }
                peak_equity = equity;
                peak_ts = p.timestamp;
                peak_index = i;
                current_trough_equity = equity;
                current_trough_ts = p.timestamp;
                current_trough_index = i;
                in_drawdown = false;
                continue;
            }

            in_drawdown = true;
            if equity <= current_trough_equity {
                current_trough_equity = equity;
                current_trough_ts = p.timestamp;
                current_trough_index = i;
            }

            let _ = current_trough_index;
        }

        if in_drawdown {
            let last = input.equity.last().expect("non-empty");
            let depth = if peak_equity > 0.0 {
                (peak_equity - current_trough_equity) / peak_equity
            } else {
                0.0
            };
            segments.push(Segment {
                peak_ts,
                trough_ts: current_trough_ts,
                recovery_ts: None,
                depth_pct: depth,
                duration_bars: (input.equity.len().saturating_sub(1) - peak_index) as u64,
            });
            let _ = last;
        }

        segments.sort_by(|a, b| {
            b.depth_pct
                .partial_cmp(&a.depth_pct)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let top = segments
            .into_iter()
            .take(self.top_n)
            .map(|s| {
                serde_json::json!({
                    "peak_ts": s.peak_ts,
                    "trough_ts": s.trough_ts,
                    "recovery_ts": s.recovery_ts,
                    "depth_pct": s.depth_pct,
                    "duration_bars": s.duration_bars,
                })
            })
            .collect::<Vec<_>>();

        Ok(serde_json::json!({
            "name": self.name(),
            "max_drawdown_pct": input.summary.max_drawdown,
            "top": top,
        }))
    }
}
