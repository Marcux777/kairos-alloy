use crate::types::Bar;
use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use postgres::{Client, NoTls};
use serde::Deserialize;
use std::fs::File;
use std::path::Path;

#[derive(Debug, Default)]
pub struct DataQualityReport {
    pub duplicates: usize,
    pub gaps: usize,
    pub out_of_order: usize,
    pub invalid_close: usize,
    pub first_timestamp: Option<i64>,
    pub last_timestamp: Option<i64>,
    pub first_gap: Option<i64>,
    pub first_duplicate: Option<i64>,
    pub first_out_of_order: Option<i64>,
    pub first_invalid_close: Option<i64>,
    pub max_gap_seconds: Option<i64>,
    pub gap_count: usize,
}

#[derive(Debug, Deserialize)]
pub struct OhlcvRecord {
    pub timestamp_utc: String,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

pub fn load_csv(path: &Path) -> Result<(Vec<Bar>, DataQualityReport), String> {
    let file = File::open(path)
        .map_err(|err| format!("failed to open OHLCV CSV {}: {}", path.display(), err))?;
    let mut reader = csv::Reader::from_reader(file);

    let mut bars: Vec<Bar> = Vec::new();
    let mut report = DataQualityReport::default();
    let mut last_ts: Option<i64> = None;
    let mut max_gap: Option<i64> = None;

    for result in reader.deserialize::<OhlcvRecord>() {
        let record = result.map_err(|err| format!("failed to parse CSV row: {}", err))?;
        let timestamp = parse_timestamp(&record.timestamp_utc)?;

        if !record.close.is_finite() || record.close <= 0.0 {
            report.invalid_close += 1;
            if report.first_invalid_close.is_none() {
                report.first_invalid_close = Some(timestamp);
            }
            continue;
        }

        if report.first_timestamp.is_none() {
            report.first_timestamp = Some(timestamp);
        }

        if let Some(prev) = last_ts {
            if timestamp < prev {
                report.out_of_order += 1;
                if report.first_out_of_order.is_none() {
                    report.first_out_of_order = Some(timestamp);
                }
            } else if timestamp > prev {
                let diff = timestamp - prev;
                if diff > 1 {
                    report.gaps += 1;
                    report.gap_count += 1;
                    if report.first_gap.is_none() {
                        report.first_gap = Some(timestamp);
                    }
                    max_gap = Some(max_gap.map_or(diff, |current| current.max(diff)));
                }
            }
        }

        if last_ts == Some(timestamp) {
            report.duplicates += 1;
            if report.first_duplicate.is_none() {
                report.first_duplicate = Some(timestamp);
            }
            if let Some(last) = bars.last_mut() {
                last.open = record.open;
                last.high = record.high;
                last.low = record.low;
                last.close = record.close;
                last.volume = record.volume;
                report.last_timestamp = Some(timestamp);
                continue;
            }
        }

        last_ts = Some(timestamp);
        report.last_timestamp = Some(timestamp);
        bars.push(Bar {
            symbol: "UNKNOWN".to_string(),
            timestamp,
            open: record.open,
            high: record.high,
            low: record.low,
            close: record.close,
            volume: record.volume,
        });
    }
    report.max_gap_seconds = max_gap;

    Ok((bars, report))
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
    validate_table_name(table)?;
    let mut client = Client::connect(db_url, NoTls)
        .map_err(|err| format!("failed to connect to postgres: {err}"))?;

    let query = format!(
        "SELECT timestamp_utc, open, high, low, close, volume FROM {} \
         WHERE exchange=$1 AND market=$2 AND symbol=$3 AND timeframe=$4 \
         ORDER BY timestamp_utc ASC",
        table
    );
    let rows = client
        .query(&query, &[&exchange, &market, &symbol, &timeframe])
        .map_err(|err| format!("failed to query OHLCV: {err}"))?;

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
    Ok((bars, report))
}

fn validate_table_name(table: &str) -> Result<(), String> {
    if table.is_empty() {
        return Err("table name is empty".to_string());
    }
    let valid = table
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '.');
    if !valid {
        return Err(format!("invalid table name: {table}"));
    }
    Ok(())
}

fn parse_timestamp(value: &str) -> Result<i64, String> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(value) {
        return Ok(dt.timestamp());
    }
    if let Ok(dt) = DateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%z") {
        return Ok(dt.timestamp());
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S") {
        let dt: DateTime<Utc> = Utc.from_utc_datetime(&naive);
        return Ok(dt.timestamp());
    }

    Err(format!("unsupported timestamp format: {}", value))
}

#[cfg(test)]
mod tests {
    use super::{load_csv, validate_table_name};
    use std::fs;
    use std::path::Path;

    #[test]
    fn load_csv_detects_duplicates_and_gaps() {
        let tmp_path = Path::new("/tmp/kairos_ohlcv_test.csv");
        let csv_data = "timestamp_utc,open,high,low,close,volume\n\
2026-01-01T00:00:00Z,1,1,1,1,1\n\
2026-01-01T00:00:00Z,1,1,1,1,1\n\
2026-01-01T00:00:02Z,1,1,1,1,1\n";
        fs::write(tmp_path, csv_data).expect("write csv");

        let (bars, report) = load_csv(tmp_path).expect("load csv");
        assert_eq!(bars.len(), 2);
        assert_eq!(report.duplicates, 1);
        assert_eq!(report.gaps, 1);
        assert_eq!(report.invalid_close, 0);
    }

    #[test]
    fn validate_table_name_accepts_schema() {
        assert!(validate_table_name("ohlcv_candles").is_ok());
        assert!(validate_table_name("public.ohlcv_candles").is_ok());
        assert!(validate_table_name("").is_err());
        assert!(validate_table_name("ohlcv;drop").is_err());
    }
}
