use std::path::PathBuf;

pub(super) fn run_paper(config_path: PathBuf, out: Option<PathBuf>) -> Result<(), String> {
    super::run_paper(config_path, out)
}
