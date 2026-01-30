use kairos_application::config::{AgentMode, Config};
use kairos_domain::repositories::artifacts::{ArtifactReader, ArtifactWriter};
use kairos_domain::repositories::market_data::{MarketDataRepository, OhlcvQuery};
use kairos_domain::repositories::sentiment::{SentimentQuery, SentimentRepository};
use kairos_domain::services::audit::AuditEvent;
use kairos_domain::services::ohlcv::DataQualityReport;
use kairos_domain::services::sentiment::{SentimentPoint, SentimentReport};
use kairos_domain::value_objects::bar::Bar;
use kairos_domain::value_objects::equity_point::EquityPoint;
use kairos_domain::value_objects::side::Side;
use kairos_domain::value_objects::trade::Trade;
use std::cell::RefCell;
use std::path::{Path, PathBuf};

#[derive(Default)]
struct FakeMarketDataRepo {
    bars: Vec<Bar>,
    report: DataQualityReport,
}

impl MarketDataRepository for FakeMarketDataRepo {
    fn load_ohlcv(&self, _query: &OhlcvQuery) -> Result<(Vec<Bar>, DataQualityReport), String> {
        Ok((self.bars.clone(), self.report.clone()))
    }
}

#[derive(Default)]
struct FakeSentimentRepo;

impl SentimentRepository for FakeSentimentRepo {
    fn load_sentiment(
        &self,
        _query: &SentimentQuery,
    ) -> Result<(Vec<SentimentPoint>, SentimentReport), String> {
        Ok((Vec::new(), SentimentReport::default()))
    }
}

#[derive(Default)]
struct RecordingWriter {
    ensured_dirs: RefCell<Vec<PathBuf>>,
    trades_written: RefCell<Option<usize>>,
    equity_written: RefCell<Option<usize>>,
    summary_written: RefCell<Option<serde_json::Value>>,
    summary_html_written: RefCell<bool>,
    audit_written: RefCell<Option<usize>>,
    config_snapshot: RefCell<Option<String>>,
}

impl ArtifactWriter for RecordingWriter {
    fn ensure_dir(&self, path: &Path) -> Result<(), String> {
        self.ensured_dirs.borrow_mut().push(path.to_path_buf());
        Ok(())
    }

    fn write_trades_csv(&self, _path: &Path, trades: &[Trade]) -> Result<(), String> {
        *self.trades_written.borrow_mut() = Some(trades.len());
        Ok(())
    }

    fn write_equity_csv(&self, _path: &Path, points: &[EquityPoint]) -> Result<(), String> {
        *self.equity_written.borrow_mut() = Some(points.len());
        Ok(())
    }

    fn write_summary_json(
        &self,
        _path: &Path,
        summary: &kairos_domain::entities::metrics::MetricsSummary,
        meta: Option<&serde_json::Value>,
        config_snapshot: Option<&serde_json::Value>,
    ) -> Result<(), String> {
        let json = serde_json::json!({
            "summary": {
                "bars_processed": summary.bars_processed,
                "trades": summary.trades,
                "win_rate": summary.win_rate,
                "net_profit": summary.net_profit,
                "sharpe": summary.sharpe,
                "max_drawdown": summary.max_drawdown,
            },
            "meta": meta,
            "config_snapshot": config_snapshot,
        });
        *self.summary_written.borrow_mut() = Some(json);
        Ok(())
    }

    fn write_summary_html(
        &self,
        _path: &Path,
        _summary: &kairos_domain::entities::metrics::MetricsSummary,
        _meta: Option<&serde_json::Value>,
    ) -> Result<(), String> {
        *self.summary_html_written.borrow_mut() = true;
        Ok(())
    }

    fn write_audit_jsonl(&self, _path: &Path, events: &[AuditEvent]) -> Result<(), String> {
        *self.audit_written.borrow_mut() = Some(events.len());
        Ok(())
    }

    fn write_config_snapshot_toml(&self, _path: &Path, contents: &str) -> Result<(), String> {
        *self.config_snapshot.borrow_mut() = Some(contents.to_string());
        Ok(())
    }
}

#[derive(Default)]
struct FakeReader {
    trades: Vec<Trade>,
    equity: Vec<EquityPoint>,
    config_toml: Option<String>,
}

impl ArtifactReader for FakeReader {
    fn read_trades_csv(&self, _path: &Path) -> Result<Vec<Trade>, String> {
        Ok(self.trades.clone())
    }

    fn read_equity_csv(&self, _path: &Path) -> Result<Vec<EquityPoint>, String> {
        Ok(self.equity.clone())
    }

    fn read_config_snapshot_toml(&self, _path: &Path) -> Result<Option<String>, String> {
        Ok(self.config_toml.clone())
    }

    fn exists(&self, _path: &Path) -> bool {
        true
    }
}

fn minimal_config() -> Config {
    Config {
        run: kairos_application::config::RunConfig {
            run_id: "test_run".to_string(),
            symbol: "BTCUSD".to_string(),
            timeframe: "1m".to_string(),
            initial_capital: 1000.0,
        },
        db: kairos_application::config::DbConfig {
            url: None,
            ohlcv_table: "ohlcv_candles".to_string(),
            exchange: "kucoin".to_string(),
            market: "spot".to_string(),
            source_timeframe: None,
        },
        paths: kairos_application::config::PathsConfig {
            sentiment_path: None,
            out_dir: "runs/".to_string(),
        },
        costs: kairos_application::config::CostsConfig {
            fee_bps: 0.0,
            slippage_bps: 0.0,
        },
        risk: kairos_application::config::RiskConfig {
            max_position_qty: 1.0,
            max_drawdown_pct: 1.0,
            max_exposure_pct: 1.0,
        },
        orders: Some(kairos_application::config::OrdersConfig {
            size_mode: Some("qty".to_string()),
        }),
        execution: None,
        features: kairos_application::config::FeaturesConfig {
            return_mode: kairos_domain::services::features::ReturnMode::Pct,
            sma_windows: vec![2, 3],
            volatility_windows: None,
            rsi_enabled: false,
            sentiment_lag: "0s".to_string(),
            sentiment_missing: Some("error".to_string()),
        },
        agent: kairos_application::config::AgentConfig {
            mode: AgentMode::Baseline,
            url: "http://127.0.0.1:8000".to_string(),
            timeout_ms: 200,
            retries: 0,
            fallback_action: kairos_domain::value_objects::action_type::ActionType::Hold,
            api_version: "v1".to_string(),
            feature_version: "v1".to_string(),
        },
        strategy: Some(kairos_application::config::StrategyConfig {
            baseline: "buy_and_hold".to_string(),
            sma_short: None,
            sma_long: None,
        }),
        metrics: None,
        data_quality: Some(kairos_application::config::DataQualityConfig {
            max_gaps: Some(0),
            max_duplicates: Some(0),
            max_out_of_order: Some(0),
            max_invalid_close: Some(0),
            max_sentiment_missing: Some(0),
            max_sentiment_invalid: Some(0),
            max_sentiment_dropped: Some(0),
        }),
        paper: Some(kairos_application::config::PaperConfig {
            replay_scale: Some(0),
        }),
        report: Some(kairos_application::config::ReportConfig { html: Some(false) }),
    }
}

#[test]
fn run_backtest_writes_summary_and_snapshot() {
    let mut config = minimal_config();
    config.report = Some(kairos_application::config::ReportConfig { html: Some(false) });

    let bars = vec![
        Bar {
            symbol: "BTCUSD".to_string(),
            timestamp: 1,
            open: 10.0,
            high: 10.0,
            low: 10.0,
            close: 10.0,
            volume: 10.0,
        },
        Bar {
            symbol: "BTCUSD".to_string(),
            timestamp: 2,
            open: 10.0,
            high: 10.0,
            low: 10.0,
            close: 10.0,
            volume: 10.0,
        },
        Bar {
            symbol: "BTCUSD".to_string(),
            timestamp: 3,
            open: 10.0,
            high: 10.0,
            low: 10.0,
            close: 10.0,
            volume: 10.0,
        },
    ];
    let market = FakeMarketDataRepo {
        bars,
        report: DataQualityReport::default(),
    };
    let sentiment = FakeSentimentRepo;
    let writer = RecordingWriter::default();

    let config_toml = "[run]\nrun_id=\"test_run\"\n";
    let out_dir = std::env::temp_dir().join("kairos_app_tests");
    let run_dir = kairos_application::backtesting::run_backtest(
        &config,
        config_toml,
        Some(out_dir.clone()),
        &market,
        &sentiment,
        &writer,
        None,
    )
    .expect("run_backtest");

    assert!(run_dir.ends_with("test_run"));
    assert_eq!(
        writer.config_snapshot.borrow().as_deref(),
        Some(config_toml)
    );
    let summary_json = writer.summary_written.borrow();
    let json = summary_json.as_ref().expect("summary json written");
    assert_eq!(json["summary"]["bars_processed"], 3);
    assert_eq!(json["meta"]["run_id"], "test_run");
}

#[test]
fn run_backtest_rejects_negative_slippage() {
    let mut config = minimal_config();
    config.costs.slippage_bps = -1.0;

    let market = FakeMarketDataRepo {
        bars: vec![Bar {
            symbol: "BTCUSD".to_string(),
            timestamp: 1,
            open: 10.0,
            high: 10.0,
            low: 10.0,
            close: 10.0,
            volume: 10.0,
        }],
        report: DataQualityReport::default(),
    };
    let sentiment = FakeSentimentRepo;
    let writer = RecordingWriter::default();

    let err = kairos_application::backtesting::run_backtest(
        &config,
        "",
        Some(std::env::temp_dir()),
        &market,
        &sentiment,
        &writer,
        None,
    )
    .expect_err("should fail");
    assert!(err.contains("slippage_bps"));
}

#[test]
fn validate_strict_fails_when_limits_exceeded() {
    let config = minimal_config();
    let market = FakeMarketDataRepo {
        bars: Vec::new(),
        report: DataQualityReport {
            gaps: 1,
            ..DataQualityReport::default()
        },
    };
    let sentiment = FakeSentimentRepo;

    let err = kairos_application::validation::validate(&config, true, &market, &sentiment)
        .expect_err("strict should fail");
    assert!(err.contains("strict validation failed"));
}

#[test]
fn generate_report_writes_html_when_enabled() {
    let trades = vec![Trade {
        timestamp: 1,
        symbol: "BTCUSD".to_string(),
        side: Side::Buy,
        quantity: 1.0,
        price: 100.0,
        fee: 0.0,
        slippage: 0.0,
        strategy_id: "s".to_string(),
        reason: "unit".to_string(),
    }];
    let equity = vec![
        EquityPoint {
            timestamp: 1,
            equity: 100.0,
            cash: 100.0,
            position_qty: 0.0,
            unrealized_pnl: 0.0,
            realized_pnl: 0.0,
        },
        EquityPoint {
            timestamp: 2,
            equity: 110.0,
            cash: 110.0,
            position_qty: 0.0,
            unrealized_pnl: 0.0,
            realized_pnl: 0.0,
        },
    ];

    let config_toml = r#"
[run]
run_id = "rep1"
symbol = "BTCUSD"
timeframe = "1m"
initial_capital = 1000.0

[db]
url = "postgres://x"
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
max_drawdown_pct = 1.0
max_exposure_pct = 1.0

[features]
return_mode = "pct"
sma_windows = [2,3]
volatility_windows = [2]
rsi_enabled = false
sentiment_lag = "0s"

[agent]
mode = "baseline"
url = "http://127.0.0.1:8000"
timeout_ms = 200
retries = 0
fallback_action = "HOLD"
api_version = "v1"
feature_version = "v1"

[report]
html = true
"#;

    let reader = FakeReader {
        trades,
        equity,
        config_toml: Some(config_toml.trim().to_string()),
    };
    let writer = RecordingWriter::default();

    let tmp_dir = std::env::temp_dir().join("kairos_report_test");
    let result =
        kairos_application::reporting::generate_report(tmp_dir.as_path(), &reader, &writer)
            .expect("generate report");

    assert_eq!(result.run_id, "rep1");
    assert!(result.wrote_html);
    assert!(*writer.summary_html_written.borrow());
    assert!(writer.audit_written.borrow().unwrap_or(0) >= 2);
}

#[test]
fn run_paper_writes_summary_and_snapshot_without_sleep() {
    let mut config = minimal_config();
    config.paper = Some(kairos_application::config::PaperConfig {
        replay_scale: Some(0),
    });
    config.agent.mode = AgentMode::Baseline;
    config.report = Some(kairos_application::config::ReportConfig { html: Some(false) });

    let bars = vec![
        Bar {
            symbol: "BTCUSD".to_string(),
            timestamp: 1,
            open: 10.0,
            high: 10.0,
            low: 10.0,
            close: 10.0,
            volume: 10.0,
        },
        Bar {
            symbol: "BTCUSD".to_string(),
            timestamp: 2,
            open: 10.0,
            high: 10.0,
            low: 10.0,
            close: 10.0,
            volume: 10.0,
        },
        Bar {
            symbol: "BTCUSD".to_string(),
            timestamp: 3,
            open: 10.0,
            high: 10.0,
            low: 10.0,
            close: 10.0,
            volume: 10.0,
        },
    ];

    let market = FakeMarketDataRepo {
        bars,
        report: DataQualityReport::default(),
    };
    let sentiment = FakeSentimentRepo;
    let writer = RecordingWriter::default();

    let config_toml = "[run]\nrun_id=\"test_run\"\n";
    let out_dir = std::env::temp_dir().join("kairos_app_paper_tests");
    let run_dir = kairos_application::paper_trading::run_paper(
        &config,
        config_toml,
        Some(out_dir),
        &market,
        &sentiment,
        &writer,
        None,
    )
    .expect("run_paper");

    assert!(run_dir.ends_with("test_run"));
    assert_eq!(
        writer.config_snapshot.borrow().as_deref(),
        Some(config_toml)
    );
    let summary_json = writer.summary_written.borrow();
    let json = summary_json.as_ref().expect("summary json written");
    assert_eq!(json["summary"]["bars_processed"], 3);
    assert_eq!(json["meta"]["run_id"], "test_run");
}
