use crate::{
    command::{run_cmd, Verbosity},
    config::Config,
    utils::{detect_project_type, ProjectType},
};

pub fn clean(config: &Config) -> Result<(), failure::Error> {
    match detect_project_type() {
        ProjectType::Unknown => Ok(()),
        ProjectType::Rust(_) => run_cmd(config, "cargo", &["clean"], Verbosity::Silent),
    }
}
