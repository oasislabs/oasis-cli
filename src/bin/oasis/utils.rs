use std::{ffi::OsStr, process};

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

pub struct Output {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
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

pub fn run_cmd_with_output(
    name: &str,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    envs: impl IntoIterator<Item = (impl AsRef<OsStr>, impl AsRef<OsStr>)>,
    stdout: process::Stdio,
    stderr: process::Stdio,
) -> Result<process::Output, failure::Error> {
    let mut cmd = process::Command::new(name);
    cmd.stdout(stdout).stderr(stderr).args(args).envs(envs);
    cmd.spawn()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => failure::format_err!(
                "Could not run `{}`, please make sure it is in your PATH.",
                name
            ),
            _ => failure::format_err!("{}", e.to_string()),
        })?
        .wait_with_output()
        .map_err(|e| failure::format_err!("{}", e.to_string()))
}

pub fn run_cmd_capture_output(
    name: &str,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
) -> Result<Output, failure::Error> {
    let output = run_cmd_with_output(
        name,
        args,
        std::env::vars_os(),
        process::Stdio::piped(),
        process::Stdio::piped(),
    )?;

    if output.status.success() {
        Ok(Output {
            stdout: output.stdout,
            stderr: output.stderr,
        })
    } else {
        Err(failure::format_err!(
            "Processes `{}` exited with code `{}`",
            name,
            output.status.code().unwrap()
        ))
    }
}

pub fn run_cmd_with_env(
    name: &str,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    verbosity: Verbosity,
    envs: impl IntoIterator<Item = (impl AsRef<OsStr>, impl AsRef<OsStr>)>,
) -> Result<(), failure::Error> {
    let (stdout, stderr) = if verbosity < Verbosity::Normal {
        (process::Stdio::null(), process::Stdio::null())
    } else if verbosity == Verbosity::Verbose {
        (process::Stdio::null(), process::Stdio::inherit())
    } else {
        (process::Stdio::inherit(), process::Stdio::inherit())
    };

    let output = run_cmd_with_output(name, args, envs, stdout, stderr)?;

    if output.status.success() {
        Ok(())
    } else {
        Err(failure::format_err!(
            "Processes `{}` exited with code `{}`",
            name,
            output.status.code().unwrap()
        ))
    }
}
