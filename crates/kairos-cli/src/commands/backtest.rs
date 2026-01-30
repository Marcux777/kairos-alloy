use std::path::PathBuf;

pub(super) fn run_backtest(config_path: PathBuf, out: Option<PathBuf>) -> Result<(), String> {
    super::run_backtest(config_path, out)
}
