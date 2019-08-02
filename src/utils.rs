use std::{
    fmt,
    path::{Path, PathBuf},
};

use colored::*;

use crate::error::Error;

pub struct Project {
    pub manifest_path: PathBuf,
    pub kind: ProjectKind,
}

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
    let root = git2::Repository::discover(&cwd)
        .map_err(|_| enoproj())?
        .path()
        .parent() // remove the .git
        .unwrap()
        .to_path_buf();

    let mut projects = std::collections::HashMap::new();
    for ancestor in cwd.ancestors() {
        let mut found_project = false;
        for entry in ignore::Walk::new(ancestor) {
            let entry = entry?;
            if projects.contains_key(entry.path()) {
                continue;
            }
            if let Some(project_kind) = ProjectKind::from_manifest(entry.path())? {
                found_project = true;
                projects.insert(
                    entry.path().to_path_buf(),
                    Project {
                        manifest_path: entry.path().to_path_buf(),
                        kind: project_kind,
                    },
                );
            }
        }
        if ancestor == root || found_project {
            break;
        }
    }

    let mut projects: Vec<Project> = projects.into_iter().map(|(_, v)| v).collect();
    projects.sort_unstable_by(|a, b| {
        // build service before js app
        match (&a.kind, &b.kind) {
            (ProjectKind::Javascript(_), _) => std::cmp::Ordering::Greater,
            (_, ProjectKind::Javascript(_)) => std::cmp::Ordering::Less,
            _ => std::cmp::Ordering::Equal,
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
    let cwd = std::env::current_dir().unwrap();
    eprintln!(
        "{} {}{}",
        status,
        what.to_string(),
        match whence.map(|w| w.strip_prefix(cwd)) {
            Some(Ok(rel_whence)) if rel_whence != Path::new("") => {
                format!(" ({})", rel_whence.display())
            }
            _ => "".to_string(),
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
