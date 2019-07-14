pub enum ProjectType {
    Rust(Box<cargo_toml::Manifest>),
    Unknown,
}

pub fn detect_project_type() -> ProjectType {
    let cargo_toml = std::path::Path::new("Cargo.toml");
    if cargo_toml.exists() {
        let mut manifest = cargo_toml::Manifest::from_path(cargo_toml).unwrap();
        manifest
            .complete_from_path(std::path::Path::new("."))
            .unwrap();
        ProjectType::Rust(box manifest)
    } else {
        ProjectType::Unknown
    }
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
