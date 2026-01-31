use kairos_domain::entities::metrics::MetricsSummary;
use kairos_domain::services::audit::AuditEvent;
use kairos_domain::value_objects::equity_point::EquityPoint;
use kairos_domain::value_objects::side::Side;
use kairos_domain::value_objects::trade::Trade;
use serde::Serialize;
use std::fs;
use std::io::Write;
use std::path::Path;

pub fn write_audit_jsonl(path: &Path, events: &[AuditEvent]) -> Result<(), String> {
    let mut file =
        fs::File::create(path).map_err(|err| format!("failed to create logs: {}", err))?;
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
    let mut wtr = csv::Writer::from_path(path)
        .map_err(|err| format!("failed to create trades csv {}: {}", path.display(), err))?;
    wtr.write_record([
        "timestamp_utc",
        "symbol",
        "side",
        "qty",
        "price",
        "fee",
        "slippage",
        "strategy_id",
        "reason",
    ])
    .map_err(|err| format!("failed to write trades csv header: {}", err))?;

    for trade in trades {
        let side = match trade.side {
            Side::Buy => "BUY",
            Side::Sell => "SELL",
        };
        wtr.write_record([
            trade.timestamp.to_string(),
            trade.symbol.clone(),
            side.to_string(),
            trade.quantity.to_string(),
            trade.price.to_string(),
            trade.fee.to_string(),
            trade.slippage.to_string(),
            trade.strategy_id.clone(),
            trade.reason.clone(),
        ])
        .map_err(|err| format!("failed to write trades row: {}", err))?;
    }

    wtr.flush()
        .map_err(|err| format!("failed to flush trades csv: {}", err))
}

pub fn write_equity_csv(path: &Path, points: &[EquityPoint]) -> Result<(), String> {
    let mut wtr = csv::Writer::from_path(path)
        .map_err(|err| format!("failed to create equity csv {}: {}", path.display(), err))?;
    wtr.write_record([
        "timestamp_utc",
        "equity",
        "cash",
        "position_qty",
        "unrealized_pnl",
        "realized_pnl",
    ])
    .map_err(|err| format!("failed to write equity csv header: {}", err))?;

    for point in points {
        wtr.write_record([
            point.timestamp.to_string(),
            point.equity.to_string(),
            point.cash.to_string(),
            point.position_qty.to_string(),
            point.unrealized_pnl.to_string(),
            point.realized_pnl.to_string(),
        ])
        .map_err(|err| format!("failed to write equity row: {}", err))?;
    }

    wtr.flush()
        .map_err(|err| format!("failed to flush equity csv: {}", err))
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
    let mut file =
        fs::File::create(path).map_err(|err| format!("failed to create summary: {}", err))?;
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
        None => (
            "unknown",
            "unknown",
            "unknown",
            "unknown".to_string(),
            "unknown".to_string(),
        ),
    };

    let html = format!(
        r#"<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8"/>
  <title>Kairos Alloy Summary</title>
  <style>
    body {{ font-family: ui-sans-serif, system-ui; padding: 24px; }}
    table {{ border-collapse: collapse; width: 520px; }}
    th, td {{ border: 1px solid #ddd; padding: 8px; }}
    th {{ background: #f6f6f6; text-align: left; }}
    code {{ background: #f2f2f2; padding: 2px 6px; border-radius: 4px; }}
  </style>
</head>
<body>
  <h1>Kairos Alloy Summary</h1>
  <p><strong>run_id:</strong> <code>{run_id}</code></p>
  <p><strong>symbol:</strong> <code>{symbol}</code></p>
  <p><strong>timeframe:</strong> <code>{timeframe}</code></p>
  <p><strong>start:</strong> <code>{start}</code></p>
  <p><strong>end:</strong> <code>{end}</code></p>
  <h2>Metrics</h2>
  <table>
    <tr><th>bars_processed</th><td>{}</td></tr>
    <tr><th>trades</th><td>{}</td></tr>
    <tr><th>win_rate</th><td>{:.4}</td></tr>
    <tr><th>net_profit</th><td>{:.4}</td></tr>
    <tr><th>sharpe</th><td>{:.4}</td></tr>
    <tr><th>max_drawdown</th><td>{:.4}</td></tr>
  </table>
</body>
</html>"#,
        summary.bars_processed,
        summary.trades,
        summary.win_rate,
        summary.net_profit,
        summary.sharpe,
        summary.max_drawdown,
    );

    let mut file =
        fs::File::create(path).map_err(|err| format!("failed to create html: {}", err))?;
    file.write_all(html.as_bytes())
        .map_err(|err| format!("failed to write html: {}", err))
}

pub fn write_dashboard_html(
    path: &Path,
    summary: &MetricsSummary,
    meta: Option<&SummaryMeta>,
    trades: &[Trade],
    equity: &[EquityPoint],
) -> Result<(), String> {
    let (run_id, symbol, timeframe, start, end) = match meta {
        Some(meta) => (
            meta.run_id.as_str(),
            meta.symbol.as_str(),
            meta.timeframe.as_str(),
            meta.start.to_string(),
            meta.end.to_string(),
        ),
        None => (
            "unknown",
            "unknown",
            "unknown",
            "unknown".to_string(),
            "unknown".to_string(),
        ),
    };

    let equity_json = serde_json::to_string(equity)
        .map_err(|err| format!("failed to serialize equity: {err}"))?;
    let trades_json = serde_json::to_string(trades)
        .map_err(|err| format!("failed to serialize trades: {err}"))?;

    let html = format!(
        r#"<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8"/>
  <title>Kairos Alloy Dashboard</title>
  <style>
    body {{ font-family: ui-sans-serif, system-ui; padding: 24px; }}
    code {{ background: #f2f2f2; padding: 2px 6px; border-radius: 4px; }}
    .grid {{ display: grid; grid-template-columns: 1fr 1fr; gap: 16px; align-items: start; }}
    .card {{ border: 1px solid #ddd; border-radius: 10px; padding: 16px; background: #fff; }}
    canvas {{ width: 100%; height: 260px; border: 1px solid #eee; border-radius: 8px; }}
    table {{ border-collapse: collapse; width: 100%; }}
    th, td {{ border: 1px solid #eee; padding: 8px; font-size: 12px; }}
    th {{ background: #fafafa; text-align: left; }}
    .muted {{ color: #666; }}
  </style>
</head>
<body>
  <h1>Kairos Alloy Dashboard</h1>
  <p class="muted">
    run_id: <code>{run_id}</code> · symbol: <code>{symbol}</code> · timeframe: <code>{timeframe}</code>
    · start: <code>{start}</code> · end: <code>{end}</code>
  </p>

  <div class="grid">
    <div class="card">
      <h2>Equity</h2>
      <canvas id="equity"></canvas>
      <p class="muted">bars_processed={bars_processed} trades={trades} net_profit={net_profit:.4} sharpe={sharpe:.4} max_drawdown={max_drawdown:.4}</p>
    </div>
    <div class="card">
      <h2>Trades</h2>
      <table id="trades_table">
        <thead>
          <tr>
            <th>ts</th>
            <th>side</th>
            <th>qty</th>
            <th>price</th>
            <th>fee</th>
            <th>slippage</th>
            <th>strategy</th>
          </tr>
        </thead>
        <tbody></tbody>
      </table>
    </div>
  </div>

  <script>
    const equity = {equity_json};
    const trades = {trades_json};

    function drawLine(canvas, points) {{
      const ctx = canvas.getContext('2d');
      const w = canvas.width = canvas.clientWidth * window.devicePixelRatio;
      const h = canvas.height = canvas.clientHeight * window.devicePixelRatio;
      ctx.clearRect(0, 0, w, h);

      if (!points || points.length < 2) {{
        ctx.fillStyle = '#666';
        ctx.fillText('no equity data', 10, 20);
        return;
      }}

      const values = points.map(p => p.equity);
      const minV = Math.min(...values);
      const maxV = Math.max(...values);
      const pad = 20 * window.devicePixelRatio;
      const x0 = pad, y0 = pad, x1 = w - pad, y1 = h - pad;

      function x(i) {{
        return x0 + (i / (points.length - 1)) * (x1 - x0);
      }}
      function y(v) {{
        if (maxV === minV) return (y0 + y1) / 2;
        const t = (v - minV) / (maxV - minV);
        return y1 - t * (y1 - y0);
      }}

      ctx.strokeStyle = '#2b6cb0';
      ctx.lineWidth = 2 * window.devicePixelRatio;
      ctx.beginPath();
      ctx.moveTo(x(0), y(points[0].equity));
      for (let i = 1; i < points.length; i++) {{
        ctx.lineTo(x(i), y(points[i].equity));
      }}
      ctx.stroke();
    }}

    function renderTrades(tableId, trades) {{
      const tbody = document.querySelector(`#${{tableId}} tbody`);
      tbody.innerHTML = '';
      for (const t of trades) {{
        const tr = document.createElement('tr');
        tr.innerHTML = `
          <td>${{t.timestamp}}</td>
          <td>${{t.side}}</td>
          <td>${{t.quantity}}</td>
          <td>${{t.price}}</td>
          <td>${{t.fee}}</td>
          <td>${{t.slippage}}</td>
          <td>${{t.strategy_id}}</td>
        `;
        tbody.appendChild(tr);
      }}
    }}

    drawLine(document.getElementById('equity'), equity);
    renderTrades('trades_table', trades);
    window.addEventListener('resize', () => drawLine(document.getElementById('equity'), equity));
  </script>
</body>
</html>"#,
        bars_processed = summary.bars_processed,
        trades = summary.trades,
        net_profit = summary.net_profit,
        sharpe = summary.sharpe,
        max_drawdown = summary.max_drawdown,
    );

    let mut file =
        fs::File::create(path).map_err(|err| format!("failed to create html: {}", err))?;
    file.write_all(html.as_bytes())
        .map_err(|err| format!("failed to write html: {}", err))
}

#[derive(Debug, Clone, serde::Deserialize)]
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

pub fn read_trades_csv(path: &Path) -> Result<Vec<Trade>, String> {
    let mut rdr = csv::Reader::from_path(path)
        .map_err(|err| format!("failed to open trades csv {}: {}", path.display(), err))?;
    let mut trades = Vec::new();
    for result in rdr.deserialize::<TradeRecord>() {
        let record = result.map_err(|err| format!("failed to parse trade record: {}", err))?;
        let side = match record.side.to_uppercase().as_str() {
            "BUY" => Side::Buy,
            "SELL" => Side::Sell,
            other => return Err(format!("invalid side '{}'", other)),
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

#[derive(Debug, Clone, serde::Deserialize)]
struct EquityRecord {
    timestamp_utc: i64,
    equity: f64,
    cash: f64,
    position_qty: f64,
    unrealized_pnl: f64,
    realized_pnl: f64,
}

pub fn read_equity_csv(path: &Path) -> Result<Vec<EquityPoint>, String> {
    let mut rdr = csv::Reader::from_path(path)
        .map_err(|err| format!("failed to open equity csv {}: {}", path.display(), err))?;
    let mut points = Vec::new();
    for result in rdr.deserialize::<EquityRecord>() {
        let record = result.map_err(|err| format!("failed to parse equity record: {}", err))?;
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
    kairos_domain::entities::metrics::recompute_summary(trades, equity)
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
    use super::{
        read_trades_csv, write_equity_csv, write_logs_jsonl, write_summary_json, write_trades_csv,
    };
    use kairos_domain::entities::metrics::MetricsSummary;
    use kairos_domain::value_objects::equity_point::EquityPoint;
    use kairos_domain::value_objects::side::Side;
    use kairos_domain::value_objects::trade::Trade;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_tmp_dir(prefix: &str) -> PathBuf {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("kairos_{prefix}_{}_{}", std::process::id(), now))
    }

    #[test]
    fn writes_report_files() {
        let dir = unique_tmp_dir("report_test");
        let _ = fs::create_dir_all(&dir);

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

    #[test]
    fn trades_csv_roundtrips_with_escaping() {
        let dir = unique_tmp_dir("report_trades_roundtrip");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("trades.csv");

        let trades = vec![Trade {
            timestamp: 1704067200,
            symbol: "BTCUSD".to_string(),
            side: Side::Buy,
            quantity: 1.0,
            price: 100.0,
            fee: 0.1,
            slippage: 0.2,
            strategy_id: "strat,a\"b".to_string(),
            reason: "line1\nline2,comma".to_string(),
        }];

        write_trades_csv(path.as_path(), &trades).expect("write trades");
        let parsed = read_trades_csv(path.as_path()).expect("read trades");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].strategy_id, trades[0].strategy_id);
        assert_eq!(parsed[0].reason, trades[0].reason);
        assert_eq!(parsed[0].symbol, trades[0].symbol);
        assert_eq!(parsed[0].timestamp, trades[0].timestamp);
    }
}
