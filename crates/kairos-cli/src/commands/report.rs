use std::path::PathBuf;

pub(super) fn run_report(input: PathBuf) -> Result<(), String> {
    let deps = crate::infra::build_reporting_deps();
    let result = kairos_application::reporting::generate_report(
        input.as_path(),
        deps.reader.as_ref(),
        deps.writer.as_ref(),
    )?;
    println!(
        "{} cli: report regenerated (run_id={}, trades={}, bars={})",
        kairos_application::meta::engine_name(),
        result.run_id,
        result.summary.trades,
        result.summary.bars_processed
    );
    Ok(())
}
