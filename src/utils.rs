use std::{
    fmt,
    path::{Path, PathBuf},
};

use colored::*;

use crate::error::Error;

#[derive(Debug)]
pub struct Project {
    pub manifest_path: PathBuf,
    pub kind: ProjectKind,
}

#[derive(Debug)]
pub enum ProjectKind {
    Rust(Box<cargo_toml::Manifest>),
    Javascript(serde_json::Map<String, serde_json::Value>),
}

impl ProjectKind {
    fn from_manifest(manifest_path: &Path) -> Result<Option<Self>, failure::Error> {
        match manifest_path.file_name().and_then(|p| p.to_str()) {
            Some("Cargo.toml") => {
                let mut manifest = cargo_toml::Manifest::from_path(manifest_path).unwrap();
                manifest.complete_from_path(manifest_path).unwrap();
                Ok(Some(ProjectKind::Rust(box manifest)))
            }
            Some("package.json") => Ok(Some(ProjectKind::Javascript(serde_json::from_slice(
                &std::fs::read(manifest_path)?,
            )?))),
            _ => Ok(None),
        }
    }
}

pub fn detect_projects() -> Result<Vec<Project>, failure::Error> {
    let cwd = std::env::current_dir()?;
    let enoproj = || Error::DetectProject(format!("{}", cwd.display()));
    git2::Repository::discover(&cwd).map_err(|_| enoproj())?;

    let mut projects = std::collections::HashMap::new();
    for entry in ignore::Walk::new(cwd) {
        let entry = entry?;
        if projects.contains_key(entry.path()) {
            continue;
        }
        if let Some(project_kind) = ProjectKind::from_manifest(entry.path())? {
            projects.insert(
                entry.path().to_path_buf(),
                Project {
                    manifest_path: entry.path().to_path_buf(),
                    kind: project_kind,
                },
            );
        }
    }

    let mut projects: Vec<Project> = projects.into_iter().map(|(_, v)| v).collect();
    projects.sort_by(|a, b| {
        // build rust service before js app
        use std::cmp::Ordering::*;
        match (&a.kind, &b.kind) {
            (ProjectKind::Rust(_), ProjectKind::Rust(_)) => Equal,
            (ProjectKind::Javascript(_), ProjectKind::Javascript(_)) => Equal,
            (ProjectKind::Rust(_), ProjectKind::Javascript(_)) => Less,
            (ProjectKind::Javascript(_), ProjectKind::Rust(_)) => Greater,
        }
    });
    Ok(projects)
}

pub enum Status {
    Building,
    Preparing,
    Testing,
    Created,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{: >12}",
            match self {
                Self::Building => "Building".cyan(),
                Self::Preparing => "Preparing".cyan(),
                Self::Testing => "Testing".cyan(),
                Self::Created => "Created".green(),
            }
        )
    }
}

pub fn print_status(status: Status, what: impl fmt::Display, whence: Option<&Path>) {
    eprintln!(
        "{} {}{}",
        status,
        what.to_string(),
        match whence {
            Some(whence) => {
                let cwd = std::env::current_dir().unwrap();
                let rel_whence =
                    pathdiff::diff_paths(whence, &cwd).unwrap_or_else(|| whence.to_path_buf());
                if whence != cwd {
                    format!(" ({})", rel_whence.display())
                } else {
                    "".to_string()
                }
            }
            None => "".to_string(),
        }
    );
}

#[macro_export]
macro_rules! oasis_dir {
    ($dir:ident) => {{
        use dirs::*;
        use failure::format_err;

        concat_idents!($dir, _dir)()
            .ok_or_else(|| format_err!("could not determine {} dir", stringify!($dir)))
            .and_then(|mut dir| {
                dir.push("oasis");
                if dir.is_file() {
                    return Err(format_err!(
                        "{} dir `{}` is a file",
                        stringify!(dir),
                        dir.display()
                    ));
                }

                if !dir.is_dir() {
                    std::fs::create_dir_all(&dir)?;
                }
                Ok(dir)
            })
    }};
}
