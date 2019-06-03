use std::ffi::OsStr;

pub enum ProjectType {
    Rust(Box<cargo_toml::Manifest>),
    Unknown,
}

#[derive(Clone, Copy, PartialOrd, PartialEq)]
pub enum Verbosity {
    Silent,
    Normal,
    Verbose,
    High,
    Debug,
}

impl From<u64> for Verbosity {
    fn from(num_vs: u64) -> Self {
        match num_vs {
            0 => Verbosity::Normal,
            1 => Verbosity::Verbose,
            2 => Verbosity::High,
            _ => Verbosity::Debug,
        }
    }
}

pub fn detect_project_type() -> ProjectType {
    let cargo_toml = std::path::Path::new("Cargo.toml");
    if cargo_toml.exists() {
        let mut manifest = cargo_toml::Manifest::from_path(cargo_toml).unwrap();
        manifest
            .complete_from_path(std::path::Path::new("."))
            .unwrap();
        ProjectType::Rust(Box::new(manifest))
    } else {
        ProjectType::Unknown
    }
}

pub fn run_cmd(
    name: &str,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    verbosity: Verbosity,
) -> Result<(), failure::Error> {
    run_cmd_with_env(name, args, verbosity, std::env::vars_os())
}

pub fn run_cmd_with_env(
    name: &str,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    verbosity: Verbosity,
    envs: impl IntoIterator<Item = (impl AsRef<OsStr>, impl AsRef<OsStr>)>,
) -> Result<(), failure::Error> {
    use std::process::{Command, Stdio};
    let (stdout, stderr) = if verbosity < Verbosity::Normal {
        (Stdio::null(), Stdio::null())
    } else if verbosity == Verbosity::Verbose {
        (Stdio::null(), Stdio::inherit())
    } else {
        (Stdio::inherit(), Stdio::inherit())
    };
    let mut cmd = Command::new(name);
    cmd.stdout(stdout).stderr(stderr).args(args).envs(envs);
    let status = cmd
        .spawn()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => failure::format_err!(
                "Could not run `{}`, please make sure it is in your PATH.",
                name
            ),
            _ => failure::format_err!("{}", e.to_string()),
        })?
        .wait()
        .map_err(|e| failure::format_err!("{}", e.to_string()))?;
    if status.success() {
        Ok(())
    } else {
        Err(failure::format_err!(
            "Processes `{}` exited with code `{}`",
            name,
            status.code().unwrap()
        ))
    }
}
