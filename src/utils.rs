extern crate walkdir;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

pub enum ProjectType {
    Rust(Box<cargo_toml::Manifest>),
    Unknown,
}

type CreateProject = fn(&Path) -> ProjectType;

const RUST_ARTIFACT: &str = "Cargo.toml";

lazy_static! {
    static ref CREATE_PROJECT: HashMap<&'static str, CreateProject> = {
        let mut m: HashMap<&str, CreateProject> = HashMap::new();
        m.insert(RUST_ARTIFACT, create_rust_project);
        m
    };
}

pub fn create_rust_project(artifact: &Path) -> ProjectType {
    let mut manifest = cargo_toml::Manifest::from_path(artifact).unwrap();
    manifest.complete_from_path(artifact).unwrap();
    ProjectType::Rust(box manifest)
}

pub fn detect_project_type(path: &mut PathBuf) -> ProjectType {
    // if cwd is empty then return default Rust project
    if Path::new(".").read_dir().into_iter().len() == 0 {
        path.push(RUST_ARTIFACT);
        return create_rust_project(Path::new(&RUST_ARTIFACT));
    }

    for entry in WalkDir::new(".")
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let to_match = entry.file_name().to_str().unwrap();
        if CREATE_PROJECT.contains_key(to_match) {
            path.push(entry.path());
            return (CREATE_PROJECT.get(to_match).unwrap())(entry.path());
        }
    }

    path.push(".");
    ProjectType::Unknown
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
