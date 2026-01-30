use std::path::PathBuf;

pub(super) fn run_backtest(config_path: PathBuf, out: Option<PathBuf>) -> Result<(), String> {
    let (config, config_toml) = crate::config::load_config_with_source(&config_path)?;
    super::common::print_config_summary("backtest", &config, out.as_ref())?;

    let overall_start = std::time::Instant::now();

    let crate::infra::EngineDeps {
        market_data,
        sentiment_repo,
        artifacts,
        remote_agent,
    } = crate::infra::build_engine_deps(&config)?;

    let run_dir = kairos_application::backtesting::run_backtest(
        &config,
        &config_toml,
        out,
        market_data.as_ref(),
        sentiment_repo.as_ref(),
        artifacts.as_ref(),
        remote_agent,
    )?;

    println!("run output: {}", run_dir.display());
    println!(
        "{} cli: backtest total_ms={}",
        kairos_application::meta::engine_name(),
        overall_start.elapsed().as_millis()
    );
    Ok(())
}
