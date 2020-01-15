use std::{collections::BTreeMap, ffi::OsString, io, path::Path, process::Stdio};

use crate::{
    emit,
    errors::{CliError, Error, Result},
    workspace::{Project, ProjectKind, Target},
};

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
        cmd.envs(std::env::vars_os());
        $( cmd.arg($arg); )+
        debug!("running internal command: {:?}", cmd);
        cmd.output().map_err(|e| {
            anyhow!(
                "could not invoke `{}`: {}",
                &[
                    $prog.to_string(),
                    $(std::ffi::OsString::from($arg).into_string().unwrap()),+
                ].join(" "),
                e
            )
        })
        .and_then(|output| {
            if !output.status.success() {
                Err(anyhow!(
                        "`{}` exited with error:\n{}", $prog, String::from_utf8(output.stderr).unwrap()
                ))
            } else {
                Ok(output)
            }
        })
    }}
}

pub struct Builder<'a> {
    pub workdir: &'a Path,
    pub kind: BuilderKind,
}

impl<'a> Builder<'a> {
    pub fn for_target(target: &'a Target) -> Self {
        Self {
            workdir: &target.path,
            kind: BuilderKind::detect(target.project.kind, &target.path),
        }
    }

    pub fn for_project(project: &'a Project) -> Self {
        let workdir = project.manifest_path.parent().unwrap();
        Self {
            workdir,
            kind: BuilderKind::detect(project.kind, workdir),
        }
    }

    fn name(&self) -> &str {
        match self.kind {
            BuilderKind::Cargo => "cargo",
            BuilderKind::Npm => "npm",
            BuilderKind::Yarn => "yarn",
        }
    }

    fn insert_default_args(&self, args: &mut Vec<&'a str>) {
        // insert workdir directive
        args.push(match self.kind {
            BuilderKind::Cargo => "--manifest-path",
            BuilderKind::Npm => "--prefix",
            BuilderKind::Yarn => "--cwd",
        });
        args.push(self.workdir.to_str().unwrap());

        if let BuilderKind::Cargo = self.kind {
            if !args.get(0).unwrap_or(&"").starts_with('+') {
                args.insert(0, concat!("+", rust_toolchain!()))
            }
        }
    }
}

#[derive(Clone, Copy)]
pub enum BuilderKind {
    Cargo,
    Npm,
    Yarn,
}

impl BuilderKind {
    fn detect(project_kind: ProjectKind, workdir: &Path) -> Self {
        match project_kind {
            ProjectKind::Wasm => unreachable!("wasm is not buildable"),
            ProjectKind::Rust => BuilderKind::Cargo,
            ProjectKind::JavaScript | ProjectKind::TypeScript => {
                if workdir.join("yarn.lock").is_file() || cmd!("which", "yarn").is_ok() {
                    BuilderKind::Yarn
                } else {
                    BuilderKind::Npm
                }
            }
        }
    }
}

fn install_node_modules(builder: &Builder) -> Result<()> {
    if !builder.workdir.join("node_modules").is_dir() {
        let mut args = vec!["install", "--silent"];
        builder.insert_default_args(&mut args);
        if let Err(e) = run_cmd_internal(builder.name(), args, None, Verbosity::Silent) {
            emit!(cmd.build.error, {
                "cause": format!("{} install", builder.name())
            });
            return Err(e);
        }
    }
    Ok(())
}

pub fn run_builder<'a>(
    builder: Builder<'a>,
    mut args: Vec<&'a str>,
    extra_envs: Option<BTreeMap<OsString, OsString>>,
    verbosity: Verbosity,
) -> Result<()> {
    builder.insert_default_args(&mut args);
    if let BuilderKind::Npm | BuilderKind::Yarn = builder.kind {
        install_node_modules(&builder)?;
    }
    let envs = match extra_envs {
        Some(mut extra_envs) => {
            extra_envs.extend(std::env::vars_os());
            extra_envs
        }
        None => std::env::vars_os().collect::<BTreeMap<_, _>>(),
    };
    run_cmd_internal(builder.name(), args, Some(envs), verbosity)
}

pub fn run_cmd(name: &str, args: Vec<&str>, verbosity: Verbosity) -> Result<()> {
    run_cmd_internal(name, args, None /* envs */, verbosity)
}

pub fn run_cmd_with_env(
    name: &str,
    args: Vec<&str>,
    envs: BTreeMap<OsString, OsString>,
    verbosity: Verbosity,
) -> Result<()> {
    run_cmd_internal(name, args, Some(envs), verbosity)
}

fn run_cmd_internal(
    name: &str,
    args: Vec<&str>,
    envs: Option<BTreeMap<OsString, OsString>>,
    verbosity: Verbosity,
) -> Result<()> {
    let (stdout, stderr) = match verbosity {
        Verbosity::Silent => (Stdio::null(), Stdio::null()),
        _ => (Stdio::inherit(), Stdio::inherit()),
    };
    let mut cmd = std::process::Command::new(name.to_string());
    cmd.args(args).stdout(stdout).stderr(stderr);

    if let Some(envs) = envs {
        cmd.envs(envs);
    }
    debug!("running command: {:?}", cmd);
    let output = cmd.output().map_err(|e| match e.kind() {
        io::ErrorKind::NotFound => CliError::ExecNotFound(name.to_string()).into(),
        _ => Error::from(e),
    })?;

    if output.status.success() {
        Ok(())
    } else {
        Err(CliError::ProcessExit(name.to_string(), output.status.code().unwrap()).into())
    }
}
