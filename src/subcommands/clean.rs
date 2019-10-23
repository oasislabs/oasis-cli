use crate::{
    command::{run_cmd, Verbosity},
    emit,
    workspace::{ProjectKind, Workspace},
};

pub fn clean(target_strs: &[&str]) -> failure::Fallible<()> {
    let workspace = Workspace::populate()?;
    let targets = workspace.collect_targets(target_strs)?;
    for proj in workspace.projects_of(&targets) {
        match &proj.kind {
            ProjectKind::Rust { .. } => {
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
            ProjectKind::JavaScript { .. } => {
                emit!(cmd.clean, "javascript");
                run_cmd(
                    "npm",
                    vec![
                        "run",
                        "--prefix",
                        proj.manifest_path.parent().unwrap().to_str().unwrap(),
                        "clean",
                    ],
                    Verbosity::Silent,
                )?;
            }
            ProjectKind::Wasm => std::fs::remove_file(&proj.targets[0].name)?,
        };
    }
    Ok(())
}
