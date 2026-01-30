use std::path::PathBuf;

pub(super) fn run_validate(
    config_path: PathBuf,
    strict: bool,
    out: Option<PathBuf>,
) -> Result<(), String> {
    let (config, _config_toml) = crate::config::load_config_with_source(&config_path)?;
    super::common::print_config_summary("validate", &config, None)?;

    let crate::infra::ValidateDeps {
        market_data,
        sentiment_repo,
    } = crate::infra::build_validate_deps(&config)?;

    let report = kairos_application::validation::validate(
        &config,
        strict,
        market_data.as_ref(),
        sentiment_repo.as_ref(),
    )?;

    if let Some(out_path) = out {
        std::fs::write(&out_path, report.to_string())
            .map_err(|err| format!("failed to write report {}: {}", out_path.display(), err))?;
    }

    Ok(())
}
