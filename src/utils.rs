use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
};

pub enum ProjectType {
    Rust(Box<cargo_toml::Manifest>),
    Javascript(serde_json::Value),
    Unknown,
}

type ProjectCreator = fn(&Path) -> Result<ProjectType, failure::Error>;

const RUST_ARTIFACT: &str = "Cargo.toml";

static PROJECT_CREATORS: phf::Map<&'static str, ProjectCreator> = phf::phf_map! {
    "Cargo.toml" => create_rust_project,
    "package.json" => create_js_project,
};

fn create_rust_project(artifact: &Path) -> Result<ProjectType, failure::Error> {
    let mut manifest = cargo_toml::Manifest::from_path(artifact).unwrap();
    manifest.complete_from_path(artifact).unwrap();
    Ok(ProjectType::Rust(box manifest))
}

fn create_js_project(artifact: &Path) -> Result<ProjectType, failure::Error> {
    Ok(ProjectType::Javascript(serde_json::from_slice(
        &std::fs::read(artifact)?,
    )?))
}

fn search_for_artifact(
    path: &mut PathBuf,
    start: &Path,
    visited_ancestors: &mut Vec<PathBuf>,
) -> Result<ProjectType, failure::Error> {
    let mut to_visit = VecDeque::new();
    to_visit.push_back(start.to_path_buf());
    visited_ancestors.push(start.to_path_buf());
    loop {
        let to_search = to_visit.pop_front().unwrap();
        for e in to_search.read_dir()? {
            if e.is_err() {
                continue;
            }
            let e = e?;
            if e.path().is_dir() && !visited_ancestors.contains(&e.path()) {
                to_visit.push_back(e.path().clone());
            } else {
                let file_name = e.file_name();
                if let Some(creator) = PROJECT_CREATORS.get(file_name.to_str().unwrap()) {
                    path.push(e.path());
                    return creator(&e.path());
                }
            }
        }
        if to_visit.is_empty() {
            break;
        }
    }
    path.push(".");
    Err(failure::format_err!("No artifact found"))
}

fn find_ancestors(start: &Path) -> Option<Vec<PathBuf>> {
    let mut ancestors = Vec::new();
    let mut prefix = start.to_path_buf();
    loop {
        if !prefix.pop() {
            break;
        }
        ancestors.push(prefix.clone());
        for e in prefix.read_dir().unwrap() {
            if let Ok(e) = e {
                if e.file_name() == Path::new(".git") {
                    return Some(ancestors);
                }
            }
        }
    }
    None
}

pub fn detect_project_type(path: &mut PathBuf) -> Result<ProjectType, failure::Error> {
    // if cwd is empty then return default Rust project
    if Path::new(".").read_dir().iter().count() == 0 {
        path.push(RUST_ARTIFACT);
        return create_rust_project(Path::new(RUST_ARTIFACT));
    }

    let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if let Ok(project) = search_for_artifact(path, &current_dir, &mut Vec::new()) {
        return Ok(project);
    }

    // walk the path of your ancestors. There is wisdom there so search till
    // there is no stone left unturned till we find .git or die trying
    if let Some(ancestors) = find_ancestors(&current_dir) {
        let mut visited_ancestors = Vec::new();
        for ancestor in ancestors {
            if let Ok(p) = search_for_artifact(path, &ancestor, &mut visited_ancestors) {
                return Ok(p);
            }
        }
    }

    path.push(".");
    Ok(ProjectType::Unknown)
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
