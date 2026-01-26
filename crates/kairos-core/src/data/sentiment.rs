use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs::File;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct SentimentPoint {
    pub timestamp: i64,
    pub values: Vec<f64>,
}

#[derive(Debug, Default)]
pub struct SentimentReport {
    pub duplicates: usize,
    pub out_of_order: usize,
}

#[derive(Debug, Deserialize)]
struct SentimentJsonRecord {
    pub timestamp_utc: String,
    #[serde(flatten)]
    pub values: serde_json::Value,
}

pub fn load_csv(path: &Path) -> Result<(Vec<SentimentPoint>, SentimentReport), String> {
    let file = File::open(path)
        .map_err(|err| format!("failed to open sentiment CSV {}: {}", path.display(), err))?;
    let mut reader = csv::Reader::from_reader(file);

    let mut points = Vec::new();
    let mut report = SentimentReport::default();
    let mut last_ts: Option<i64> = None;

    let headers = reader
        .headers()
        .map_err(|err| format!("failed to read sentiment CSV headers: {}", err))?
        .clone();

    for result in reader.records() {
        let record = result.map_err(|err| format!("failed to parse sentiment CSV row: {}", err))?;
        let timestamp_str = record
            .get(0)
            .ok_or_else(|| "missing timestamp_utc column".to_string())?;
        let timestamp = parse_timestamp(timestamp_str)?;

        if let Some(prev) = last_ts {
            if timestamp == prev {
                report.duplicates += 1;
            } else if timestamp < prev {
                report.out_of_order += 1;
            }
        }
        last_ts = Some(timestamp);

        let mut values = Vec::new();
        for (idx, raw) in record.iter().enumerate() {
            if idx == 0 {
                continue;
            }
            if let Ok(value) = raw.parse::<f64>() {
                values.push(value);
            } else {
                let column = headers.get(idx).unwrap_or("unknown");
                return Err(format!(
                    "invalid sentiment value '{}' in column {}",
                    raw, column
                ));
            }
        }

        points.push(SentimentPoint { timestamp, values });
    }

    Ok((points, report))
}

pub fn load_json(path: &Path) -> Result<(Vec<SentimentPoint>, SentimentReport), String> {
    let file = File::open(path)
        .map_err(|err| format!("failed to open sentiment JSON {}: {}", path.display(), err))?;
    let records: Vec<SentimentJsonRecord> = serde_json::from_reader(file)
        .map_err(|err| format!("failed to parse sentiment JSON: {}", err))?;

    let mut points = Vec::new();
    let mut report = SentimentReport::default();
    let mut last_ts: Option<i64> = None;

    for record in records {
        let timestamp = parse_timestamp(&record.timestamp_utc)?;

        if let Some(prev) = last_ts {
            if timestamp == prev {
                report.duplicates += 1;
            } else if timestamp < prev {
                report.out_of_order += 1;
            }
        }
        last_ts = Some(timestamp);

        let mut values = Vec::new();
        if let Some(obj) = record.values.as_object() {
            for (_key, value) in obj.iter() {
                if let Some(value) = value.as_f64() {
                    values.push(value);
                }
            }
        }

        points.push(SentimentPoint { timestamp, values });
    }

    Ok((points, report))
}

pub fn align_with_bars(
    bar_timestamps: &[i64],
    sentiment: &[SentimentPoint],
    sentiment_lag_seconds: i64,
) -> Vec<Option<SentimentPoint>> {
    let mut map: BTreeMap<i64, SentimentPoint> = BTreeMap::new();
    for point in sentiment {
        map.insert(point.timestamp, point.clone());
    }

    bar_timestamps
        .iter()
        .map(|ts| {
            let cutoff = ts.saturating_sub(sentiment_lag_seconds);
            map.range(..=cutoff)
                .next_back()
                .map(|(_, point)| point.clone())
        })
        .collect()
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
    use super::{align_with_bars, load_csv, load_json};
    use std::fs;
    use std::path::Path;

    #[test]
    fn load_csv_parses_points() {
        let tmp_path = Path::new("/tmp/kairos_sentiment_test.csv");
        let csv_data = "timestamp_utc,score\n\
2026-01-01T00:00:00Z,0.5\n\
2026-01-01T00:00:01Z,0.4\n";
        fs::write(tmp_path, csv_data).expect("write csv");

        let (points, report) = load_csv(tmp_path).expect("load csv");
        assert_eq!(points.len(), 2);
        assert_eq!(report.duplicates, 0);
        assert_eq!(points[0].values.len(), 1);
    }

    #[test]
    fn load_json_parses_points() {
        let tmp_path = Path::new("/tmp/kairos_sentiment_test.json");
        let json_data = r#"[
  {"timestamp_utc": "2026-01-01T00:00:00Z", "score": 0.7},
  {"timestamp_utc": "2026-01-01T00:00:02Z", "score": 0.2}
]"#;
        fs::write(tmp_path, json_data).expect("write json");

        let (points, report) = load_json(tmp_path).expect("load json");
        assert_eq!(points.len(), 2);
        assert_eq!(report.out_of_order, 0);
        assert_eq!(points[0].values.len(), 1);
    }

    #[test]
    fn align_with_bars_applies_lag() {
        let sentiment_points = vec![
            super::SentimentPoint {
                timestamp: 10,
                values: vec![0.1],
            },
            super::SentimentPoint {
                timestamp: 20,
                values: vec![0.2],
            },
        ];
        let bar_timestamps = vec![15, 25];
        let aligned = align_with_bars(&bar_timestamps, &sentiment_points, 5);
        assert_eq!(aligned.len(), 2);
        assert_eq!(aligned[0].as_ref().unwrap().timestamp, 10);
        assert_eq!(aligned[1].as_ref().unwrap().timestamp, 20);
    }
}
