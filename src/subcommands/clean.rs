use crate::{
    command::{run_cmd, Verbosity},
    emit,
    utils::{detect_project_type, ProjectType},
};

pub fn clean() -> Result<(), failure::Error> {
    match detect_project_type() {
        ProjectType::Unknown => Ok(()),
        ProjectType::Rust(_) => {
            emit!(cmd.clean, "rust");
            run_cmd("cargo", &["clean"], Verbosity::Silent)
        }
    }
}
