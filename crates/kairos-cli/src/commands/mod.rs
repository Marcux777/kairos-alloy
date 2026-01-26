use crate::config::{load_config, Config};
use kairos_core::backtest::{BacktestResults, BacktestRunner};
use kairos_core::data::{ohlcv, sentiment};
use kairos_core::market_data::{MarketDataSource, VecBarSource};
use kairos_core::report::{write_equity_csv, write_logs_jsonl, write_summary_json, write_trades_csv};
use kairos_core::risk::RiskLimits;
use kairos_core::strategy::{AgentStrategy, BuyAndHold, HoldStrategy, StrategyKind};
use kairos_core::types::ActionType;
use kairos_core::{agents::AgentClient, engine_name, features};
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

pub enum Command {
    Backtest { config: PathBuf, out: Option<PathBuf> },
    Paper { config: PathBuf, out: Option<PathBuf> },
    Validate { config: PathBuf },
    Report { input: PathBuf },
}

pub fn run(command: Command) -> Result<(), String> {
    match command {
        Command::Backtest { config, out } => run_backtest(config, out),
        Command::Paper { config, out } => run_paper(config, out),
        Command::Validate { config } => run_validate(config),
        Command::Report { input } => run_report(input),
    }
}

fn run_validate(config_path: PathBuf) -> Result<(), String> {
    let config = load_config(&config_path)?;
    print_config_summary("validate", &config, None);

    let (_, ohlcv_report) = ohlcv::load_csv(PathBuf::from(&config.paths.ohlcv_csv).as_path())?;
    println!(
        "ohlcv report: duplicates={}, gaps={}, out_of_order={}",
        ohlcv_report.duplicates, ohlcv_report.gaps, ohlcv_report.out_of_order
    );

    if let Some(path) = &config.paths.sentiment_path {
        let path_buf = PathBuf::from(path);
        let ext = path_buf
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        let (_, report) = if ext == "json" {
            sentiment::load_json(path_buf.as_path())?
        } else {
            sentiment::load_csv(path_buf.as_path())?
        };
        println!(
            "sentiment report: duplicates={}, out_of_order={}",
            report.duplicates, report.out_of_order
        );
    }

    Ok(())
}

fn run_report(input: PathBuf) -> Result<(), String> {
    let summary_path = input.join("summary.json");
    if !summary_path.exists() {
        return Err(format!(
            "summary.json not found in {}",
            input.display()
        ));
    }
    let summary = std::fs::read_to_string(summary_path)
        .map_err(|err| format!("failed to read summary: {}", err))?;
    println!("{} cli: report summary\n{}", engine_name(), summary);
    Ok(())
}

fn print_config_summary(command: &str, config: &Config, out: Option<&PathBuf>) {
    println!(
        "{} cli: {} (run_id={}, symbol={}, timeframe={}, initial_capital={})",
        engine_name(),
        command,
        config.run.run_id,
        config.run.symbol,
        config.run.timeframe,
        config.run.initial_capital
    );
    println!(
        "data: ohlcv={}, sentiment={}, out_dir={}",
        config.paths.ohlcv_csv,
        config
            .paths
            .sentiment_path
            .as_deref()
            .unwrap_or("none"),
        config.paths.out_dir
    );
    println!(
        "costs: fee_bps={}, slippage_bps={}",
        config.costs.fee_bps, config.costs.slippage_bps
    );
    println!(
        "risk: max_position_qty={}, max_drawdown_pct={}, max_exposure_pct={}",
        config.risk.max_position_qty,
        config.risk.max_drawdown_pct,
        config.risk.max_exposure_pct
    );
    println!(
        "features: return_mode={}, sma_windows={:?}, rsi_enabled={}, sentiment_lag={}",
        config.features.return_mode,
        config.features.sma_windows,
        config.features.rsi_enabled,
        config.features.sentiment_lag
    );
    println!(
        "agent: mode={}, url={}, timeout_ms={}, retries={}, fallback_action={}, api_version={}, feature_version={}",
        config.agent.mode,
        config.agent.url,
        config.agent.timeout_ms,
        config.agent.retries,
        config.agent.fallback_action,
        config.agent.api_version,
        config.agent.feature_version
    );
    if let Some(out_dir) = out {
        println!("output dir: {}", out_dir.display());
    }
}

fn run_backtest(config_path: PathBuf, out: Option<PathBuf>) -> Result<(), String> {
    let config = load_config(&config_path)?;
    print_config_summary("backtest", &config, out.as_ref());

    let (mut bars, data_report) =
        ohlcv::load_csv(PathBuf::from(&config.paths.ohlcv_csv).as_path())?;
    for bar in &mut bars {
        bar.symbol = config.run.symbol.clone();
    }

    let sentiment_points = if let Some(path) = &config.paths.sentiment_path {
        let path_buf = PathBuf::from(path);
        let ext = path_buf
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        let (points, report) = if ext == "json" {
            sentiment::load_json(path_buf.as_path())?
        } else {
            sentiment::load_csv(path_buf.as_path())?
        };
        if report.duplicates > 0 || report.out_of_order > 0 {
            println!(
                "sentiment report: duplicates={}, out_of_order={}",
                report.duplicates, report.out_of_order
            );
        }
        Some(points)
    } else {
        None
    };

    if data_report.duplicates > 0 || data_report.gaps > 0 || data_report.out_of_order > 0 {
        println!(
            "ohlcv report: duplicates={}, gaps={}, out_of_order={}",
            data_report.duplicates, data_report.gaps, data_report.out_of_order
        );
    }

    let sentiment_lag = parse_duration_like(&config.features.sentiment_lag)?;
    let bar_timestamps: Vec<i64> = bars.iter().map(|bar| bar.timestamp).collect();
    let aligned_sentiment = sentiment_points
        .as_ref()
        .map(|points| sentiment::align_with_bars(&bar_timestamps, points, sentiment_lag))
        .unwrap_or_else(|| vec![None; bars.len()]);

    let feature_config = features::FeatureConfig {
        return_mode: match config.features.return_mode.as_str() {
            "log" => features::ReturnMode::Log,
            _ => features::ReturnMode::Pct,
        },
        sma_windows: config.features.sma_windows.iter().map(|w| *w as usize).collect(),
        rsi_enabled: config.features.rsi_enabled,
    };
    let builder = features::FeatureBuilder::new(feature_config);

    let risk_limits = RiskLimits {
        max_position_qty: config.risk.max_position_qty,
        max_drawdown_pct: config.risk.max_drawdown_pct,
        max_exposure_pct: config.risk.max_exposure_pct,
    };

    let strategy = match config.agent.mode.as_str() {
        "remote" => {
            let fallback_action = parse_action_type(&config.agent.fallback_action)?;
            let agent = AgentClient::new(
                config.agent.url.clone(),
                config.agent.timeout_ms,
                config.agent.api_version.clone(),
                config.agent.feature_version.clone(),
                config.agent.retries,
                fallback_action,
            );
            StrategyKind::Agent(AgentStrategy::new(
                config.run.run_id.clone(),
                config.run.symbol.clone(),
                config.run.timeframe.clone(),
                config.agent.feature_version.clone(),
                agent,
                builder,
                aligned_sentiment,
            ))
        }
        "baseline" => StrategyKind::BuyAndHold(BuyAndHold::new(1.0)),
        _ => StrategyKind::Hold(HoldStrategy),
    };

    let data = VecBarSource::new(bars);
    let mut runner = BacktestRunner::new(
        strategy,
        data,
        risk_limits,
        config.run.initial_capital,
        config.costs.fee_bps,
        config.costs.slippage_bps,
        config.run.symbol.clone(),
    );
    let results = runner.run();

    write_outputs(&config, out, results, &config_path)?;
    Ok(())
}

fn write_outputs(
    config: &Config,
    out: Option<PathBuf>,
    results: BacktestResults,
    config_path: &PathBuf,
) -> Result<(), String> {
    let base_dir = out.unwrap_or_else(|| PathBuf::from(&config.paths.out_dir));
    let run_dir = base_dir.join(&config.run.run_id);
    std::fs::create_dir_all(&run_dir)
        .map_err(|err| format!("failed to create run dir {}: {}", run_dir.display(), err))?;

    write_trades_csv(run_dir.join("trades.csv").as_path(), &results.trades)?;
    write_equity_csv(run_dir.join("equity.csv").as_path(), &results.equity)?;
    write_summary_json(run_dir.join("summary.json").as_path(), &results.summary)?;
    write_logs_jsonl(
        run_dir.join("logs.jsonl").as_path(),
        &config.run.run_id,
        &results.trades,
        &results.summary,
    )?;
    std::fs::copy(config_path, run_dir.join("config_snapshot.toml")).map_err(|err| {
        format!(
            "failed to copy config to snapshot {}: {}",
            run_dir.display(),
            err
        )
    })?;

    println!("run output: {}", run_dir.display());
    Ok(())
}

fn parse_duration_like(value: &str) -> Result<i64, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("empty sentiment_lag".to_string());
    }

    let (number_part, unit) = trimmed.split_at(trimmed.len() - 1);
    let multiplier = match unit {
        "s" | "S" => 1,
        "m" | "M" => 60,
        "h" | "H" => 3600,
        _ => return Err(format!("unsupported sentiment_lag unit: {}", unit)),
    };
    let number: i64 = number_part
        .parse()
        .map_err(|_| format!("invalid sentiment_lag: {}", value))?;
    Ok(number * multiplier)
}

fn parse_action_type(value: &str) -> Result<ActionType, String> {
    match value.to_uppercase().as_str() {
        "BUY" => Ok(ActionType::Buy),
        "SELL" => Ok(ActionType::Sell),
        "HOLD" => Ok(ActionType::Hold),
        _ => Err(format!("unsupported action type: {}", value)),
    }
}

fn run_paper(config_path: PathBuf, out: Option<PathBuf>) -> Result<(), String> {
    let config = load_config(&config_path)?;
    print_config_summary("paper", &config, out.as_ref());

    let (mut bars, data_report) = ohlcv::load_csv(PathBuf::from(&config.paths.ohlcv_csv).as_path())?;
    for bar in &mut bars {
        bar.symbol = config.run.symbol.clone();
    }

    if data_report.duplicates > 0 || data_report.gaps > 0 || data_report.out_of_order > 0 {
        println!(
            "ohlcv report: duplicates={}, gaps={}, out_of_order={}",
            data_report.duplicates, data_report.gaps, data_report.out_of_order
        );
    }

    let sentiment_points = if let Some(path) = &config.paths.sentiment_path {
        let path_buf = PathBuf::from(path);
        let ext = path_buf
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        let (points, report) = if ext == "json" {
            sentiment::load_json(path_buf.as_path())?
        } else {
            sentiment::load_csv(path_buf.as_path())?
        };
        if report.duplicates > 0 || report.out_of_order > 0 {
            println!(
                "sentiment report: duplicates={}, out_of_order={}",
                report.duplicates, report.out_of_order
            );
        }
        Some(points)
    } else {
        None
    };

    let sentiment_lag = parse_duration_like(&config.features.sentiment_lag)?;
    let bar_timestamps: Vec<i64> = bars.iter().map(|bar| bar.timestamp).collect();
    let aligned_sentiment = sentiment_points
        .as_ref()
        .map(|points| sentiment::align_with_bars(&bar_timestamps, points, sentiment_lag))
        .unwrap_or_else(|| vec![None; bars.len()]);

    let feature_config = features::FeatureConfig {
        return_mode: match config.features.return_mode.as_str() {
            "log" => features::ReturnMode::Log,
            _ => features::ReturnMode::Pct,
        },
        sma_windows: config.features.sma_windows.iter().map(|w| *w as usize).collect(),
        rsi_enabled: config.features.rsi_enabled,
    };
    let builder = features::FeatureBuilder::new(feature_config);

    let strategy = match config.agent.mode.as_str() {
        "remote" => {
            let fallback_action = parse_action_type(&config.agent.fallback_action)?;
            let agent = AgentClient::new(
                config.agent.url.clone(),
                config.agent.timeout_ms,
                config.agent.api_version.clone(),
                config.agent.feature_version.clone(),
                config.agent.retries,
                fallback_action,
            );
            StrategyKind::Agent(AgentStrategy::new(
                config.run.run_id.clone(),
                config.run.symbol.clone(),
                config.run.timeframe.clone(),
                config.agent.feature_version.clone(),
                agent,
                builder,
                aligned_sentiment,
            ))
        }
        "baseline" => StrategyKind::BuyAndHold(BuyAndHold::new(1.0)),
        _ => StrategyKind::Hold(HoldStrategy),
    };

    let risk_limits = RiskLimits {
        max_position_qty: config.risk.max_position_qty,
        max_drawdown_pct: config.risk.max_drawdown_pct,
        max_exposure_pct: config.risk.max_exposure_pct,
    };

    let timeframe_seconds = parse_duration_like(&config.run.timeframe)?;
    let data = RealtimeBarSource::new(bars, timeframe_seconds);
    let mut runner = BacktestRunner::new(
        strategy,
        data,
        risk_limits,
        config.run.initial_capital,
        config.costs.fee_bps,
        config.costs.slippage_bps,
        config.run.symbol.clone(),
    );
    let results = runner.run();
    write_outputs(&config, out, results, &config_path)?;
    Ok(())
}

struct RealtimeBarSource {
    bars: Vec<kairos_core::types::Bar>,
    index: usize,
    sleep_seconds: i64,
}

impl RealtimeBarSource {
    fn new(bars: Vec<kairos_core::types::Bar>, sleep_seconds: i64) -> Self {
        Self {
            bars,
            index: 0,
            sleep_seconds,
        }
    }
}

impl MarketDataSource for RealtimeBarSource {
    fn next_bar(&mut self) -> Option<kairos_core::types::Bar> {
        if self.index >= self.bars.len() {
            return None;
        }
        if self.sleep_seconds > 0 {
            thread::sleep(Duration::from_secs(self.sleep_seconds as u64));
        }
        let bar = self.bars[self.index].clone();
        self.index += 1;
        Some(bar)
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_action_type, parse_duration_like, run_backtest, run_validate};
    use std::fs;
    use std::path::PathBuf;

    fn write_file(path: &PathBuf, contents: &str) {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::write(path, contents).expect("write file");
    }

    fn sample_config(tmp_dir: &PathBuf) -> PathBuf {
        let config_path = tmp_dir.join("config.toml");
        let csv_path = tmp_dir.join("ohlcv.csv");
        let csv_contents = "timestamp_utc,open,high,low,close,volume\n\
2026-01-01T00:00:00Z,1,1,1,1,1\n\
2026-01-01T00:00:01Z,1,1,1,1,1\n";
        write_file(&csv_path, csv_contents);

        let toml_contents = format!(
            "\
[run]\n\
run_id = \"test_run\"\n\
symbol = \"BTCUSD\"\n\
timeframe = \"1m\"\n\
initial_capital = 1000.0\n\
\n\
[paths]\n\
ohlcv_csv = \"{}\"\n\
out_dir = \"{}\"\n\
\n\
[costs]\n\
fee_bps = 0.0\n\
slippage_bps = 0.0\n\
\n\
[risk]\n\
max_position_qty = 1.0\n\
max_drawdown_pct = 1.0\n\
max_exposure_pct = 1.0\n\
\n\
[features]\n\
return_mode = \"pct\"\n\
sma_windows = [2]\n\
rsi_enabled = false\n\
sentiment_lag = \"1s\"\n\
\n\
[agent]\n\
mode = \"baseline\"\n\
url = \"http://127.0.0.1:8000\"\n\
timeout_ms = 200\n\
retries = 0\n\
fallback_action = \"HOLD\"\n\
api_version = \"v1\"\n\
feature_version = \"v1\"\n",
            csv_path.display(),
            tmp_dir.display()
        );
        write_file(&config_path, &toml_contents);
        config_path
    }

    #[test]
    fn parse_duration_like_handles_units() {
        assert_eq!(parse_duration_like("5s").unwrap(), 5);
        assert_eq!(parse_duration_like("2m").unwrap(), 120);
        assert_eq!(parse_duration_like("1h").unwrap(), 3600);
    }

    #[test]
    fn parse_action_type_handles_values() {
        assert_eq!(parse_action_type("buy").unwrap() as u8, 0);
        assert_eq!(parse_action_type("sell").unwrap() as u8, 1);
        assert_eq!(parse_action_type("hold").unwrap() as u8, 2);
    }

    #[test]
    fn run_validate_reads_csv() {
        let tmp_dir = PathBuf::from("/tmp/kairos_cli_validate");
        let config_path = sample_config(&tmp_dir);
        run_validate(config_path).expect("validate");
    }

    #[test]
    fn run_backtest_writes_outputs() {
        let tmp_dir = PathBuf::from("/tmp/kairos_cli_backtest");
        let config_path = sample_config(&tmp_dir);
        run_backtest(config_path.clone(), None).expect("backtest");
        let run_dir = tmp_dir.join("test_run");
        assert!(run_dir.join("summary.json").exists());
        assert!(run_dir.join("trades.csv").exists());
        assert!(run_dir.join("equity.csv").exists());
        assert!(run_dir.join("config_snapshot.toml").exists());
    }
}
