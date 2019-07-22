use std::path::PathBuf;

use crate::{
    command::{run_cmd, Verbosity},
    emit,
    utils::{detect_project_type, ProjectType},
};

pub fn clean() -> Result<(), failure::Error> {
    let mut path = PathBuf::new();
    match detect_project_type(&mut path) {
        ProjectType::Unknown => Ok(()),
        ProjectType::Rust(_) => {
            emit!(cmd.clean, "rust");
            run_cmd(
                "cargo",
                &[
                    "clean",
                    "--manifest-path",
                    path.as_os_str().to_str().unwrap(),
                ],
                Verbosity::Silent,
            )
        }
    }
}
