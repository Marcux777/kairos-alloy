use chrono::{DateTime, Utc};
use kairos_application::config::{self, Config};
use kairos_ingest::{ingest_kucoin, migrate_db, Market};
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use tokio_postgres::NoTls;

const DEFAULT_TUI_CONFIG_PATH: &str = "platform/ops/configs/sample.toml";
const DEFAULT_MIGRATIONS_PATH: &str = "platform/ops/migrations";
const BOOTSTRAP_TIMEFRAME: &str = "1min";
const BOOTSTRAP_START: &str = "2017-01-01T00:00:00Z";
const BOOTSTRAP_END: &str = "2025-12-31T23:59:59Z";
const DEFAULT_INGEST_SLEEP_MS: u64 = 350;
const DEFAULT_INGEST_BATCH_SIZE: usize = 500;

#[derive(Debug, Clone)]
struct CoverageStats {
    min_ts: Option<DateTime<Utc>>,
    max_ts: Option<DateTime<Utc>>,
}

pub fn prepare_tui_startup(initial_config_path: Option<PathBuf>) -> Result<PathBuf, String> {
    let config_path = resolve_bootstrap_config_path(initial_config_path)?;
    let (config, _config_toml) = config::load_config_with_source(config_path.as_path())?;

    ensure_supported_exchange(&config)?;
    ensure_supported_market(&config.db.market)?;

    let db_url = resolve_db_url(&config)?;
    let workspace_root = discover_workspace_root(config_path.as_path())?;
    let migrations_path = workspace_root.join(DEFAULT_MIGRATIONS_PATH);

    println!(
        "[bootstrap] preparando ambiente (config={}, symbol={}, timeframe={} via source 1min, janela {}..{})",
        config_path.display(),
        config.run.symbol,
        config.run.timeframe,
        BOOTSTRAP_START,
        BOOTSTRAP_END
    );

    start_db_service(workspace_root.as_path())?;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|err| format!("bootstrap failed: unable to start async runtime: {err}"))?;
    runtime.block_on(run_bootstrap_async(
        &config,
        &db_url,
        migrations_path.as_path(),
    ))?;

    println!("[bootstrap] ambiente pronto");
    Ok(config_path)
}

async fn run_bootstrap_async(
    config: &Config,
    db_url: &str,
    migrations_path: &Path,
) -> Result<(), String> {
    migrate_db(db_url, migrations_path).await.map_err(|err| {
        format!(
            "bootstrap failed while running migrations ({}): {err}",
            migrations_path.display()
        )
    })?;

    ensure_bootstrap_window(config, db_url).await?;
    Ok(())
}

async fn ensure_bootstrap_window(config: &Config, db_url: &str) -> Result<(), String> {
    validate_table_name(config.db.ohlcv_table.as_str())?;
    let market = parse_market(config.db.market.as_str())?;
    let start = parse_utc_timestamp(BOOTSTRAP_START)?;
    let end = parse_utc_timestamp(BOOTSTRAP_END)?;

    let coverage = query_coverage(
        db_url,
        config.db.ohlcv_table.as_str(),
        config.db.exchange.as_str(),
        config.db.market.as_str(),
        config.run.symbol.as_str(),
        start,
        end,
    )
    .await?;

    if coverage_satisfies_window(&coverage, start, end) {
        println!(
            "[bootstrap] OHLCV 1min ja cobre {}..{} para {}",
            BOOTSTRAP_START, BOOTSTRAP_END, config.run.symbol
        );
        return Ok(());
    }

    println!(
        "[bootstrap] OHLCV 1min incompleto para {} (janela {}..{}), iniciando ingestao",
        config.run.symbol, BOOTSTRAP_START, BOOTSTRAP_END
    );

    ingest_kucoin(
        db_url,
        config.run.symbol.as_str(),
        market,
        BOOTSTRAP_TIMEFRAME,
        BOOTSTRAP_START,
        Some(BOOTSTRAP_END),
        config.db.exchange.as_str(),
        "kucoin",
        DEFAULT_INGEST_SLEEP_MS,
        DEFAULT_INGEST_BATCH_SIZE,
        None,
    )
    .await
    .map_err(|err| format!("bootstrap failed while ingesting OHLCV: {err}"))?;

    Ok(())
}

async fn query_coverage(
    db_url: &str,
    table: &str,
    exchange: &str,
    market: &str,
    symbol: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<CoverageStats, String> {
    let (client, connection) = tokio_postgres::connect(db_url, NoTls)
        .await
        .map_err(|err| format!("bootstrap failed: unable to connect to postgres: {err}"))?;
    tokio::spawn(async move {
        if let Err(err) = connection.await {
            eprintln!("postgres connection error during bootstrap coverage query: {err}");
        }
    });

    let query = format!(
        "SELECT MIN(timestamp_utc), MAX(timestamp_utc) \
         FROM {} \
         WHERE exchange=$1 AND market=$2 AND symbol=$3 AND timeframe=$4 \
           AND timestamp_utc >= $5 AND timestamp_utc <= $6",
        table
    );
    let row = client
        .query_one(
            &query,
            &[
                &exchange,
                &market,
                &symbol,
                &BOOTSTRAP_TIMEFRAME,
                &start,
                &end,
            ],
        )
        .await
        .map_err(|err| format!("bootstrap failed while checking OHLCV coverage: {err}"))?;

    let min_ts: Option<DateTime<Utc>> = row.get(0);
    let max_ts: Option<DateTime<Utc>> = row.get(1);

    Ok(CoverageStats { min_ts, max_ts })
}

fn coverage_satisfies_window(
    coverage: &CoverageStats,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> bool {
    match (coverage.min_ts, coverage.max_ts) {
        (Some(min_ts), Some(max_ts)) => min_ts <= start && max_ts >= end,
        _ => false,
    }
}

fn parse_utc_timestamp(raw: &str) -> Result<DateTime<Utc>, String> {
    chrono::DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|err| format!("invalid bootstrap timestamp {raw}: {err}"))
}

fn resolve_bootstrap_config_path(initial_config_path: Option<PathBuf>) -> Result<PathBuf, String> {
    let path = match initial_config_path {
        Some(path) => path,
        None => match env::var("KAIROS_CONFIG") {
            Ok(raw) if !raw.trim().is_empty() => PathBuf::from(raw),
            _ => PathBuf::from(DEFAULT_TUI_CONFIG_PATH),
        },
    };

    if !path.is_file() {
        return Err(format!(
            "bootstrap failed: config file not found (expected {} or set --config/KAIROS_CONFIG)",
            path.display()
        ));
    }
    Ok(path)
}

fn discover_workspace_root(config_path: &Path) -> Result<PathBuf, String> {
    let cwd = env::current_dir()
        .map_err(|err| format!("bootstrap failed: unable to read current directory: {err}"))?;

    if let Some(root) = find_workspace_root(cwd.as_path()) {
        return Ok(root);
    }
    if let Some(parent) = config_path.parent() {
        if let Some(root) = find_workspace_root(parent) {
            return Ok(root);
        }
    }

    Err("bootstrap failed: unable to find workspace root with docker-compose.yml".to_string())
}

fn find_workspace_root(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|path| path.join("docker-compose.yml").is_file())
        .map(Path::to_path_buf)
}

fn start_db_service(workspace_root: &Path) -> Result<(), String> {
    let output = Command::new("docker")
        .arg("compose")
        .arg("up")
        .arg("-d")
        .arg("db")
        .current_dir(workspace_root)
        .output()
        .map_err(|err| {
            format!(
                "bootstrap failed: unable to execute `docker compose up -d db` in {}: {err}",
                workspace_root.display()
            )
        })?;

    if output.status.success() {
        println!("[bootstrap] banco iniciado com docker compose");
        return Ok(());
    }

    Err(format!(
        "bootstrap failed: `docker compose up -d db` returned non-zero exit code ({:?})\n{}",
        output.status.code(),
        render_command_output(&output.stdout, &output.stderr)
    ))
}

fn render_command_output(stdout: &[u8], stderr: &[u8]) -> String {
    let out = String::from_utf8_lossy(stdout).trim().to_string();
    let err = String::from_utf8_lossy(stderr).trim().to_string();
    match (out.is_empty(), err.is_empty()) {
        (true, true) => "no command output".to_string(),
        (false, true) => format!("stdout:\n{out}"),
        (true, false) => format!("stderr:\n{err}"),
        (false, false) => format!("stdout:\n{out}\n\nstderr:\n{err}"),
    }
}

fn resolve_db_url(config: &Config) -> Result<String, String> {
    match config.db.url.as_deref() {
        Some(url) if !url.trim().is_empty() => Ok(url.to_string()),
        _ => env::var("KAIROS_DB_URL").map_err(|_| {
            "bootstrap failed: missing db.url in config and env KAIROS_DB_URL is not set"
                .to_string()
        }),
    }
}

fn ensure_supported_exchange(config: &Config) -> Result<(), String> {
    if config.db.exchange.trim().eq_ignore_ascii_case("kucoin") {
        return Ok(());
    }
    Err(format!(
        "bootstrap failed: db.exchange='{}' is not supported for auto-ingest; use 'kucoin'",
        config.db.exchange
    ))
}

fn ensure_supported_market(market: &str) -> Result<(), String> {
    parse_market(market).map(|_| ())
}

fn parse_market(market: &str) -> Result<Market, String> {
    match market.trim().to_lowercase().as_str() {
        "spot" => Ok(Market::Spot),
        "futures" => Ok(Market::Futures),
        other => Err(format!(
            "bootstrap failed: unsupported db.market='{}'; expected 'spot' or 'futures'",
            other
        )),
    }
}

fn validate_table_name(table: &str) -> Result<(), String> {
    if table.is_empty() {
        return Err("bootstrap failed: db.ohlcv_table is empty".to_string());
    }
    let parts: Vec<&str> = table.split('.').collect();
    if parts.is_empty() || parts.len() > 2 {
        return Err(format!(
            "bootstrap failed: invalid db.ohlcv_table '{}'",
            table
        ));
    }
    for part in parts {
        if part.is_empty() {
            return Err(format!(
                "bootstrap failed: invalid db.ohlcv_table '{}'",
                table
            ));
        }
        let mut chars = part.chars();
        let first = chars
            .next()
            .ok_or_else(|| format!("bootstrap failed: invalid db.ohlcv_table '{}'", table))?;
        if !(first.is_ascii_alphabetic() || first == '_') {
            return Err(format!(
                "bootstrap failed: invalid db.ohlcv_table '{}'",
                table
            ));
        }
        if !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_') {
            return Err(format!(
                "bootstrap failed: invalid db.ohlcv_table '{}'",
                table
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        coverage_satisfies_window, find_workspace_root, parse_market, validate_table_name,
        CoverageStats,
    };
    use chrono::{TimeZone, Utc};

    #[test]
    fn parse_market_accepts_known_values() {
        assert!(parse_market("spot").is_ok());
        assert!(parse_market("futures").is_ok());
        assert!(parse_market("SPOT").is_ok());
        assert!(parse_market(" options ").is_err());
    }

    #[test]
    fn validate_table_name_allows_schema_table() {
        assert!(validate_table_name("ohlcv_candles").is_ok());
        assert!(validate_table_name("public.ohlcv_candles").is_ok());
        assert!(validate_table_name("ohlcv;drop").is_err());
    }

    #[test]
    fn coverage_satisfies_window_only_when_bounds_cover_full_window() {
        let start = Utc.with_ymd_and_hms(2017, 1, 1, 0, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2025, 12, 31, 23, 59, 59).unwrap();

        let ok = CoverageStats {
            min_ts: Some(start),
            max_ts: Some(end),
        };
        assert!(coverage_satisfies_window(&ok, start, end));

        let missing_start = CoverageStats {
            min_ts: Some(start + chrono::Duration::minutes(1)),
            max_ts: Some(end),
        };
        assert!(!coverage_satisfies_window(&missing_start, start, end));
    }

    #[test]
    fn find_workspace_root_detects_docker_compose_ancestor() {
        let unique = format!(
            "kairos_bootstrap_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock before unix epoch")
                .as_nanos()
        );
        let root = std::env::temp_dir().join(&unique);
        let nested = root.join("a").join("b").join("c");
        std::fs::create_dir_all(&nested).expect("nested");
        std::fs::write(root.join("docker-compose.yml"), "services: {}\n").expect("compose file");

        let found = find_workspace_root(nested.as_path()).expect("root should be found");
        assert_eq!(found, root);

        let no_compose = std::env::temp_dir().join(format!("{}_empty", unique));
        let no_compose_nested = no_compose.join("x").join("y");
        std::fs::create_dir_all(&no_compose_nested).expect("nested no compose");
        let missing = find_workspace_root(no_compose_nested.as_path());
        assert!(missing.is_none());

        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(no_compose);
    }
}
