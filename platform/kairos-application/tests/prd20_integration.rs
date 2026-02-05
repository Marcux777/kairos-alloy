use kairos_infrastructure::artifacts::FilesystemArtifactWriter;
use kairos_infrastructure::persistence::postgres_ohlcv::PostgresMarketDataRepository;
use kairos_infrastructure::sentiment::FilesystemSentimentRepository;
use kairos_ingest::{ingest_kucoin, migrate_db, Market};
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn should_run_db_tests() -> bool {
    std::env::var("KAIROS_DB_RUN_TESTS").ok().as_deref() == Some("1")
}

fn db_url() -> Option<String> {
    std::env::var("KAIROS_DB_URL").ok()
}

fn unique_suffix() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("{}_{}", std::process::id(), now)
}

struct MockKucoinServer {
    base_url: String,
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl MockKucoinServer {
    fn start_spot(candles_json: String) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let addr = listener.local_addr().expect("local addr");
        let base_url = format!("http://{}", addr);
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = stop.clone();

        let handle = thread::spawn(move || {
            listener.set_nonblocking(true).expect("nonblocking");
            while !stop_clone.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = handle_connection(&mut stream, &candles_json);
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => {
                        thread::sleep(Duration::from_millis(10));
                    }
                }
            }
        });

        Self {
            base_url,
            stop,
            handle: Some(handle),
        }
    }
}

impl Drop for MockKucoinServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn handle_connection(stream: &mut TcpStream, candles_json: &str) -> Result<(), String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;

    let mut buf = Vec::new();
    let mut tmp = [0u8; 1024];
    loop {
        let n = stream.read(&mut tmp).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
        if buf.len() > 8192 {
            break;
        }
    }

    let body = candles_json.as_bytes();
    let header = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(header.as_bytes())
        .map_err(|e| e.to_string())?;
    stream.write_all(body).map_err(|e| e.to_string())?;
    Ok(())
}

fn kucoin_spot_payload() -> String {
    // Row format expected by `kairos-ingest`:
    // [timestamp, open, close, high, low, volume, turnover?]
    //
    // Timestamps are epoch seconds.
    r#"
{
  "code": "200000",
  "data": [
    ["1704067200","100","101","102","99","10","0"],
    ["1704067260","101","102","103","100","10","0"],
    ["1704067320","102","103","104","101","10","0"],
    ["1704067380","103","104","105","102","10","0"],
    ["1704067440","104","105","106","103","10","0"],
    ["1704067500","105","106","107","104","10","0"]
  ]
}
"#
    .trim()
    .to_string()
}

#[allow(clippy::too_many_arguments)]
fn write_config(
    dir: &Path,
    run_id: &str,
    symbol: &str,
    timeframe: &str,
    db_url: &str,
    exchange: &str,
    market: &str,
    sentiment_path: Option<&Path>,
    paper_replay_scale: Option<u64>,
) -> PathBuf {
    let mut toml = String::new();
    toml.push_str("[run]\n");
    toml.push_str(&format!("run_id = \"{}\"\n", run_id));
    toml.push_str(&format!("symbol = \"{}\"\n", symbol));
    toml.push_str(&format!("timeframe = \"{}\"\n", timeframe));
    toml.push_str("initial_capital = 10000.0\n\n");

    toml.push_str("[db]\n");
    toml.push_str(&format!("url = \"{}\"\n", db_url));
    toml.push_str("ohlcv_table = \"ohlcv_candles\"\n");
    toml.push_str(&format!("exchange = \"{}\"\n", exchange));
    toml.push_str(&format!("market = \"{}\"\n\n", market));

    toml.push_str("[paths]\n");
    toml.push_str(&format!("out_dir = \"{}\"\n", dir.display()));
    if let Some(sentiment) = sentiment_path {
        toml.push_str(&format!("sentiment_path = \"{}\"\n", sentiment.display()));
    }
    toml.push('\n');

    toml.push_str("[costs]\nfee_bps = 10.0\nslippage_bps = 5.0\n\n");
    toml.push_str(
        "[risk]\nmax_position_qty = 1.0\nmax_drawdown_pct = 0.90\nmax_exposure_pct = 1.00\n\n",
    );
    toml.push_str("[orders]\nsize_mode = \"qty\"\n\n");

    toml.push_str("[features]\n");
    toml.push_str("return_mode = \"log\"\n");
    toml.push_str("sma_windows = [2, 3]\n");
    toml.push_str("volatility_windows = [2]\n");
    toml.push_str("rsi_enabled = false\n");
    toml.push_str("sentiment_lag = \"0s\"\n");
    toml.push_str("sentiment_missing = \"error\"\n\n");

    toml.push_str("[strategy]\nbaseline = \"buy_and_hold\"\n\n");

    toml.push_str("[metrics]\nrisk_free_rate = 0.0\nannualization_factor = 365.0\n\n");

    toml.push_str("[agent]\n");
    toml.push_str("mode = \"baseline\"\n");
    toml.push_str("url = \"http://127.0.0.1:8000\"\n");
    toml.push_str("timeout_ms = 200\n");
    toml.push_str("retries = 0\n");
    toml.push_str("fallback_action = \"HOLD\"\n");
    toml.push_str("api_version = \"v1\"\n");
    toml.push_str("feature_version = \"v1\"\n\n");

    if let Some(scale) = paper_replay_scale {
        toml.push_str("[paper]\n");
        toml.push_str(&format!("replay_scale = {}\n\n", scale));
    }

    toml.push_str("[report]\nhtml = false\n");

    let config_path = dir.join(format!("{run_id}.toml"));
    fs::write(&config_path, toml).expect("write config");
    config_path
}

fn write_sentiment_csv(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    let contents = "timestamp_utc,score,volume_mencoes\n\
2024-01-01T00:00:00Z,0.1,10\n\
2024-01-01T00:01:00Z,0.2,11\n\
2024-01-01T00:02:00Z,0.3,12\n";
    fs::write(&path, contents).expect("write sentiment csv");
    path
}

fn write_sentiment_json(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    let contents = r#"
[
  {"timestamp_utc":"2024-01-01T00:00:00Z","score":0.1,"volume_mencoes":10},
  {"timestamp_utc":"2024-01-01T00:01:00Z","score":0.2,"volume_mencoes":11},
  {"timestamp_utc":"2024-01-01T00:02:00Z","score":0.3,"volume_mencoes":12}
]
"#;
    fs::write(&path, contents.trim()).expect("write sentiment json");
    path
}

fn build_market_repo(db_url: &str) -> PostgresMarketDataRepository {
    PostgresMarketDataRepository::new(db_url.to_string(), "ohlcv_candles".to_string(), 1)
        .expect("market repo")
}

#[test]
fn prd20_e2e_ingest_then_backtest_csv_sentiment() {
    if !should_run_db_tests() {
        return;
    }
    let db_url = match db_url() {
        Some(v) => v,
        None => return,
    };

    let suffix = unique_suffix();
    let exchange = format!("kucoin_e2e_{suffix}");
    let symbol = format!("TEST-{suffix}");
    let run_id = format!("e2e_backtest_{suffix}");

    let migrations_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../ops/migrations/0001_create_ohlcv_candles.sql");
    let payload = kucoin_spot_payload();
    let server = MockKucoinServer::start_spot(payload);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    rt.block_on(async {
        migrate_db(&db_url, migrations_path.as_path())
            .await
            .expect("migrate");

        ingest_kucoin(
            &db_url,
            &symbol,
            Market::Spot,
            "1min",
            "2024-01-01T00:00:00Z",
            Some("2024-01-01T00:05:00Z"),
            &exchange,
            "mock",
            0,
            100,
            Some(&server.base_url),
        )
        .await
        .expect("ingest");
    });
    drop(rt);

    let tmp_dir = std::env::temp_dir().join(format!("kairos_prd20_{suffix}"));
    let _ = fs::create_dir_all(&tmp_dir);
    let sentiment_path = write_sentiment_csv(&tmp_dir, "sentiment.csv");
    let config_path = write_config(
        &tmp_dir,
        &run_id,
        &symbol,
        "1min",
        &db_url,
        &exchange,
        "spot",
        Some(sentiment_path.as_path()),
        None,
    );

    let (config, config_toml) =
        kairos_application::config::load_config_with_source(&config_path).expect("config load");
    let market = build_market_repo(&db_url);
    let sentiment = FilesystemSentimentRepository;
    let artifacts = FilesystemArtifactWriter::new();

    let run_dir = kairos_application::backtesting::run_backtest(
        &config,
        &config_toml,
        None,
        &market,
        &sentiment,
        &artifacts,
        None,
    )
    .expect("backtest");

    assert!(run_dir.join("summary.json").exists());
    assert!(run_dir.join("trades.csv").exists());
    assert!(run_dir.join("equity.csv").exists());
    assert!(run_dir.join("config_snapshot.toml").exists());
    assert!(run_dir.join("logs.jsonl").exists());
}

#[test]
fn prd20_smoke_paper_json_sentiment() {
    if !should_run_db_tests() {
        return;
    }
    let db_url = match db_url() {
        Some(v) => v,
        None => return,
    };

    let suffix = unique_suffix();
    let exchange = format!("kucoin_e2e_{suffix}");
    let symbol = format!("TEST-{suffix}");
    let run_id = format!("e2e_paper_{suffix}");

    let migrations_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../ops/migrations/0001_create_ohlcv_candles.sql");
    let payload = kucoin_spot_payload();
    let server = MockKucoinServer::start_spot(payload);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    rt.block_on(async {
        migrate_db(&db_url, migrations_path.as_path())
            .await
            .expect("migrate");

        ingest_kucoin(
            &db_url,
            &symbol,
            Market::Spot,
            "1min",
            "2024-01-01T00:00:00Z",
            Some("2024-01-01T00:05:00Z"),
            &exchange,
            "mock",
            0,
            100,
            Some(&server.base_url),
        )
        .await
        .expect("ingest");
    });
    drop(rt);

    let tmp_dir = std::env::temp_dir().join(format!("kairos_prd20_{suffix}"));
    let _ = fs::create_dir_all(&tmp_dir);
    let sentiment_path = write_sentiment_json(&tmp_dir, "sentiment.json");
    let config_path = write_config(
        &tmp_dir,
        &run_id,
        &symbol,
        "1min",
        &db_url,
        &exchange,
        "spot",
        Some(sentiment_path.as_path()),
        Some(100_000),
    );

    let (config, config_toml) =
        kairos_application::config::load_config_with_source(&config_path).expect("config load");
    let market = build_market_repo(&db_url);
    let sentiment = FilesystemSentimentRepository;
    let artifacts = FilesystemArtifactWriter::new();

    let run_dir = kairos_application::paper_trading::run_paper(
        &config,
        &config_toml,
        None,
        &market,
        &sentiment,
        &artifacts,
        None,
    )
    .expect("paper");

    assert!(run_dir.join("summary.json").exists());
    assert!(run_dir.join("trades.csv").exists());
    assert!(run_dir.join("equity.csv").exists());
    assert!(run_dir.join("config_snapshot.toml").exists());
    assert!(run_dir.join("logs.jsonl").exists());
}
