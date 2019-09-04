use std::{collections::HashMap, ffi::OsString, io, path::Path, process::Stdio};

use crate::{emit, error::Error};

#[macro_export]
macro_rules! rust_toolchain {
    () => {
        "nightly-2019-08-26"
    };
}

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
            Err(failure::format_err!(
                "{} exited with error:\n{}",
                $prog,
                String::from_utf8(output.stderr).unwrap()
            ))
        } else {
            Ok(output)
        }
    }}
}

pub fn run_cmd(name: &str, args: Vec<&str>, verbosity: Verbosity) -> Result<(), failure::Error> {
    run_cmd_internal(name, args, None, verbosity, true)
}

pub fn run_cmd_with_env(
    name: &str,
    args: Vec<&str>,
    envs: HashMap<OsString, OsString>,
    verbosity: Verbosity,
) -> Result<(), failure::Error> {
    run_cmd_internal(name, args, Some(envs), verbosity, true)
}

fn run_cmd_internal(
    name: &str,
    mut args: Vec<&str>,
    envs: Option<HashMap<OsString, OsString>>,
    verbosity: Verbosity,
    allow_hook: bool,
) -> Result<(), failure::Error> {
    let (stdout, stderr) = match verbosity {
        Verbosity::Silent => (Stdio::null(), Stdio::null()),
        _ => (Stdio::inherit(), Stdio::inherit()),
    };
    let mut cmd = std::process::Command::new(if allow_hook {
        hook_cmd(name, &mut args, verbosity)?
    } else {
        name.to_string()
    });
    cmd.args(args).stdout(stdout).stderr(stderr);

    if let Some(envs) = envs {
        cmd.envs(envs);
    }
    let output = cmd.output().map_err(|e| match e.kind() {
        io::ErrorKind::NotFound => Error::ExecNotFound(name.to_string()).into(),
        _ => failure::Error::from(e),
    })?;

    if output.status.success() {
        Ok(())
    } else {
        Err(Error::ProcessExit(name.to_string(), output.status.code().unwrap()).into())
    }
}

fn hook_cmd(
    name: &str,
    args: &mut Vec<&str>,
    verbosity: Verbosity,
) -> Result<String, failure::Error> {
    Ok(match name {
        "npm" => {
            let npm = std::env::var("OASIS_NPM").unwrap_or_else(|_| name.to_string());
            let package_dir = Path::new(
                args.iter()
                    .position(|a| *a == "--prefix")
                    .map(|p| args[p + 1])
                    .unwrap_or(""),
            );
            npm_install_if_needed(&npm, &package_dir, verbosity)?;
            npm
        }
        "cargo" => {
            if !args.get(0).unwrap_or(&"").starts_with('+') {
                args.insert(0, concat!("+", rust_toolchain!()))
            }
            name.to_string()
        }
        _ => name.to_string(),
    })
}

fn npm_install_if_needed<'a>(
    npm_command: &'a str,
    package_dir: &'a Path,
    verbosity: Verbosity,
) -> Result<(), failure::Error> {
    if !package_dir.join("node_modules").is_dir() {
        let npm_args = vec![
            "install",
            "--prefix",
            package_dir.to_str().unwrap(),
            "--quiet",
        ];
        if let Err(e) = run_cmd_internal(npm_command, npm_args, None, verbosity, false) {
            emit!(cmd.build.error, { "cause": "npm install" });
            return Err(e);
        }
    }
    Ok(())
}
