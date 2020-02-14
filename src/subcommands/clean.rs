use crate::{
    command::BuildTool,
    emit,
    workspace::{ProjectKind, Workspace},
};

pub fn clean(target_strs: &[&str]) -> Result<(), crate::errors::Error> {
    let workspace = Workspace::populate()?;
    let targets = workspace
        .collect_targets(target_strs)?
        .into_iter()
        .filter(|t| t.is_cleanable())
        .collect::<Vec<_>>();
    for proj in workspace.projects_of(&targets) {
        emit!(cmd.clean, { "project_type": proj.kind.name() });
        match &proj.kind {
            ProjectKind::Wasm => std::fs::remove_file(&proj.targets[0].name)?,
            _ => BuildTool::for_project(proj).clean()?,
        };
    }
    Ok(())
}
