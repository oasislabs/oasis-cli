use crate::utils::{detect_project_type, run_cmd, ProjectType, Verbosity};

pub fn clean() -> Result<(), failure::Error> {
    match detect_project_type() {
        ProjectType::Unknown => Ok(()),
        ProjectType::Rust(_) => run_cmd("cargo", &["clean"], Verbosity::Silent),
    }
}
