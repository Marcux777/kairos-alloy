use std::path::PathBuf;

pub(super) fn run_validate(
    config_path: PathBuf,
    strict: bool,
    out: Option<PathBuf>,
) -> Result<(), String> {
    super::run_validate(config_path, strict, out)
}
