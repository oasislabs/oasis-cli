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
    let root = cwd
        .ancestors()
        .find(|a| a.join(".git").is_dir())
        .ok_or_else(enoproj)?;

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
    Deploying,
    Downloading,
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
                Self::Deploying => "Deploying".cyan(),
                Self::Downloading => "Downloading".cyan(),
                Self::Created => "Created".green(),
            }
        )
    }
}

pub fn print_status(status: Status, what: impl fmt::Display) {
    print_status_ctx(status, what, "");
}

pub fn print_status_in(status: Status, what: impl fmt::Display, whence: &Path) {
    let cwd = std::env::current_dir().unwrap();
    print_status_ctx(
        status,
        what,
        whence
            .strip_prefix(cwd)
            .unwrap_or_else(|_| Path::new(""))
            .display(),
    );
}

pub fn print_status_ctx(status: Status, what: impl fmt::Display, ctx: impl fmt::Display) {
    eprint!("{} {}", status, what.to_string());
    let ctx_str = ctx.to_string();
    if !ctx_str.is_empty() {
        eprintln!(" ({})", ctx_str);
    } else {
        eprintln!();
    }
}

#[macro_export]
macro_rules! ensure_dir {
    ($dir:ident$( .push($subdir:expr) )? ) => {{
        use crate::dirs::*;
        #[allow(unused_mut)]
        let mut dir = concat_idents!($dir, _dir)();
        $( dir.push($subdir); )?
        if dir.is_file() {
            Err(failure::format_err!(
                "{} dir `{}` is a file",
                stringify!($dir),
                dir.display()
            ))
        } else {
            if !dir.is_dir() {
                std::fs::create_dir_all(&dir)?
            }
            Ok(dir)
        }
    }};
}

#[macro_export]
macro_rules! oasis_dir {
    ($dir:ident) => {
        $crate::ensure_dir!($dir.push("oasis"));
    };
}
