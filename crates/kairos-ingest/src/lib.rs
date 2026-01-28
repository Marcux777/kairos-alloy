use clap::ValueEnum;
use chrono::{DateTime, TimeZone, Utc};
use reqwest::Client;
use serde::Deserialize;
use std::path::Path;
use std::time::Duration;
use tokio_postgres::{Client as PgClient, NoTls};

const KUCOIN_SPOT_BASE: &str = "https://api.kucoin.com";
const KUCOIN_FUTURES_BASE: &str = "https://api-futures.kucoin.com";
const KUCOIN_SPOT_LIMIT: i64 = 1500;
const KUCOIN_FUTURES_LIMIT: i64 = 500;

#[derive(ValueEnum, Clone, Debug)]
pub enum Market {
    Spot,
    Futures,
}

#[derive(Debug, Clone)]
struct Candle {
    timestamp: DateTime<Utc>,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
    turnover: Option<f64>,
}

#[derive(Debug, Clone)]
struct TimeframeInfo {
    api: String,
    canonical: String,
    seconds: i64,
}

#[derive(Debug, Deserialize)]
struct KucoinResponse {
    code: String,
    data: Vec<Vec<String>>,
}

pub async fn migrate_db(db_url: &str, migrations_path: &Path) -> Result<(), String> {
    let sql = std::fs::read_to_string(migrations_path).map_err(|err| {
        format!(
            "failed to read migrations file {}: {}",
            migrations_path.display(),
            err
        )
    })?;

    let (client, connection) = tokio_postgres::connect(db_url, NoTls)
        .await
        .map_err(|err| format!("failed to connect to postgres: {err}"))?;
    tokio::spawn(async move {
        if let Err(err) = connection.await {
            eprintln!("postgres connection error: {err}");
        }
    });

    client
        .batch_execute(&sql)
        .await
        .map_err(|err| format!("failed to apply migrations: {err}"))?;
    println!("migrate complete: {}", migrations_path.display());
    Ok(())
}

pub async fn ingest_kucoin(
    db_url: &str,
    symbol: &str,
    market: Market,
    timeframe: &str,
    start: &str,
    end: Option<&str>,
    exchange: &str,
    source: &str,
    sleep_ms: u64,
    batch_size: usize,
    base_url: Option<&str>,
) -> Result<(), String> {
    let timeframe_info = normalize_timeframe(&market, timeframe)?;
    let start = parse_time_input(start)?;
    let end = match end {
        Some(end) => parse_time_input(end)?,
        None => Utc::now(),
    };

    let start_seconds = start.timestamp();
    let end_seconds = end.timestamp();

    let mut window_start = start_seconds;
    let window_span = match market {
        Market::Spot => timeframe_info.seconds * (KUCOIN_SPOT_LIMIT - 1),
        Market::Futures => timeframe_info.seconds * (KUCOIN_FUTURES_LIMIT - 1),
    };

    let http_client = Client::builder()
        .timeout(Duration::from_secs(30))
        .pool_idle_timeout(Duration::from_secs(90))
        .build()
        .map_err(|err| format!("failed to build http client: {err}"))?;

    let (mut client, connection) = tokio_postgres::connect(db_url, NoTls)
        .await
        .map_err(|err| format!("failed to connect to postgres: {err}"))?;
    tokio::spawn(async move {
        if let Err(err) = connection.await {
            eprintln!("postgres connection error: {err}");
        }
    });

    let mut total = 0u64;
    let mut window_index = 0u64;

    while window_start <= end_seconds {
        let window_end = (window_start + window_span).min(end_seconds);
        let candles = match market {
            Market::Spot => {
                fetch_kucoin_spot(
                    &http_client,
                    base_url.unwrap_or(KUCOIN_SPOT_BASE),
                    symbol,
                    &timeframe_info.api,
                    window_start,
                    window_end,
                )
                .await?
            }
            Market::Futures => {
                fetch_kucoin_futures(
                    &http_client,
                    base_url.unwrap_or(KUCOIN_FUTURES_BASE),
                    symbol,
                    timeframe_info.seconds,
                    window_start,
                    window_end,
                )
                .await?
            }
        };

        if !candles.is_empty() {
            let inserted = upsert_candles(
                &mut client,
                exchange,
                &market,
                symbol,
                &timeframe_info.canonical,
                source,
                &candles,
                batch_size,
            )
            .await?;
            total += inserted;
        }

        window_index += 1;
        println!(
            "ingest window={} start={} end={} candles={} total_upserts={}",
            window_index,
            window_start,
            window_end,
            candles.len(),
            total
        );

        if window_end >= end_seconds {
            break;
        }
        window_start = window_end + timeframe_info.seconds;
        tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
    }

    println!(
        "ingest complete: symbol={} market={:?} timeframe={} total_upserts={}",
        symbol, market, timeframe_info.canonical, total
    );
    Ok(())
}

fn normalize_timeframe(market: &Market, value: &str) -> Result<TimeframeInfo, String> {
    let trimmed = value.trim().to_lowercase();
    let (api, canonical, seconds) = match trimmed.as_str() {
        "1min" | "1m" => ("1min", "1min", 60),
        "3min" | "3m" => ("3min", "3min", 180),
        "5min" | "5m" => ("5min", "5min", 300),
        "15min" | "15m" => ("15min", "15min", 900),
        "30min" | "30m" => ("30min", "30min", 1800),
        "1hour" | "1h" => ("1hour", "1hour", 3600),
        "2hour" | "2h" => ("2hour", "2hour", 7200),
        "4hour" | "4h" => ("4hour", "4hour", 14400),
        "6hour" | "6h" => ("6hour", "6hour", 21600),
        "8hour" | "8h" => ("8hour", "8hour", 28800),
        "12hour" | "12h" => ("12hour", "12hour", 43200),
        "1day" | "1d" => ("1day", "1day", 86400),
        "1week" | "1w" => ("1week", "1week", 604800),
        "1month" => ("1month", "1month", 2592000),
        other => {
            if matches!(market, Market::Futures) {
                let seconds = other
                    .parse::<i64>()
                    .map_err(|_| format!("unsupported futures timeframe: {}", value))?;
                (other, other, seconds)
            } else {
                return Err(format!("unsupported spot timeframe: {}", value));
            }
        }
    };
    Ok(TimeframeInfo {
        api: api.to_string(),
        canonical: canonical.to_string(),
        seconds,
    })
}

async fn fetch_kucoin_spot(
    client: &Client,
    base_url: &str,
    symbol: &str,
    timeframe: &str,
    start: i64,
    end: i64,
) -> Result<Vec<Candle>, String> {
    let url = format!(
        "{}/api/v1/market/candles",
        base_url.trim_end_matches('/')
    );
    let mut attempts = 0u32;
    loop {
        attempts += 1;
        let response = client
            .get(&url)
            .query(&[
                ("symbol", symbol),
                ("type", timeframe),
                ("startAt", &start.to_string()),
                ("endAt", &end.to_string()),
            ])
            .send()
            .await
            .map_err(|err| format!("spot request failed: {err}"))?;

        if response.status().as_u16() == 429 && attempts <= 5 {
            let backoff = 500u64 * attempts as u64;
            tokio::time::sleep(Duration::from_millis(backoff)).await;
            continue;
        }

        if !response.status().is_success() {
            return Err(format!(
                "spot request failed with status {}",
                response.status()
            ));
        }

        let payload: KucoinResponse = response
            .json()
            .await
            .map_err(|err| format!("spot response parse failed: {err}"))?;
        if payload.code != "200000" {
            return Err(format!("spot response error code: {}", payload.code));
        }

        return parse_kucoin_rows(&payload.data);
    }
}

async fn fetch_kucoin_futures(
    client: &Client,
    base_url: &str,
    symbol: &str,
    granularity_seconds: i64,
    start: i64,
    end: i64,
) -> Result<Vec<Candle>, String> {
    let url = format!("{}/api/v1/kline/query", base_url.trim_end_matches('/'));
    let mut attempts = 0u32;
    let start_ms = start * 1000;
    let end_ms = end * 1000;

    loop {
        attempts += 1;
        let response = client
            .get(&url)
            .query(&[
                ("symbol", symbol),
                ("granularity", &granularity_seconds.to_string()),
                ("from", &start_ms.to_string()),
                ("to", &end_ms.to_string()),
            ])
            .send()
            .await
            .map_err(|err| format!("futures request failed: {err}"))?;

        if response.status().as_u16() == 429 && attempts <= 5 {
            let backoff = 500u64 * attempts as u64;
            tokio::time::sleep(Duration::from_millis(backoff)).await;
            continue;
        }

        if !response.status().is_success() {
            return Err(format!(
                "futures request failed with status {}",
                response.status()
            ));
        }

        let payload: KucoinResponse = response
            .json()
            .await
            .map_err(|err| format!("futures response parse failed: {err}"))?;
        if payload.code != "200000" {
            return Err(format!("futures response error code: {}", payload.code));
        }

        return parse_kucoin_rows(&payload.data);
    }
}

fn parse_kucoin_rows(rows: &[Vec<String>]) -> Result<Vec<Candle>, String> {
    let mut candles = Vec::with_capacity(rows.len());
    for row in rows {
        if row.len() < 6 {
            return Err("unexpected candle row length".to_string());
        }

        let ts = parse_epoch_value(&row[0])?;
        let timestamp = Utc
            .timestamp_opt(ts, 0)
            .single()
            .ok_or("invalid timestamp")?;

        let open = parse_f64(&row[1], "open")?;
        let close = parse_f64(&row[2], "close")?;
        let high = parse_f64(&row[3], "high")?;
        let low = parse_f64(&row[4], "low")?;
        let volume = parse_f64(&row[5], "volume")?;
        let turnover = if row.len() > 6 {
            Some(parse_f64(&row[6], "turnover")?)
        } else {
            None
        };

        candles.push(Candle {
            timestamp,
            open,
            high,
            low,
            close,
            volume,
            turnover,
        });
    }

    candles.sort_by_key(|c| c.timestamp);
    Ok(candles)
}

async fn upsert_candles(
    client: &mut PgClient,
    exchange: &str,
    market: &Market,
    symbol: &str,
    timeframe: &str,
    source: &str,
    candles: &[Candle],
    batch_size: usize,
) -> Result<u64, String> {
    if candles.is_empty() {
        return Ok(0);
    }

    let market_value = match market {
        Market::Spot => "spot",
        Market::Futures => "futures",
    };

    let statement = client
        .prepare(
            "INSERT INTO ohlcv_candles (
                exchange,
                market,
                symbol,
                timeframe,
                timestamp_utc,
                open,
                high,
                low,
                close,
                volume,
                turnover,
                source
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12
            )
            ON CONFLICT (exchange, market, symbol, timeframe, timestamp_utc)
            DO UPDATE SET
                open = EXCLUDED.open,
                high = EXCLUDED.high,
                low = EXCLUDED.low,
                close = EXCLUDED.close,
                volume = EXCLUDED.volume,
                turnover = EXCLUDED.turnover,
                source = EXCLUDED.source,
                ingested_at = NOW()",
        )
        .await
        .map_err(|err| format!("failed to prepare upsert: {err}"))?;

    let mut total = 0u64;
    let transaction = client
        .transaction()
        .await
        .map_err(|err| format!("failed to start transaction: {err}"))?;

    for chunk in candles.chunks(batch_size.max(1)) {
        for candle in chunk {
            transaction
                .execute(
                    &statement,
                    &[
                        &exchange,
                        &market_value,
                        &symbol,
                        &timeframe,
                        &candle.timestamp,
                        &candle.open,
                        &candle.high,
                        &candle.low,
                        &candle.close,
                        &candle.volume,
                        &candle.turnover,
                        &source,
                    ],
                )
                .await
                .map_err(|err| format!("upsert failed: {err}"))?;
            total += 1;
        }
    }

    transaction
        .commit()
        .await
        .map_err(|err| format!("failed to commit: {err}"))?;
    Ok(total)
}

fn parse_time_input(value: &str) -> Result<DateTime<Utc>, String> {
    if let Ok(ts) = value.parse::<i64>() {
        let seconds = if ts > 1_000_000_000_000 { ts / 1000 } else { ts };
        return Ok(Utc
            .timestamp_opt(seconds, 0)
            .single()
            .ok_or("invalid epoch")?);
    }

    DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|_| format!("unsupported timestamp format: {value}"))
}

fn parse_epoch_value(value: &str) -> Result<i64, String> {
    let ts = value
        .parse::<i64>()
        .map_err(|_| format!("invalid epoch: {value}"))?;
    if ts > 1_000_000_000_000 {
        Ok(ts / 1000)
    } else {
        Ok(ts)
    }
}

fn parse_f64(value: &str, field: &str) -> Result<f64, String> {
    value
        .parse::<f64>()
        .map_err(|_| format!("invalid {field}: {value}"))
}

#[cfg(test)]
mod tests {
    use super::normalize_timeframe;
    use super::Market;

    #[test]
    fn normalize_timeframe_maps_for_spot_and_futures() {
        let spot = normalize_timeframe(&Market::Spot, "1m").unwrap();
        assert_eq!(spot.canonical, "1min");
        let futures = normalize_timeframe(&Market::Futures, "60").unwrap();
        assert_eq!(futures.seconds, 60);
    }
}

