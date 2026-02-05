use chrono::{DateTime, Utc};
use kairos_domain::services::ohlcv::DataQualityReport;
use kairos_domain::value_objects::bar::Bar;
use postgres::NoTls;
use r2d2::Pool;
use r2d2_postgres::PostgresConnectionManager;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct PostgresMarketDataRepository {
    pool: Pool<PostgresConnectionManager<NoTls>>,
    pub ohlcv_table: String,
}

impl PostgresMarketDataRepository {
    pub fn new(db_url: String, ohlcv_table: String, pool_max_size: u32) -> Result<Self, String> {
        if let Err(err) = validate_table_name(&ohlcv_table) {
            return Err(format!("invalid ohlcv_table '{}': {}", ohlcv_table, err));
        }

        let config = db_url
            .parse::<postgres::Config>()
            .map_err(|err| format!("invalid postgres db url: {err}"))?;
        let manager = PostgresConnectionManager::new(config, NoTls);
        let pool = Pool::builder()
            .max_size(pool_max_size)
            .build(manager)
            .map_err(|err| format!("failed to build postgres pool: {err}"))?;

        Ok(Self { pool, ohlcv_table })
    }
}

impl kairos_domain::repositories::market_data::MarketDataRepository
    for PostgresMarketDataRepository
{
    fn load_ohlcv(
        &self,
        query: &kairos_domain::repositories::market_data::OhlcvQuery,
    ) -> Result<(Vec<Bar>, DataQualityReport), String> {
        load_postgres(
            &self.pool,
            &self.ohlcv_table,
            &query.exchange,
            &query.market,
            &query.symbol,
            &query.timeframe,
            query.expected_step_seconds,
        )
    }
}

pub fn load_postgres(
    pool: &Pool<PostgresConnectionManager<NoTls>>,
    table: &str,
    exchange: &str,
    market: &str,
    symbol: &str,
    timeframe: &str,
    expected_step_seconds: Option<i64>,
) -> Result<(Vec<Bar>, DataQualityReport), String> {
    let overall_start = Instant::now();
    let span = tracing::info_span!(
        "infra.postgres.load_ohlcv",
        table = %table,
        exchange = %exchange,
        market = %market,
        symbol = %symbol,
        timeframe = %timeframe
    );
    let _enter = span.enter();

    if let Err(err) = validate_table_name(table) {
        metrics::counter!("kairos.infra.postgres.load_ohlcv.calls_total", "result" => "err")
            .increment(1);
        metrics::counter!(
            "kairos.infra.postgres.load_ohlcv.errors_total",
            "stage" => "validate_table"
        )
        .increment(1);
        tracing::warn!(error = %err, "invalid table name");
        return Err(err);
    }

    let get_start = Instant::now();
    let mut client = match pool.get() {
        Ok(client) => client,
        Err(err) => {
            metrics::counter!("kairos.infra.postgres.load_ohlcv.calls_total", "result" => "err")
                .increment(1);
            metrics::counter!(
                "kairos.infra.postgres.load_ohlcv.errors_total",
                "stage" => "pool_get"
            )
            .increment(1);
            metrics::counter!("kairos.infra.postgres.pool.get.errors_total", "stage" => "get")
                .increment(1);
            tracing::error!(error = %err, "failed to checkout postgres connection");
            return Err(format!("failed to checkout postgres connection: {err}"));
        }
    };
    metrics::histogram!("kairos.infra.postgres.pool.get_ms")
        .record(get_start.elapsed().as_secs_f64() * 1000.0);

    let query = format!(
        "SELECT timestamp_utc, open, high, low, close, volume FROM {} \
         WHERE exchange=$1 AND market=$2 AND symbol=$3 AND timeframe=$4 \
         ORDER BY timestamp_utc ASC",
        table
    );
    let query_start = Instant::now();
    let rows = match client.query(&query, &[&exchange, &market, &symbol, &timeframe]) {
        Ok(rows) => rows,
        Err(err) => {
            metrics::counter!("kairos.infra.postgres.load_ohlcv.calls_total", "result" => "err")
                .increment(1);
            metrics::counter!("kairos.infra.postgres.load_ohlcv.errors_total", "stage" => "query")
                .increment(1);
            tracing::error!(error = %err, "failed to query OHLCV");
            return Err(format!("failed to query OHLCV: {err}"));
        }
    };
    metrics::histogram!("kairos.infra.postgres.query_ms")
        .record(query_start.elapsed().as_secs_f64() * 1000.0);

    let rows_len = rows.len();

    let mut bars_raw = Vec::with_capacity(rows.len());
    let mut report = DataQualityReport::default();
    let mut last_seen_ts: Option<i64> = None;

    for row in rows {
        let timestamp: DateTime<Utc> = row.get(0);
        let ts = timestamp.timestamp();
        let close: f64 = row.get(4);
        if !close.is_finite() || close <= 0.0 {
            report.invalid_close += 1;
            if report.first_invalid_close.is_none() {
                report.first_invalid_close = Some(ts);
            }
            continue;
        }

        if let Some(prev) = last_seen_ts {
            if ts < prev {
                report.out_of_order += 1;
                if report.first_out_of_order.is_none() {
                    report.first_out_of_order = Some(ts);
                }
            }
        }

        last_seen_ts = Some(ts);
        bars_raw.push(Bar {
            symbol: symbol.to_string(),
            timestamp: ts,
            open: row.get(1),
            high: row.get(2),
            low: row.get(3),
            close,
            volume: row.get(5),
        });
    }

    if bars_raw.is_empty() {
        metrics::counter!("kairos.infra.postgres.load_ohlcv.calls_total", "result" => "ok")
            .increment(1);
        metrics::histogram!("kairos.infra.postgres.load_ohlcv_ms")
            .record(overall_start.elapsed().as_secs_f64() * 1000.0);
        metrics::gauge!("kairos.infra.postgres.load_ohlcv.rows_returned").set(rows_len as f64);
        metrics::counter!("kairos.infra.postgres.load_ohlcv.rows_returned_total")
            .increment(rows_len as u64);
        metrics::gauge!("kairos.infra.postgres.load_ohlcv.bars_loaded").set(0.0);
        metrics::counter!("kairos.infra.postgres.load_ohlcv.bars_loaded_total").increment(0u64);
        metrics::gauge!("kairos.infra.postgres.load_ohlcv.invalid_close")
            .set(report.invalid_close as f64);
        metrics::gauge!("kairos.infra.postgres.load_ohlcv.duplicates").set(0.0);
        metrics::gauge!("kairos.infra.postgres.load_ohlcv.gaps").set(0.0);

        tracing::debug!(
            rows = rows_len,
            bars = 0,
            invalid_close = report.invalid_close,
            duplicates = report.duplicates,
            gaps = report.gaps,
            out_of_order = report.out_of_order,
            "loaded OHLCV"
        );
        return Ok((Vec::new(), report));
    }

    let bars = canonicalize_bars(bars_raw, expected_step_seconds, &mut report);

    metrics::counter!("kairos.infra.postgres.load_ohlcv.calls_total", "result" => "ok")
        .increment(1);
    metrics::histogram!("kairos.infra.postgres.load_ohlcv_ms")
        .record(overall_start.elapsed().as_secs_f64() * 1000.0);
    metrics::gauge!("kairos.infra.postgres.load_ohlcv.rows_returned").set(rows_len as f64);
    metrics::counter!("kairos.infra.postgres.load_ohlcv.rows_returned_total")
        .increment(rows_len as u64);
    metrics::gauge!("kairos.infra.postgres.load_ohlcv.bars_loaded").set(bars.len() as f64);
    metrics::counter!("kairos.infra.postgres.load_ohlcv.bars_loaded_total")
        .increment(bars.len() as u64);
    metrics::gauge!("kairos.infra.postgres.load_ohlcv.invalid_close")
        .set(report.invalid_close as f64);
    metrics::gauge!("kairos.infra.postgres.load_ohlcv.duplicates").set(report.duplicates as f64);
    metrics::gauge!("kairos.infra.postgres.load_ohlcv.gaps").set(report.gaps as f64);

    tracing::debug!(
        rows = rows_len,
        bars = bars.len(),
        invalid_close = report.invalid_close,
        duplicates = report.duplicates,
        gaps = report.gaps,
        out_of_order = report.out_of_order,
        "loaded OHLCV"
    );
    Ok((bars, report))
}

fn canonicalize_bars(
    mut bars_raw: Vec<Bar>,
    expected_step_seconds: Option<i64>,
    report: &mut DataQualityReport,
) -> Vec<Bar> {
    report.duplicates = 0;
    report.gaps = 0;
    report.gap_count = 0;
    report.first_timestamp = None;
    report.last_timestamp = None;
    report.first_gap = None;
    report.first_duplicate = None;
    report.max_gap_seconds = None;

    bars_raw.sort_by_key(|bar| bar.timestamp);

    let mut bars: Vec<Bar> = Vec::with_capacity(bars_raw.len());
    for bar in bars_raw {
        if let Some(last) = bars.last_mut() {
            if bar.timestamp == last.timestamp {
                report.duplicates += 1;
                if report.first_duplicate.is_none() {
                    report.first_duplicate = Some(bar.timestamp);
                }
                *last = bar;
                continue;
            }
        }
        bars.push(bar);
    }

    report.first_timestamp = bars.first().map(|b| b.timestamp);
    report.last_timestamp = bars.last().map(|b| b.timestamp);

    let step = expected_step_seconds.unwrap_or(1).max(1);
    let mut max_gap: Option<i64> = None;
    let mut last_unique_ts: Option<i64> = None;
    for bar in &bars {
        let ts = bar.timestamp;
        if let Some(prev) = last_unique_ts {
            let diff = ts - prev;
            if diff > step {
                report.gaps += 1;
                report.gap_count += ((diff - 1) / step) as usize;
                if report.first_gap.is_none() {
                    report.first_gap = Some(ts);
                }
                max_gap = Some(max_gap.map_or(diff, |current| current.max(diff)));
            }
        }
        last_unique_ts = Some(ts);
    }
    report.max_gap_seconds = max_gap;

    bars
}

fn validate_table_name(table: &str) -> Result<(), String> {
    if table.is_empty() {
        return Err("table name is empty".to_string());
    }
    let parts: Vec<&str> = table.split('.').collect();
    if parts.is_empty() || parts.len() > 2 {
        return Err(format!("invalid table name: {table}"));
    }
    for part in parts {
        if part.is_empty() {
            return Err(format!("invalid table name: {table}"));
        }
        let mut chars = part.chars();
        let first = match chars.next() {
            Some(ch) => ch,
            None => return Err(format!("invalid table name: {table}")),
        };
        if !(first.is_ascii_alphabetic() || first == '_') {
            return Err(format!("invalid table name: {table}"));
        }
        if !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_') {
            return Err(format!("invalid table name: {table}"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{canonicalize_bars, load_postgres, validate_table_name};
    use kairos_domain::services::ohlcv::DataQualityReport;
    use kairos_domain::value_objects::bar::Bar;
    use postgres::NoTls;
    use r2d2::Pool;
    use r2d2_postgres::PostgresConnectionManager;

    #[test]
    fn validate_table_name_accepts_schema() {
        assert!(validate_table_name("ohlcv_candles").is_ok());
        assert!(validate_table_name("public.ohlcv_candles").is_ok());
        assert!(validate_table_name("").is_err());
        assert!(validate_table_name("ohlcv;drop").is_err());
    }

    #[test]
    fn load_postgres_rejects_invalid_table_name_before_connect() {
        let pool = build_pool("postgres://invalid");
        let err = load_postgres(&pool, "ohlcv;drop", "ex", "spot", "BTCUSD", "1m", None)
            .expect_err("invalid table name");
        assert!(err.contains("invalid table name"));
    }

    #[test]
    fn load_postgres_errors_on_invalid_db_url() {
        let err = super::PostgresMarketDataRepository::new(
            "not a url".to_string(),
            "ohlcv_candles".to_string(),
            1,
        )
        .expect_err("invalid db url should fail fast");
        assert!(err.contains("invalid postgres db url"));
    }

    #[test]
    fn canonicalize_bars_dedupes_keeps_last_and_counts_missing_bars() {
        let mut report = DataQualityReport::default();
        let raw = vec![
            Bar {
                symbol: "BTCUSD".to_string(),
                timestamp: 0,
                open: 1.0,
                high: 1.0,
                low: 1.0,
                close: 1.0,
                volume: 1.0,
            },
            Bar {
                symbol: "BTCUSD".to_string(),
                timestamp: 0,
                open: 2.0,
                high: 2.0,
                low: 2.0,
                close: 2.0,
                volume: 2.0,
            },
            Bar {
                symbol: "BTCUSD".to_string(),
                timestamp: 300,
                open: 3.0,
                high: 3.0,
                low: 3.0,
                close: 3.0,
                volume: 3.0,
            },
        ];

        let bars = canonicalize_bars(raw, Some(60), &mut report);
        assert_eq!(bars.len(), 2);
        assert_eq!(report.duplicates, 1);
        assert_eq!(bars[0].timestamp, 0);
        assert!((bars[0].close - 2.0).abs() < 1e-9);
        assert_eq!(report.gaps, 1);
        assert_eq!(report.gap_count, 4);
    }

    fn build_pool(db_url: &str) -> Pool<PostgresConnectionManager<NoTls>> {
        let config = db_url
            .parse::<postgres::Config>()
            .expect("test db url should parse");
        let manager = PostgresConnectionManager::new(config, NoTls);
        Pool::builder().max_size(1).build_unchecked(manager)
    }
}
