use crate::metrics::MetricsSummary;
use crate::metrics::{MetricsConfig, MetricsState};
use crate::types::{EquityPoint, Side, Trade};
use serde::Serialize;
use std::fs;
use std::io::Write;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct AuditEvent {
    pub run_id: String,
    pub timestamp: i64,
    pub stage: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub details: serde_json::Value,
}

pub fn write_audit_jsonl(path: &Path, events: &[AuditEvent]) -> Result<(), String> {
    let mut file = fs::File::create(path)
        .map_err(|err| format!("failed to create logs: {}", err))?;
    for event in events {
        let line = serde_json::to_string(event)
            .map_err(|err| format!("failed to serialize audit event: {}", err))?;
        file.write_all(line.as_bytes())
            .and_then(|_| file.write_all(b"\n"))
            .map_err(|err| format!("failed to write audit event: {}", err))?;
    }
    Ok(())
}

pub fn write_trades_csv(path: &Path, trades: &[Trade]) -> Result<(), String> {
    let mut output = String::from("timestamp_utc,symbol,side,qty,price,fee,slippage,strategy_id,reason\n");
    for trade in trades {
        output.push_str(&format!(
            "{},{},{:?},{},{},{},{},{},{}\n",
            trade.timestamp,
            trade.symbol,
            trade.side,
            trade.quantity,
            trade.price,
            trade.fee,
            trade.slippage,
            trade.strategy_id,
            trade.reason
        ));
    }
    fs::write(path, output).map_err(|err| format!("failed to write trades: {}", err))
}

pub fn write_equity_csv(path: &Path, points: &[EquityPoint]) -> Result<(), String> {
    let mut output = String::from("timestamp_utc,equity,cash,position_qty,unrealized_pnl,realized_pnl\n");
    for point in points {
        output.push_str(&format!(
            "{},{},{},{},{},{}\n",
            point.timestamp,
            point.equity,
            point.cash,
            point.position_qty,
            point.unrealized_pnl,
            point.realized_pnl
        ));
    }
    fs::write(path, output).map_err(|err| format!("failed to write equity: {}", err))
}

#[derive(Debug, Serialize)]
pub struct SummaryMeta {
    pub run_id: String,
    pub symbol: String,
    pub timeframe: String,
    pub start: i64,
    pub end: i64,
}

pub fn write_summary_json(
    path: &Path,
    summary: &MetricsSummary,
    meta: Option<&SummaryMeta>,
    config_snapshot: Option<&serde_json::Value>,
) -> Result<(), String> {
    let meta_json = meta.map(|meta| {
        serde_json::json!({
            "run_id": meta.run_id,
            "symbol": meta.symbol,
            "timeframe": meta.timeframe,
            "start": meta.start,
            "end": meta.end,
        })
    });

    let json = serde_json::json!({
        "meta": meta_json,
        "config_snapshot": config_snapshot,
        "bars_processed": summary.bars_processed,
        "trades": summary.trades,
        "win_rate": summary.win_rate,
        "net_profit": summary.net_profit,
        "sharpe": summary.sharpe,
        "max_drawdown": summary.max_drawdown,
    });
    let json = serde_json::to_string_pretty(&json)
        .map_err(|err| format!("failed to serialize summary: {}", err))?;
    let mut file = fs::File::create(path)
        .map_err(|err| format!("failed to create summary: {}", err))?;
    file.write_all(json.as_bytes())
        .map_err(|err| format!("failed to write summary: {}", err))
}

pub fn write_summary_html(
    path: &Path,
    summary: &MetricsSummary,
    meta: Option<&SummaryMeta>,
) -> Result<(), String> {
    let (run_id, symbol, timeframe, start, end) = match meta {
        Some(meta) => (
            meta.run_id.as_str(),
            meta.symbol.as_str(),
            meta.timeframe.as_str(),
            meta.start.to_string(),
            meta.end.to_string(),
        ),
        None => ("unknown", "unknown", "unknown", "n/a".to_string(), "n/a".to_string()),
    };
    let html = format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n  <meta charset=\"utf-8\"/>\n  <title>Kairos Alloy Summary</title>\n  <style>\n    body {{ font-family: Arial, sans-serif; margin: 24px; }}\n    table {{ border-collapse: collapse; }}\n    td, th {{ border: 1px solid #ddd; padding: 8px; }}\n    th {{ text-align: left; }}\n  </style>\n</head>\n<body>\n  <h1>Kairos Alloy Summary</h1>\n  <h2>Run</h2>\n  <table>\n    <tr><th>run_id</th><td>{}</td></tr>\n    <tr><th>symbol</th><td>{}</td></tr>\n    <tr><th>timeframe</th><td>{}</td></tr>\n    <tr><th>start</th><td>{}</td></tr>\n    <tr><th>end</th><td>{}</td></tr>\n  </table>\n  <h2>Metrics</h2>\n  <table>\n    <tr><th>bars_processed</th><td>{}</td></tr>\n    <tr><th>trades</th><td>{}</td></tr>\n    <tr><th>win_rate</th><td>{:.4}</td></tr>\n    <tr><th>net_profit</th><td>{:.4}</td></tr>\n    <tr><th>sharpe</th><td>{:.4}</td></tr>\n    <tr><th>max_drawdown</th><td>{:.4}</td></tr>\n  </table>\n</body>\n</html>\n",
        run_id,
        symbol,
        timeframe,
        start,
        end,
        summary.bars_processed,
        summary.trades,
        summary.win_rate,
        summary.net_profit,
        summary.sharpe,
        summary.max_drawdown
    );
    fs::write(path, html).map_err(|err| format!("failed to write summary html: {}", err))
}

#[derive(Debug, serde::Deserialize)]
struct TradeRecord {
    timestamp_utc: i64,
    symbol: String,
    side: String,
    qty: f64,
    price: f64,
    fee: f64,
    slippage: f64,
    strategy_id: String,
    reason: String,
}

#[derive(Debug, serde::Deserialize)]
struct EquityRecord {
    timestamp_utc: i64,
    equity: f64,
    cash: f64,
    position_qty: f64,
    unrealized_pnl: f64,
    realized_pnl: f64,
}

pub fn read_trades_csv(path: &Path) -> Result<Vec<Trade>, String> {
    let file = fs::File::open(path)
        .map_err(|err| format!("failed to open trades csv {}: {}", path.display(), err))?;
    let mut reader = csv::Reader::from_reader(file);
    let mut trades = Vec::new();
    for result in reader.deserialize::<TradeRecord>() {
        let record = result.map_err(|err| format!("failed to parse trades row: {}", err))?;
        let side = match record.side.to_uppercase().as_str() {
            "BUY" => Side::Buy,
            "SELL" => Side::Sell,
            _ => return Err(format!("invalid side value: {}", record.side)),
        };
        trades.push(Trade {
            timestamp: record.timestamp_utc,
            symbol: record.symbol,
            side,
            quantity: record.qty,
            price: record.price,
            fee: record.fee,
            slippage: record.slippage,
            strategy_id: record.strategy_id,
            reason: record.reason,
        });
    }
    Ok(trades)
}

pub fn read_equity_csv(path: &Path) -> Result<Vec<EquityPoint>, String> {
    let file = fs::File::open(path)
        .map_err(|err| format!("failed to open equity csv {}: {}", path.display(), err))?;
    let mut reader = csv::Reader::from_reader(file);
    let mut points = Vec::new();
    for result in reader.deserialize::<EquityRecord>() {
        let record = result.map_err(|err| format!("failed to parse equity row: {}", err))?;
        points.push(EquityPoint {
            timestamp: record.timestamp_utc,
            equity: record.equity,
            cash: record.cash,
            position_qty: record.position_qty,
            unrealized_pnl: record.unrealized_pnl,
            realized_pnl: record.realized_pnl,
        });
    }
    Ok(points)
}

pub fn recompute_summary(trades: &[Trade], equity: &[EquityPoint]) -> MetricsSummary {
    let mut state = MetricsState::new(MetricsConfig::default());
    for point in equity {
        state.record_equity(point.clone());
    }
    for trade in trades {
        state.record_trade(trade.clone());
    }
    state.summary()
}

pub fn write_logs_jsonl(
    path: &Path,
    run_id: &str,
    trades: &[Trade],
    summary: &MetricsSummary,
) -> Result<(), String> {
    let mut events = Vec::with_capacity(trades.len() + 1);

    for trade in trades {
        events.push(AuditEvent {
            run_id: run_id.to_string(),
            timestamp: trade.timestamp,
            stage: "trade".to_string(),
            symbol: Some(trade.symbol.clone()),
            action: format!("{:?}", trade.side),
            error: None,
            details: serde_json::json!({
                "qty": trade.quantity,
                "price": trade.price,
                "fee": trade.fee,
                "slippage": trade.slippage,
                "strategy_id": trade.strategy_id,
                "reason": trade.reason,
            }),
        });
    }

    events.push(AuditEvent {
        run_id: run_id.to_string(),
        timestamp: 0,
        stage: "summary".to_string(),
        symbol: None,
        action: "complete".to_string(),
        error: None,
        details: serde_json::json!({
            "bars_processed": summary.bars_processed,
            "trades": summary.trades,
            "net_profit": summary.net_profit,
            "sharpe": summary.sharpe,
            "max_drawdown": summary.max_drawdown,
        }),
    });

    write_audit_jsonl(path, &events)
}

#[cfg(test)]
mod tests {
    use super::{write_equity_csv, write_logs_jsonl, write_summary_json, write_trades_csv};
    use crate::metrics::MetricsSummary;
    use crate::types::{EquityPoint, Trade, Side};
    use std::fs;
    use std::path::Path;

    #[test]
    fn writes_report_files() {
        let dir = Path::new("/tmp/kairos_report_test");
        let _ = fs::create_dir_all(dir);

        let trades = vec![Trade {
            timestamp: 1,
            symbol: "BTCUSD".to_string(),
            side: Side::Buy,
            quantity: 1.0,
            price: 100.0,
            fee: 0.1,
            slippage: 0.0,
            strategy_id: "test".to_string(),
            reason: "unit".to_string(),
        }];
        let equity = vec![EquityPoint {
            timestamp: 1,
            equity: 100.0,
            cash: 100.0,
            position_qty: 0.0,
            unrealized_pnl: 0.0,
            realized_pnl: 0.0,
        }];
        let summary = MetricsSummary {
            bars_processed: 1,
            trades: 1,
            win_rate: 0.0,
            net_profit: 0.0,
            sharpe: 0.0,
            max_drawdown: 0.0,
        };

        write_trades_csv(dir.join("trades.csv").as_path(), &trades).expect("trades");
        write_equity_csv(dir.join("equity.csv").as_path(), &equity).expect("equity");
        write_summary_json(dir.join("summary.json").as_path(), &summary, None, None)
            .expect("summary");
        write_logs_jsonl(dir.join("logs.jsonl").as_path(), "run1", &trades, &summary)
            .expect("logs");

        assert!(dir.join("trades.csv").exists());
        assert!(dir.join("equity.csv").exists());
        assert!(dir.join("summary.json").exists());
        assert!(dir.join("logs.jsonl").exists());
    }
}
