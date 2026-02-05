use crate::repositories::market_stream::MarketEvent;
use crate::value_objects::bar::Bar;

#[derive(Debug, Default, Clone)]
pub struct BarAggregationReport {
    pub out_of_order_events: u64,
    pub invalid_events: u64,
    pub last_event_timestamp: Option<i64>,
    pub last_bar_timestamp: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct BarAggregator {
    symbol: String,
    step_seconds: i64,
    current_bucket_start: Option<i64>,
    working: Option<Bar>,
    last_event_ts: Option<i64>,
    report: BarAggregationReport,
}

impl BarAggregator {
    pub fn new(symbol: String, step_seconds: i64) -> Result<Self, String> {
        if step_seconds <= 0 {
            return Err("step_seconds must be > 0".to_string());
        }
        Ok(Self {
            symbol,
            step_seconds,
            current_bucket_start: None,
            working: None,
            last_event_ts: None,
            report: BarAggregationReport::default(),
        })
    }

    pub fn report(&self) -> &BarAggregationReport {
        &self.report
    }

    pub fn ingest(&mut self, event: MarketEvent) -> Option<Bar> {
        let (ts_raw, price, qty) = match event {
            MarketEvent::Tick { timestamp, price } => (timestamp, price, 0.0),
            MarketEvent::Trade {
                timestamp,
                price,
                quantity,
            } => (timestamp, price, quantity),
        };

        let ts = normalize_epoch_seconds(ts_raw);
        if !price.is_finite() || price <= 0.0 {
            self.report.invalid_events = self.report.invalid_events.saturating_add(1);
            return None;
        }

        if let Some(prev) = self.last_event_ts {
            if ts < prev {
                self.report.out_of_order_events = self.report.out_of_order_events.saturating_add(1);
                // Determinism: drop out-of-order events instead of rewriting past bars.
                return None;
            }
        }
        self.last_event_ts = Some(ts);
        self.report.last_event_timestamp = Some(ts);

        let bucket_start = ts.saturating_sub(ts.rem_euclid(self.step_seconds));
        let mut finalized: Option<Bar> = None;

        match self.current_bucket_start {
            None => {
                self.current_bucket_start = Some(bucket_start);
                self.working = Some(Bar {
                    symbol: self.symbol.clone(),
                    timestamp: bucket_start,
                    open: price,
                    high: price,
                    low: price,
                    close: price,
                    volume: qty.max(0.0),
                });
            }
            Some(active) if active == bucket_start => {
                if let Some(ref mut bar) = self.working {
                    bar.high = bar.high.max(price);
                    bar.low = bar.low.min(price);
                    bar.close = price;
                    if qty.is_finite() && qty > 0.0 {
                        bar.volume += qty;
                    }
                }
            }
            Some(_) => {
                finalized = self.working.take();
                self.report.last_bar_timestamp = finalized.as_ref().map(|b| b.timestamp);
                self.current_bucket_start = Some(bucket_start);
                self.working = Some(Bar {
                    symbol: self.symbol.clone(),
                    timestamp: bucket_start,
                    open: price,
                    high: price,
                    low: price,
                    close: price,
                    volume: qty.max(0.0),
                });
            }
        }

        finalized
    }

    pub fn flush(&mut self) -> Option<Bar> {
        let finalized = self.working.take();
        self.report.last_bar_timestamp = finalized.as_ref().map(|b| b.timestamp);
        finalized
    }
}

fn normalize_epoch_seconds(ts: i64) -> i64 {
    // Heuristic: treat values larger than year 2286 in seconds as milliseconds.
    // 10^12 is ~2001-09-09 in milliseconds.
    if ts >= 1_000_000_000_000i64 {
        ts / 1000
    } else {
        ts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregates_ticks_into_bars_and_rolls_on_bucket_change() {
        let mut agg = BarAggregator::new("BTC-USDT".to_string(), 60).unwrap();

        assert_eq!(
            agg.ingest(MarketEvent::Tick {
                timestamp: 0,
                price: 10.0
            }),
            None
        );
        assert_eq!(
            agg.ingest(MarketEvent::Tick {
                timestamp: 10,
                price: 11.0
            }),
            None
        );
        let bar0 = agg
            .ingest(MarketEvent::Tick {
                timestamp: 70,
                price: 12.0,
            })
            .expect("finalize first bar");

        assert_eq!(bar0.timestamp, 0);
        assert_eq!(bar0.open, 10.0);
        assert_eq!(bar0.high, 11.0);
        assert_eq!(bar0.low, 10.0);
        assert_eq!(bar0.close, 11.0);
    }

    #[test]
    fn drops_out_of_order_events_and_counts_them() {
        let mut agg = BarAggregator::new("BTC-USDT".to_string(), 60).unwrap();
        agg.ingest(MarketEvent::Tick {
            timestamp: 100,
            price: 10.0,
        });
        let out = agg.ingest(MarketEvent::Tick {
            timestamp: 90,
            price: 9.0,
        });
        assert!(out.is_none());
        assert_eq!(agg.report().out_of_order_events, 1);
    }
}
