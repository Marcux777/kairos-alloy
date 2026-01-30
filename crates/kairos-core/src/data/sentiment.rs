use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs::File;
use std::path::Path;

#[derive(Debug, Clone, Copy)]
pub enum MissingValuePolicy {
    Error,
    ZeroFill,
    ForwardFill,
    DropRow,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SentimentPoint {
    pub timestamp: i64,
    pub values: Vec<f64>,
}

#[derive(Debug, Default)]
pub struct SentimentReport {
    pub duplicates: usize,
    pub out_of_order: usize,
    pub missing_values: usize,
    pub invalid_values: usize,
    pub dropped_rows: usize,
    pub first_timestamp: Option<i64>,
    pub last_timestamp: Option<i64>,
    pub first_duplicate: Option<i64>,
    pub first_out_of_order: Option<i64>,
    pub schema: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct SentimentJsonRecord {
    pub timestamp_utc: String,
    #[serde(flatten)]
    pub values: serde_json::Value,
}

pub fn load_csv(path: &Path) -> Result<(Vec<SentimentPoint>, SentimentReport), String> {
    load_csv_with_policy(path, MissingValuePolicy::Error)
}

pub fn load_csv_with_policy(
    path: &Path,
    policy: MissingValuePolicy,
) -> Result<(Vec<SentimentPoint>, SentimentReport), String> {
    let file = File::open(path)
        .map_err(|err| format!("failed to open sentiment CSV {}: {}", path.display(), err))?;
    let mut reader = csv::Reader::from_reader(file);

    let mut raw_by_ts: BTreeMap<i64, Vec<Option<f64>>> = BTreeMap::new();
    let mut report = SentimentReport::default();
    let mut last_ts: Option<i64> = None;

    let headers = reader
        .headers()
        .map_err(|err| format!("failed to read sentiment CSV headers: {}", err))?
        .clone();
    report.schema = headers.iter().skip(1).map(|h| h.to_string()).collect();
    let schema_len = report.schema.len();

    for result in reader.records() {
        let record = result.map_err(|err| format!("failed to parse sentiment CSV row: {}", err))?;
        let timestamp_str = record
            .get(0)
            .ok_or_else(|| "missing timestamp_utc column".to_string())?;
        let timestamp = parse_timestamp(timestamp_str)?;

        if report.first_timestamp.is_none() {
            report.first_timestamp = Some(timestamp);
        }

        if let Some(prev) = last_ts {
            if timestamp < prev {
                report.out_of_order += 1;
                if report.first_out_of_order.is_none() {
                    report.first_out_of_order = Some(timestamp);
                }
            }
        }
        last_ts = Some(timestamp);
        report.last_timestamp = Some(timestamp);

        let mut values: Vec<Option<f64>> = vec![None; schema_len];
        for (idx, slot) in values.iter_mut().enumerate() {
            let raw = record.get(idx + 1).unwrap_or("");
            if raw.trim().is_empty() {
                report.missing_values += 1;
                continue;
            }
            match raw.parse::<f64>() {
                Ok(value) => *slot = Some(value),
                Err(_) => {
                    report.invalid_values += 1;
                    let column = headers.get(idx + 1).unwrap_or("unknown");
                    if matches!(policy, MissingValuePolicy::Error) {
                        return Err(format!(
                            "invalid sentiment value '{}' in column {}",
                            raw, column
                        ));
                    }
                }
            }
        }

        if raw_by_ts.insert(timestamp, values).is_some() {
            report.duplicates += 1;
            if report.first_duplicate.is_none() {
                report.first_duplicate = Some(timestamp);
            }
        }
    }

    let mut points = Vec::with_capacity(raw_by_ts.len());
    let mut last_values: Vec<Option<f64>> = vec![None; schema_len];
    for (timestamp, values) in raw_by_ts {
        if values.iter().any(|v| v.is_none()) && matches!(policy, MissingValuePolicy::DropRow) {
            report.dropped_rows += 1;
            continue;
        }
        let mut resolved = Vec::with_capacity(schema_len);
        for (idx, value) in values.into_iter().enumerate() {
            let v = match value {
                Some(v) => {
                    last_values[idx] = Some(v);
                    v
                }
                None => match policy {
                    MissingValuePolicy::Error => {
                        return Err(format!("missing sentiment value at ts={}", timestamp))
                    }
                    MissingValuePolicy::ZeroFill => 0.0,
                    MissingValuePolicy::ForwardFill => last_values[idx].unwrap_or(0.0),
                    MissingValuePolicy::DropRow => 0.0,
                },
            };
            resolved.push(v);
        }
        points.push(SentimentPoint {
            timestamp,
            values: resolved,
        });
    }

    Ok((points, report))
}

pub fn load_json(path: &Path) -> Result<(Vec<SentimentPoint>, SentimentReport), String> {
    load_json_with_policy(path, MissingValuePolicy::Error)
}

pub fn load_json_with_policy(
    path: &Path,
    policy: MissingValuePolicy,
) -> Result<(Vec<SentimentPoint>, SentimentReport), String> {
    let file = File::open(path)
        .map_err(|err| format!("failed to open sentiment JSON {}: {}", path.display(), err))?;
    let records: Vec<SentimentJsonRecord> = serde_json::from_reader(file)
        .map_err(|err| format!("failed to parse sentiment JSON: {}", err))?;

    let mut raw_by_ts: BTreeMap<i64, BTreeMap<String, Option<f64>>> = BTreeMap::new();
    let mut schema_set: BTreeMap<String, ()> = BTreeMap::new();
    let mut report = SentimentReport::default();
    let mut last_ts: Option<i64> = None;

    for record in records {
        let timestamp = parse_timestamp(&record.timestamp_utc)?;

        if report.first_timestamp.is_none() {
            report.first_timestamp = Some(timestamp);
        }

        if let Some(prev) = last_ts {
            if timestamp < prev {
                report.out_of_order += 1;
                if report.first_out_of_order.is_none() {
                    report.first_out_of_order = Some(timestamp);
                }
            }
        }
        last_ts = Some(timestamp);
        report.last_timestamp = Some(timestamp);

        let mut row: BTreeMap<String, Option<f64>> = BTreeMap::new();
        if let Some(obj) = record.values.as_object() {
            for (key, value) in obj.iter() {
                schema_set.insert(key.clone(), ());
                let parsed = value.as_f64();
                if parsed.is_none() {
                    if value.is_null() {
                        report.missing_values += 1;
                    } else {
                        report.invalid_values += 1;
                        if matches!(policy, MissingValuePolicy::Error) {
                            return Err(format!(
                                "invalid sentiment json value for key '{}' at ts={}",
                                key, timestamp
                            ));
                        }
                    }
                }
                row.insert(key.clone(), parsed);
            }
        }

        if raw_by_ts.insert(timestamp, row).is_some() {
            report.duplicates += 1;
            if report.first_duplicate.is_none() {
                report.first_duplicate = Some(timestamp);
            }
        }
    }

    report.schema = schema_set.keys().cloned().collect();
    let schema_len = report.schema.len();

    let mut points = Vec::with_capacity(raw_by_ts.len());
    let mut last_values: Vec<Option<f64>> = vec![None; schema_len];
    for (timestamp, row) in raw_by_ts {
        let mut has_missing = false;
        let mut resolved = Vec::with_capacity(schema_len);
        for (idx, key) in report.schema.iter().enumerate() {
            let value = row.get(key).and_then(|v| *v);
            if value.is_none() {
                has_missing = true;
            }
            let v = match value {
                Some(v) => {
                    last_values[idx] = Some(v);
                    v
                }
                None => match policy {
                    MissingValuePolicy::Error => {
                        return Err(format!(
                            "missing sentiment value for key '{}' at ts={}",
                            key, timestamp
                        ))
                    }
                    MissingValuePolicy::ZeroFill => 0.0,
                    MissingValuePolicy::ForwardFill => last_values[idx].unwrap_or(0.0),
                    MissingValuePolicy::DropRow => 0.0,
                },
            };
            resolved.push(v);
        }

        if has_missing && matches!(policy, MissingValuePolicy::DropRow) {
            report.dropped_rows += 1;
            continue;
        }

        points.push(SentimentPoint {
            timestamp,
            values: resolved,
        });
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
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_tmp_path(name: &str) -> PathBuf {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("kairos_{name}_{}_{}", std::process::id(), now))
    }

    #[test]
    fn load_csv_parses_points() {
        let tmp_path = unique_tmp_path("sentiment_test.csv");
        let csv_data = "timestamp_utc,score\n\
2026-01-01T00:00:00Z,0.5\n\
2026-01-01T00:00:01Z,0.4\n";
        fs::write(&tmp_path, csv_data).expect("write csv");

        let (points, report) = load_csv(&tmp_path).expect("load csv");
        assert_eq!(points.len(), 2);
        assert_eq!(report.duplicates, 0);
        assert_eq!(points[0].values.len(), 1);
    }

    #[test]
    fn load_json_parses_points() {
        let tmp_path = unique_tmp_path("sentiment_test.json");
        let json_data = r#"[
  {"timestamp_utc": "2026-01-01T00:00:00Z", "score": 0.7},
  {"timestamp_utc": "2026-01-01T00:00:02Z", "score": 0.2}
]"#;
        fs::write(&tmp_path, json_data).expect("write json");

        let (points, report) = load_json(&tmp_path).expect("load json");
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
