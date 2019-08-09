use std::{collections::HashMap, ffi::OsString, io, path::Path, process::Stdio};

use crate::{emit, error::Error};

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
    args: &[&str],
    verbosity: Verbosity,
) -> Result<(), failure::Error> {
    run_cmd_internal(name, args, None, verbosity, true)
}

pub fn run_cmd_with_env(
    name: &'static str,
    args: &[&str],
    envs: HashMap<OsString, OsString>,
    verbosity: Verbosity,
) -> Result<(), failure::Error> {
    run_cmd_internal(name, args, Some(envs), verbosity, true)
}

fn run_cmd_internal(
    name: &'static str,
    args: &[&str],
    envs: Option<HashMap<OsString, OsString>>,
    verbosity: Verbosity,
    allow_hook: bool,
) -> Result<(), failure::Error> {
    let (stdout, stderr) = match verbosity {
        Verbosity::Silent => (Stdio::null(), Stdio::null()),
        _ => (Stdio::inherit(), Stdio::inherit()),
    };
    let mut cmd = std::process::Command::new(if allow_hook {
        hook_cmd(name, &args, verbosity)?
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
    name: &'static str,
    args: &[&str],
    verbosity: Verbosity,
) -> Result<String, failure::Error> {
    let args = args.into_iter().collect::<Vec<_>>();
    Ok(if name == "npm" {
        let name = std::env::var("OASIS_NPM").unwrap_or_else(|_| name.to_string());
        let package_dir = &args[args.iter().position(|a| **a == "--prefix").unwrap() + 1];
        npm_install_if_needed(&Path::new(package_dir), verbosity)?;
        name
    } else {
        name.to_string()
    })
}

fn npm_install_if_needed(package_dir: &Path, verbosity: Verbosity) -> Result<(), failure::Error> {
    if !package_dir.join("node_modules").is_dir() {
        let npm_args = &[
            "install",
            "--prefix",
            package_dir.to_str().unwrap(),
            "--quiet",
        ];
        if let Err(e) = run_cmd_internal("npm", npm_args, None, verbosity, false) {
            emit!(cmd.build.error, { "cause": "npm install" });
            return Err(e);
        }
    }
    Ok(())
}
