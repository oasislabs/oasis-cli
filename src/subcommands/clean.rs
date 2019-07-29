use std::path::PathBuf;

use crate::{
    command::{run_cmd, Verbosity},
    emit,
    utils::{detect_project_type, ProjectType},
};

pub fn clean() -> Result<(), failure::Error> {
    let mut manifest_path = PathBuf::new();
    match detect_project_type(&mut manifest_path)? {
        ProjectType::Unknown => Ok(()),
        ProjectType::Rust(_) => {
            emit!(cmd.clean, "rust");
            run_cmd(
                "cargo",
                &[
                    "clean",
                    "--manifest-path",
                    manifest_path.as_os_str().to_str().unwrap(),
                ],
                Verbosity::Silent,
            )
        }
        ProjectType::Javascript(_) => {
            manifest_path.pop();
            emit!(cmd.clean, "javascript");
            run_cmd(
                "npm",
                &[
                    "run-script",
                    "--prefix",
                    manifest_path.as_os_str().to_str().unwrap(),
                    "clean",
                ],
                Verbosity::Silent,
            )
        }
    }
}
