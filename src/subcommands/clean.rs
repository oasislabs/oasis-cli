use crate::{
    command::{run_builder, Builder, Verbosity},
    emit,
    workspace::{ProjectKind, Workspace},
};

pub fn clean(target_strs: &[&str]) -> Result<(), crate::errors::Error> {
    let workspace = Workspace::populate()?;
    let targets = workspace
        .collect_targets(target_strs)?
        .into_iter()
        .filter(|t| t.is_clean())
        .collect::<Vec<_>>();
    for proj in workspace.projects_of(&targets) {
        let builder = Builder::for_project(proj);
        match &proj.kind {
            ProjectKind::Rust { .. } => {
                emit!(cmd.clean, "rust");
                run_builder(
                    builder,
                    vec!["clean"],
                    None, /* extra_envs */
                    Verbosity::Silent,
                )?;
            }
            ProjectKind::JavaScript { .. } | ProjectKind::TypeScript { .. } => {
                emit!(cmd.clean, "javascript");
                run_builder(
                    builder,
                    vec!["clean"],
                    None, /* extra_envs */
                    Verbosity::Silent,
                )?;
            }
            ProjectKind::Wasm => std::fs::remove_file(&proj.targets[0].name)?,
        };
    }
    Ok(())
}
