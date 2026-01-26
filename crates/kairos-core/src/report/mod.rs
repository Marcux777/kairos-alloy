use crate::metrics::MetricsSummary;
use crate::types::{EquityPoint, Trade};
use serde::Serialize;
use std::fs;
use std::io::Write;
use std::path::Path;

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

pub fn write_summary_json(path: &Path, summary: &MetricsSummary) -> Result<(), String> {
    let json = format!(
        "{{\n  \"bars_processed\": {},\n  \"trades\": {},\n  \"win_rate\": {},\n  \"net_profit\": {},\n  \"sharpe\": {},\n  \"max_drawdown\": {}\n}}\n",
        summary.bars_processed,
        summary.trades,
        summary.win_rate,
        summary.net_profit,
        summary.sharpe,
        summary.max_drawdown
    );
    let mut file = fs::File::create(path)
        .map_err(|err| format!("failed to create summary: {}", err))?;
    file.write_all(json.as_bytes())
        .map_err(|err| format!("failed to write summary: {}", err))
}

#[derive(Debug, Serialize)]
struct LogEntry {
    run_id: String,
    timestamp: i64,
    stage: String,
    action: String,
    details: serde_json::Value,
}

pub fn write_logs_jsonl(
    path: &Path,
    run_id: &str,
    trades: &[Trade],
    summary: &MetricsSummary,
) -> Result<(), String> {
    let mut file = fs::File::create(path)
        .map_err(|err| format!("failed to create logs: {}", err))?;

    for trade in trades {
        let entry = LogEntry {
            run_id: run_id.to_string(),
            timestamp: trade.timestamp,
            stage: "trade".to_string(),
            action: format!("{:?}", trade.side),
            details: serde_json::json!({
                "symbol": trade.symbol,
                "qty": trade.quantity,
                "price": trade.price,
                "fee": trade.fee,
                "slippage": trade.slippage,
                "strategy_id": trade.strategy_id,
                "reason": trade.reason,
            }),
        };
        let line = serde_json::to_string(&entry)
            .map_err(|err| format!("failed to serialize log entry: {}", err))?;
        file.write_all(line.as_bytes())
            .and_then(|_| file.write_all(b"\n"))
            .map_err(|err| format!("failed to write log entry: {}", err))?;
    }

    let summary_entry = LogEntry {
        run_id: run_id.to_string(),
        timestamp: 0,
        stage: "summary".to_string(),
        action: "complete".to_string(),
        details: serde_json::json!({
            "bars_processed": summary.bars_processed,
            "trades": summary.trades,
            "net_profit": summary.net_profit,
            "sharpe": summary.sharpe,
            "max_drawdown": summary.max_drawdown,
        }),
    };
    let line = serde_json::to_string(&summary_entry)
        .map_err(|err| format!("failed to serialize summary log: {}", err))?;
    file.write_all(line.as_bytes())
        .and_then(|_| file.write_all(b"\n"))
        .map_err(|err| format!("failed to write summary log: {}", err))?;

    Ok(())
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
        write_summary_json(dir.join("summary.json").as_path(), &summary).expect("summary");
        write_logs_jsonl(dir.join("logs.jsonl").as_path(), "run1", &trades, &summary)
            .expect("logs");

        assert!(dir.join("trades.csv").exists());
        assert!(dir.join("equity.csv").exists());
        assert!(dir.join("summary.json").exists());
        assert!(dir.join("logs.jsonl").exists());
    }
}
