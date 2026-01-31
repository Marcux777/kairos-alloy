use chrono::{DateTime, Utc};
use kairos_domain::services::ohlcv::DataQualityReport;
use kairos_domain::value_objects::bar::Bar;
use postgres::{Client, NoTls};
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct PostgresMarketDataRepository {
    pub db_url: String,
    pub ohlcv_table: String,
}

impl PostgresMarketDataRepository {
    pub fn new(db_url: String, ohlcv_table: String) -> Self {
        Self {
            db_url,
            ohlcv_table,
        }
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
            &self.db_url,
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
    db_url: &str,
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

    validate_table_name(table)?;

    let connect_start = Instant::now();
    let mut client = match Client::connect(db_url, NoTls) {
        Ok(client) => client,
        Err(err) => {
            metrics::counter!("kairos.infra.postgres.load_ohlcv.calls", "result" => "err")
                .increment(1);
            metrics::counter!("kairos.infra.postgres.load_ohlcv.errors", "stage" => "connect")
                .increment(1);
            return Err(format!("failed to connect to postgres: {err}"));
        }
    };
    metrics::histogram!("kairos.infra.postgres.connect_ms")
        .record(connect_start.elapsed().as_millis() as f64);

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
            metrics::counter!("kairos.infra.postgres.load_ohlcv.calls", "result" => "err")
                .increment(1);
            metrics::counter!("kairos.infra.postgres.load_ohlcv.errors", "stage" => "query")
                .increment(1);
            return Err(format!("failed to query OHLCV: {err}"));
        }
    };
    metrics::histogram!("kairos.infra.postgres.query_ms")
        .record(query_start.elapsed().as_millis() as f64);

    let mut bars = Vec::with_capacity(rows.len());
    let mut report = DataQualityReport::default();
    let mut last_ts: Option<i64> = None;
    let mut max_gap: Option<i64> = None;
    let step = expected_step_seconds.unwrap_or(1);

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
        if report.first_timestamp.is_none() {
            report.first_timestamp = Some(ts);
        }

        if let Some(prev) = last_ts {
            if ts == prev {
                report.duplicates += 1;
                if report.first_duplicate.is_none() {
                    report.first_duplicate = Some(ts);
                }
            } else if ts < prev {
                report.out_of_order += 1;
                if report.first_out_of_order.is_none() {
                    report.first_out_of_order = Some(ts);
                }
            } else if ts > prev {
                let diff = ts - prev;
                if diff > step {
                    report.gaps += 1;
                    report.gap_count += 1;
                    if report.first_gap.is_none() {
                        report.first_gap = Some(ts);
                    }
                    max_gap = Some(max_gap.map_or(diff, |current| current.max(diff)));
                }
            }
        }

        last_ts = Some(ts);
        report.last_timestamp = Some(ts);
        bars.push(Bar {
            symbol: symbol.to_string(),
            timestamp: ts,
            open: row.get(1),
            high: row.get(2),
            low: row.get(3),
            close,
            volume: row.get(5),
        });
    }

    report.max_gap_seconds = max_gap;

    metrics::counter!("kairos.infra.postgres.load_ohlcv.calls", "result" => "ok").increment(1);
    metrics::histogram!("kairos.infra.postgres.load_ohlcv_ms")
        .record(overall_start.elapsed().as_millis() as f64);
    metrics::gauge!("kairos.infra.postgres.load_ohlcv.bars_loaded").set(bars.len() as f64);
    metrics::gauge!("kairos.infra.postgres.load_ohlcv.invalid_close")
        .set(report.invalid_close as f64);
    metrics::gauge!("kairos.infra.postgres.load_ohlcv.duplicates").set(report.duplicates as f64);
    metrics::gauge!("kairos.infra.postgres.load_ohlcv.gaps").set(report.gaps as f64);

    tracing::debug!(
        bars = bars.len(),
        invalid_close = report.invalid_close,
        duplicates = report.duplicates,
        gaps = report.gaps,
        out_of_order = report.out_of_order,
        "loaded OHLCV"
    );
    Ok((bars, report))
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
    use super::{load_postgres, validate_table_name};

    #[test]
    fn validate_table_name_accepts_schema() {
        assert!(validate_table_name("ohlcv_candles").is_ok());
        assert!(validate_table_name("public.ohlcv_candles").is_ok());
        assert!(validate_table_name("").is_err());
        assert!(validate_table_name("ohlcv;drop").is_err());
    }

    #[test]
    fn load_postgres_rejects_invalid_table_name_before_connect() {
        let err = load_postgres(
            "postgres://invalid",
            "ohlcv;drop",
            "ex",
            "spot",
            "BTCUSD",
            "1m",
            None,
        )
        .expect_err("invalid table name");
        assert!(err.contains("invalid table name"));
    }

    #[test]
    fn load_postgres_errors_on_invalid_db_url() {
        let err = load_postgres(
            "not a url",
            "ohlcv_candles",
            "ex",
            "spot",
            "BTCUSD",
            "1m",
            None,
        )
        .expect_err("invalid db url should fail fast");
        assert!(err.contains("failed to connect to postgres"));
    }
}
