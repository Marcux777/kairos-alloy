use std::path::PathBuf;

mod backtest;
mod bench;
mod common;
mod paper;
mod report;
mod validate;

pub enum Command {
    Backtest {
        config: PathBuf,
        out: Option<PathBuf>,
    },
    Bench {
        bars: usize,
        step_seconds: i64,
        mode: String,
        json: bool,
        profile_svg: Option<PathBuf>,
    },
    Paper {
        config: PathBuf,
        out: Option<PathBuf>,
    },
    Validate {
        config: PathBuf,
        strict: bool,
        out: Option<PathBuf>,
    },
    Report {
        input: PathBuf,
    },
}

pub fn run(command: Command) -> Result<(), String> {
    match command {
        Command::Backtest { config, out } => backtest::run_backtest(config, out),
        Command::Bench {
            bars,
            step_seconds,
            mode,
            json,
            profile_svg,
        } => bench::run_bench(bars, step_seconds, mode, json, profile_svg),
        Command::Paper { config, out } => paper::run_paper(config, out),
        Command::Validate {
            config,
            strict,
            out,
        } => validate::run_validate(config, strict, out),
        Command::Report { input } => report::run_report(input),
    }
}

#[cfg(test)]
mod tests {
    use super::{backtest, validate};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_tmp_dir(prefix: &str) -> PathBuf {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("kairos_{prefix}_{}_{}", std::process::id(), now))
    }

    fn write_file(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::write(path, contents).expect("write file");
    }

    fn sample_config(tmp_dir: &Path, db_url: &str) -> PathBuf {
        let config_path = tmp_dir.join("config.toml");
        let toml_contents = format!(
            "\
[run]\n\
run_id = \"test_run\"\n\
symbol = \"BTCUSD\"\n\
timeframe = \"1m\"\n\
initial_capital = 1000.0\n\
\n\
[db]\n\
url = \"{}\"\n\
ohlcv_table = \"ohlcv_candles\"\n\
exchange = \"kucoin\"\n\
market = \"spot\"\n\
\n\
[paths]\n\
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
            db_url,
            tmp_dir.display()
        );
        write_file(&config_path, &toml_contents);
        config_path
    }

    #[test]
    fn parse_duration_like_handles_units() {
        let parse = kairos_domain::value_objects::timeframe::parse_duration_like_seconds;
        assert_eq!(parse("5s").unwrap(), 5);
        assert_eq!(parse("2m").unwrap(), 120);
        assert_eq!(parse("1h").unwrap(), 3600);
        assert_eq!(parse("1min").unwrap(), 60);
    }

    #[test]
    fn normalize_timeframe_label_handles_aliases() {
        let parse = kairos_domain::value_objects::timeframe::Timeframe::parse;
        assert_eq!(parse("1m").unwrap().label, "1min");
        assert_eq!(parse("1hour").unwrap().label, "1hour");
        assert_eq!(parse("1d").unwrap().label, "1day");
    }

    #[test]
    fn run_validate_reads_postgres() {
        if std::env::var("KAIROS_DB_RUN_TESTS").ok().as_deref() != Some("1") {
            return;
        }
        let db_url = std::env::var("KAIROS_DB_URL").expect("KAIROS_DB_URL must be set");
        let tmp_dir = unique_tmp_dir("cli_validate");
        let config_path = sample_config(&tmp_dir, &db_url);
        validate::run_validate(config_path, false, None).expect("validate");
    }

    #[test]
    fn run_backtest_writes_outputs() {
        if std::env::var("KAIROS_DB_RUN_TESTS").ok().as_deref() != Some("1") {
            return;
        }
        let db_url = std::env::var("KAIROS_DB_URL").expect("KAIROS_DB_URL must be set");
        let tmp_dir = unique_tmp_dir("cli_backtest");
        let config_path = sample_config(&tmp_dir, &db_url);
        backtest::run_backtest(config_path, None).expect("backtest");
        let run_dir = tmp_dir.join("test_run");
        assert!(run_dir.join("summary.json").exists());
        assert!(run_dir.join("trades.csv").exists());
        assert!(run_dir.join("equity.csv").exists());
        assert!(run_dir.join("config_snapshot.toml").exists());
    }
}
