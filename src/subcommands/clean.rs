use crate::{
    command::{run_cmd, Verbosity},
    utils::{detect_project_type, ProjectType},
};

pub fn clean(_config: &crate::config::Config) -> Result<(), failure::Error> {
    match detect_project_type() {
        ProjectType::Unknown => Ok(()),
        ProjectType::Rust(_) => run_cmd("cargo", &["clean"], Verbosity::Silent),
    }
}
