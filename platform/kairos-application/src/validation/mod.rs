use crate::config::Config;
use crate::shared::{
    normalize_timeframe_label, parse_duration_like, resolve_sentiment_missing_policy,
};
use kairos_domain::repositories::market_data::{MarketDataRepository, OhlcvQuery};
use kairos_domain::repositories::sentiment::{
    SentimentFormat, SentimentQuery, SentimentRepository,
};
use kairos_domain::services::ohlcv::{data_quality_from_bars, resample_bars, DataQualityReport};
use std::path::PathBuf;
use std::time::Instant;
use tracing::info_span;

pub fn validate(
    config: &Config,
    strict: bool,
    market_data: &dyn MarketDataRepository,
    sentiment_repo: &dyn SentimentRepository,
) -> Result<serde_json::Value, String> {
    let _span = info_span!(
        "validate",
        strict = strict,
        run_id = %config.run.run_id,
        symbol = %config.run.symbol,
        timeframe = %config.run.timeframe
    )
    .entered();

    let stage_start = Instant::now();
    let expected_step = parse_duration_like(&config.run.timeframe)?;
    let timeframe_label = normalize_timeframe_label(&config.run.timeframe)?;
    let source_timeframe_label = normalize_timeframe_label(
        config
            .db
            .source_timeframe
            .as_deref()
            .unwrap_or(&timeframe_label),
    )?;
    let source_step = parse_duration_like(&source_timeframe_label)?;

    let (source_bars, source_report) = market_data.load_ohlcv(&OhlcvQuery {
        exchange: config.db.exchange.to_lowercase(),
        market: config.db.market.to_lowercase(),
        symbol: config.run.symbol.clone(),
        timeframe: source_timeframe_label.clone(),
        expected_step_seconds: Some(source_step),
    })?;
    let source_rows = source_bars.len();
    metrics::histogram!("kairos.validate.load_ohlcv_ms")
        .record(stage_start.elapsed().as_millis() as f64);

    let (ohlcv_report, ohlcv_source_report_json, effective_rows, resampled) =
        if source_timeframe_label != timeframe_label {
            if source_step > expected_step {
                return Err(format!(
                    "cannot resample OHLCV: source timeframe ({}) is larger than run timeframe ({})",
                    source_timeframe_label, timeframe_label
                ));
            }
            let resampled_bars = resample_bars(&source_bars, expected_step)?;
            let report = data_quality_from_bars(&resampled_bars, Some(expected_step));
            (
                report,
                Some(data_quality_json(&source_report, source_rows)),
                resampled_bars.len(),
                true,
            )
        } else {
            (source_report, None, source_rows, false)
        };

    let (s_duplicates, s_out_of_order, s_missing, s_invalid, s_dropped, sentiment_schema) =
        if let Some(path) = &config.paths.sentiment_path {
            let path_buf = PathBuf::from(path);
            let ext = path_buf
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_lowercase();
            let format = if ext == "json" {
                SentimentFormat::Json
            } else {
                SentimentFormat::Csv
            };
            let missing_policy = resolve_sentiment_missing_policy(config);
            let (_points, report) = sentiment_repo.load_sentiment(&SentimentQuery {
                path: path_buf,
                format,
                missing_policy,
            })?;
            (
                report.duplicates,
                report.out_of_order,
                report.missing_values,
                report.invalid_values,
                report.dropped_rows,
                report.schema,
            )
        } else {
            (0, 0, 0, 0, 0, Vec::new())
        };

    let limits = config.data_quality.as_ref();
    let max_gaps = limits.and_then(|l| l.max_gaps).unwrap_or(0);
    let max_missing_bars = limits.and_then(|l| l.max_missing_bars).unwrap_or(0);
    let max_duplicates = limits.and_then(|l| l.max_duplicates).unwrap_or(0);
    let max_out_of_order = limits.and_then(|l| l.max_out_of_order).unwrap_or(0);
    let max_invalid_close = limits.and_then(|l| l.max_invalid_close).unwrap_or(0);
    let max_sentiment_missing = limits.and_then(|l| l.max_sentiment_missing).unwrap_or(0);
    let max_sentiment_invalid = limits.and_then(|l| l.max_sentiment_invalid).unwrap_or(0);
    let max_sentiment_dropped = limits.and_then(|l| l.max_sentiment_dropped).unwrap_or(0);

    if strict
        && (ohlcv_report.gaps > max_gaps
            || ohlcv_report.gap_count > max_missing_bars
            || ohlcv_report.duplicates > max_duplicates
            || ohlcv_report.out_of_order > max_out_of_order
            || ohlcv_report.invalid_close > max_invalid_close
            || s_duplicates > max_duplicates
            || s_out_of_order > max_out_of_order
            || s_missing > max_sentiment_missing
            || s_invalid > max_sentiment_invalid
            || s_dropped > max_sentiment_dropped)
    {
        return Err("strict validation failed: data quality limits exceeded".to_string());
    }

    metrics::gauge!("kairos.validate.ohlcv.gaps").set(ohlcv_report.gaps as f64);
    metrics::gauge!("kairos.validate.ohlcv.duplicates").set(ohlcv_report.duplicates as f64);
    metrics::gauge!("kairos.validate.ohlcv.out_of_order").set(ohlcv_report.out_of_order as f64);
    metrics::gauge!("kairos.validate.ohlcv.invalid_close").set(ohlcv_report.invalid_close as f64);
    metrics::gauge!("kairos.validate.sentiment.missing").set(s_missing as f64);
    metrics::gauge!("kairos.validate.sentiment.invalid").set(s_invalid as f64);
    metrics::gauge!("kairos.validate.sentiment.dropped").set(s_dropped as f64);

    Ok(serde_json::json!({
        "ohlcv_resample": if resampled { serde_json::json!({
            "from_timeframe": source_timeframe_label,
            "to_timeframe": timeframe_label,
            "source_step_seconds": source_step,
            "target_step_seconds": expected_step,
            "source_rows": source_rows,
            "resampled_rows": effective_rows,
        }) } else { serde_json::Value::Null },
        "ohlcv_source": ohlcv_source_report_json,
        "ohlcv": data_quality_json(&ohlcv_report, effective_rows),
        "sentiment": {
            "duplicates": s_duplicates,
            "out_of_order": s_out_of_order,
            "missing_values": s_missing,
            "invalid_values": s_invalid,
            "dropped_rows": s_dropped,
            "schema": sentiment_schema,
        },
        "limits": {
            "max_gaps": max_gaps,
            "max_missing_bars": max_missing_bars,
            "max_duplicates": max_duplicates,
            "max_out_of_order": max_out_of_order,
            "max_invalid_close": max_invalid_close,
            "max_sentiment_missing": max_sentiment_missing,
            "max_sentiment_invalid": max_sentiment_invalid,
            "max_sentiment_dropped": max_sentiment_dropped,
        },
        "strict": strict
    }))
}

fn data_quality_json(report: &DataQualityReport, rows: usize) -> serde_json::Value {
    serde_json::json!({
        "rows": rows,
        "duplicates": report.duplicates,
        "gaps": report.gaps,
        "missing_bars": report.gap_count,
        "out_of_order": report.out_of_order,
        "invalid_close": report.invalid_close,
        "first_timestamp": report.first_timestamp,
        "last_timestamp": report.last_timestamp,
        "first_gap": report.first_gap,
        "first_duplicate": report.first_duplicate,
        "first_out_of_order": report.first_out_of_order,
        "first_invalid_close": report.first_invalid_close,
        "max_gap_seconds": report.max_gap_seconds,
        "gap_count": report.gap_count,
    })
}
