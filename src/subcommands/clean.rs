use crate::{
    command::{run_cmd, Verbosity},
    emit,
    utils::{detect_projects, ProjectKind},
};

pub fn clean() -> Result<(), failure::Error> {
    for proj in detect_projects()? {
        match proj.kind {
            ProjectKind::Rust(_) => {
                emit!(cmd.clean, "rust");
                run_cmd(
                    "cargo",
                    vec![
                        "clean",
                        "--manifest-path",
                        proj.manifest_path.to_str().unwrap(),
                    ],
                    Verbosity::Silent,
                )?;
            }
            ProjectKind::Javascript(_) => {
                emit!(cmd.clean, "javascript");
                run_cmd(
                    "npm",
                    vec![
                        "run-script",
                        "--prefix",
                        proj.manifest_path.parent().unwrap().to_str().unwrap(),
                        "clean",
                    ],
                    Verbosity::Silent,
                )?;
            }
        }
    }
    Ok(())
}
