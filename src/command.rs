use std::{ffi::OsStr, io, process::Stdio};

use crate::error::Error;

#[derive(Clone, Copy, PartialOrd, PartialEq)]
pub enum Verbosity {
    Silent,
    Quiet,
    Normal,
    Verbose,
    High,
    Debug,
}

impl From<i64> for Verbosity {
    fn from(num_vs: i64) -> Self {
        match num_vs {
            vs if vs < -1 => Verbosity::Silent,
            -1 => Verbosity::Quiet,
            0 => Verbosity::Normal,
            1 => Verbosity::Verbose,
            2 => Verbosity::High,
            vs if vs > 2 => Verbosity::Debug,
            _ => unreachable!(),
        }
    }
}

// `cmd` captures output and is intended for internal use.
#[macro_export]
macro_rules! cmd {
    ($prog:expr, $( $arg:expr ),+) => {{
        let mut cmd = std::process::Command::new($prog);
        $( cmd.arg($arg); )+
        let output = cmd.output()?;
        if !output.status.success() {
            Err(failure::format_err!("{} exited with non-zero status code", $prog))
        } else {
            Ok(output)
        }
    }}
}

pub fn run_cmd(
    name: &'static str,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    verbosity: Verbosity,
) -> Result<(), failure::Error> {
    run_cmd_with_env(name, args, verbosity, std::env::vars_os())
}

pub fn run_cmd_with_env(
    name: &'static str,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    verbosity: Verbosity,
    envs: impl IntoIterator<Item = (impl AsRef<OsStr>, impl AsRef<OsStr>)>,
) -> Result<(), failure::Error> {
    let (stdout, stderr) = match verbosity {
        Verbosity::Silent => (Stdio::null(), Stdio::null()),
        _ => (Stdio::inherit(), Stdio::inherit()),
    };
    run_cmd_with_env_and_output(name, args, envs, stdout, stderr)
}

pub fn run_cmd_with_env_and_output(
    name: &str,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    envs: impl IntoIterator<Item = (impl AsRef<OsStr>, impl AsRef<OsStr>)>,
    stdout: Stdio,
    stderr: Stdio,
) -> Result<(), failure::Error> {
    let output = std::process::Command::new(name)
        .args(args)
        .envs(envs)
        .stdout(stdout)
        .stderr(stderr)
        .output()
        .map_err(|e| match e.kind() {
            io::ErrorKind::NotFound => Error::ExecNotFound(name.to_string()).into(),
            _ => failure::Error::from(e),
        })?;

    if output.status.success() {
        Ok(())
    } else {
        Err(Error::ProcessExit(name.to_string(), output.status.code().unwrap()).into())
    }
}
