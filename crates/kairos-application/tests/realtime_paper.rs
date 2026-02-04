use kairos_application::config::Config;
use kairos_domain::repositories::artifacts::ArtifactWriter;
use kairos_domain::repositories::market_stream::{MarketEvent, MarketStream, StreamError};
use kairos_domain::repositories::sentiment::{SentimentQuery, SentimentRepository};
use kairos_domain::services::engine::backtest::RunControl;
use kairos_domain::services::sentiment::{MissingValuePolicy, SentimentPoint, SentimentReport};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

struct FakeStream {
    events: Vec<MarketEvent>,
    idx: usize,
}

impl FakeStream {
    fn new(events: Vec<MarketEvent>) -> Self {
        Self { events, idx: 0 }
    }
}

impl MarketStream for FakeStream {
    fn next_event(&mut self) -> Result<MarketEvent, StreamError> {
        if self.idx >= self.events.len() {
            return Err(StreamError::Disconnected("eof".to_string()));
        }
        let ev = self.events[self.idx].clone();
        self.idx += 1;
        Ok(ev)
    }
}

#[derive(Default)]
struct NoopArtifacts {
    calls: AtomicU64,
}

impl ArtifactWriter for NoopArtifacts {
    fn ensure_dir(&self, _path: &Path) -> Result<(), String> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
    fn write_trades_csv(
        &self,
        _path: &Path,
        _trades: &[kairos_domain::value_objects::trade::Trade],
    ) -> Result<(), String> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
    fn write_equity_csv(
        &self,
        _path: &Path,
        _points: &[kairos_domain::value_objects::equity_point::EquityPoint],
    ) -> Result<(), String> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
    fn write_summary_json(
        &self,
        _path: &Path,
        _summary: &kairos_domain::entities::metrics::MetricsSummary,
        _meta: Option<&serde_json::Value>,
        _config_snapshot: Option<&serde_json::Value>,
    ) -> Result<(), String> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
    fn write_analyzer_json(&self, _path: &Path, _value: &serde_json::Value) -> Result<(), String> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
    fn write_summary_html(
        &self,
        _path: &Path,
        _summary: &kairos_domain::entities::metrics::MetricsSummary,
        _meta: Option<&serde_json::Value>,
    ) -> Result<(), String> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
    fn write_dashboard_html(
        &self,
        _path: &Path,
        _summary: &kairos_domain::entities::metrics::MetricsSummary,
        _meta: Option<&serde_json::Value>,
        _trades: &[kairos_domain::value_objects::trade::Trade],
        _equity: &[kairos_domain::value_objects::equity_point::EquityPoint],
    ) -> Result<(), String> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
    fn write_audit_jsonl(
        &self,
        _path: &Path,
        _events: &[kairos_domain::services::audit::AuditEvent],
    ) -> Result<(), String> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
    fn write_config_snapshot_toml(&self, _path: &Path, _contents: &str) -> Result<(), String> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}

struct FakeSentimentRepo;

impl SentimentRepository for FakeSentimentRepo {
    fn load_sentiment(
        &self,
        _query: &SentimentQuery,
    ) -> Result<(Vec<SentimentPoint>, SentimentReport), String> {
        Ok((Vec::new(), SentimentReport::default()))
    }
}

#[derive(Clone)]
struct CancelAfter {
    cancel: Arc<AtomicBool>,
}

impl RunControl for CancelAfter {
    fn should_cancel(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    fn wait_if_paused(&self) -> bool {
        !self.should_cancel()
    }
}

#[test]
fn paper_realtime_can_cancel_without_writing_artifacts() {
    let toml_str = r#"
[run]
run_id = "rt_test"
symbol = "BTC-USDT"
timeframe = "60s"
initial_capital = 1000.0

[db]
ohlcv_table = "ohlcv_candles"
exchange = "kucoin"
market = "spot"

[paths]
out_dir = "runs/"

[costs]
fee_bps = 0.0
slippage_bps = 0.0

[risk]
max_position_qty = 1.0
max_drawdown_pct = 0.50
max_exposure_pct = 1.0

[orders]
size_mode = "qty"

[execution]
model = "simple"
latency_bars = 0
buy_kind = "market"
sell_kind = "market"
price_reference = "close"
limit_offset_bps = 0.0
stop_offset_bps = 0.0
spread_bps = 0.0
max_fill_pct_of_volume = 1.0
tif = "gtc"

[features]
return_mode = "log"
sma_windows = [10]
volatility_windows = [10]
rsi_enabled = false
sentiment_lag = "0s"
sentiment_missing = "error"

[strategy]
baseline = "buy_and_hold"

[metrics]
risk_free_rate = 0.0
annualization_factor = 365.0

[agent]
mode = "baseline"
url = "http://127.0.0.1:8000"
timeout_ms = 200
retries = 0
fallback_action = "HOLD"
api_version = "v1"
feature_version = "v1"
"#;

    let config: Config = toml::from_str(toml_str).expect("config parses");
    let artifacts = NoopArtifacts::default();
    let sentiment = FakeSentimentRepo;

    let events = vec![
        MarketEvent::Tick {
            timestamp: 0,
            price: 10.0,
        },
        MarketEvent::Tick {
            timestamp: 10,
            price: 11.0,
        },
        MarketEvent::Tick {
            timestamp: 70,
            price: 12.0,
        },
        MarketEvent::Tick {
            timestamp: 80,
            price: 13.0,
        },
        MarketEvent::Tick {
            timestamp: 130,
            price: 14.0,
        },
    ];

    let cancel = Arc::new(AtomicBool::new(false));
    let control = CancelAfter {
        cancel: cancel.clone(),
    };

    let mut connect_calls = 0u32;
    let mut connect_stream = || {
        connect_calls += 1;
        Ok(Box::new(FakeStream::new(events.clone())) as Box<dyn MarketStream>)
    };

    let mut bars_seen = 0u64;
    let mut progress = |p: kairos_domain::services::engine::backtest::BarProgress| {
        bars_seen += 1;
        if bars_seen >= 2 {
            cancel.store(true, Ordering::Relaxed);
        }
        // Ensure progress is sane.
        assert!(p.close.is_finite());
        assert!(p.equity.is_finite());
    };

    let mut status_calls = 0u64;
    let mut on_status = |_s: kairos_application::paper_trading::RealtimeStreamStatus| {
        status_calls += 1;
    };

    let err = kairos_application::paper_trading::run_paper_realtime_streaming_control(
        &config,
        toml_str,
        None,
        &mut connect_stream,
        &sentiment,
        &artifacts,
        None,
        &control,
        &mut progress,
        &mut on_status,
    )
    .expect_err("cancel should return error");

    assert!(
        err.to_lowercase().contains("cancel"),
        "unexpected error: {err}"
    );
    assert!(bars_seen >= 2);
    assert!(status_calls > 0);
    assert_eq!(connect_calls, 1);
    assert_eq!(artifacts.calls.load(Ordering::Relaxed), 0);

    // Keep compiler from warning about unused policy in this file on some configurations.
    let _ = MissingValuePolicy::Error;
}
