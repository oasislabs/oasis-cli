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
