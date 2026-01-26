use crate::types::Bar;
use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use serde::Deserialize;
use std::fs::File;
use std::path::Path;

#[derive(Debug, Default)]
pub struct DataQualityReport {
    pub duplicates: usize,
    pub gaps: usize,
    pub out_of_order: usize,
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

    let mut bars = Vec::new();
    let mut report = DataQualityReport::default();
    let mut last_ts: Option<i64> = None;

    for result in reader.deserialize::<OhlcvRecord>() {
        let record = result.map_err(|err| format!("failed to parse CSV row: {}", err))?;
        let timestamp = parse_timestamp(&record.timestamp_utc)?;

        if let Some(prev) = last_ts {
            if timestamp == prev {
                report.duplicates += 1;
            } else if timestamp < prev {
                report.out_of_order += 1;
            } else if timestamp > prev {
                let diff = timestamp - prev;
                if diff > 1 {
                    report.gaps += 1;
                }
            }
        }

        last_ts = Some(timestamp);
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

    Ok((bars, report))
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
    use super::load_csv;
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
        assert_eq!(bars.len(), 3);
        assert_eq!(report.duplicates, 1);
        assert_eq!(report.gaps, 1);
    }
}
