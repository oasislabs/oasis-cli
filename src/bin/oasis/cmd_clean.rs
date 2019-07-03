use crate::utils::{detect_project_type, run_cmd, ProjectType, Verbosity};

pub fn clean() -> Result<String, failure::Error> {
    match detect_project_type() {
        ProjectType::Unknown => Ok("".to_owned()),
        ProjectType::Rust(_) => match run_cmd("cargo", &["clean"], Verbosity::Silent) {
            Ok(()) => Ok("".to_owned()),
            Err(err) => Err(err),
        },
    }
}
