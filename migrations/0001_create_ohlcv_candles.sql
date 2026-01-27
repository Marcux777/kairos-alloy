CREATE TABLE IF NOT EXISTS ohlcv_candles (
    exchange TEXT NOT NULL,
    market TEXT NOT NULL,
    symbol TEXT NOT NULL,
    timeframe TEXT NOT NULL,
    timestamp_utc TIMESTAMPTZ NOT NULL,
    open DOUBLE PRECISION NOT NULL,
    high DOUBLE PRECISION NOT NULL,
    low DOUBLE PRECISION NOT NULL,
    close DOUBLE PRECISION NOT NULL,
    volume DOUBLE PRECISION NOT NULL,
    turnover DOUBLE PRECISION,
    source TEXT NOT NULL,
    ingested_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (exchange, market, symbol, timeframe, timestamp_utc)
);

CREATE INDEX IF NOT EXISTS ohlcv_candles_symbol_timeframe_ts_idx
    ON ohlcv_candles (symbol, timeframe, timestamp_utc);
