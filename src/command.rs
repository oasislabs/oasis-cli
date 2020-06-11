use std::{collections::BTreeMap, ffi::OsString, io, path::Path, process::Stdio};

use crate::{
    emit,
    errors::{CliError, Error, Result},
    workspace::{Project, ProjectKind, Target},
};

#[macro_export]
macro_rules! rust_toolchain {
    () => {
        "nightly-2020-02-16"
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
    ($(in $curdir:expr,)? $prog:expr, $( $arg:expr ),+) => {{
        let mut cmd = std::process::Command::new($prog);
        $(cmd.current_dir(&$curdir);)?
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
                let err_msg = [
                    std::str::from_utf8(&output.stdout).unwrap(),
                    std::str::from_utf8(&output.stderr).unwrap()
                ].join("\n");
                Err(anyhow!("`{}` exited with error:\n{}", $prog, err_msg.trim()))
            } else {
                Ok(output)
            }
        })
    }}
}

pub struct BuildTool<'a> {
    project: &'a Project,
    workdir: &'a Path,
    kind: BuildToolKind,
}

impl<'a> BuildTool<'a> {
    pub fn for_target(target: &'a Target) -> Self {
        Self::for_project(&target.project)
    }

    pub fn for_project(project: &'a Project) -> Self {
        Self {
            project,
            workdir: project.manifest_path.parent().unwrap(), // TODO: fixup for lerna
            kind: BuildToolKind::detect(project),
        }
    }

    pub fn build(
        self,
        args: Vec<&'a str>,
        envs: BTreeMap<OsString, OsString>,
        verbosity: Verbosity,
    ) -> Result<()> {
        if let BuildToolKind::Npm | BuildToolKind::Yarn = self.kind {
            self.install_node_modules()?;
        }
        self.run("build", args, envs, verbosity)
    }

    pub fn test(
        self,
        args: Vec<&'a str>,
        envs: BTreeMap<OsString, OsString>,
        verbosity: Verbosity,
    ) -> Result<()> {
        if let BuildToolKind::Npm | BuildToolKind::Yarn = self.kind {
            self.install_node_modules()?;
        }
        self.run("test", args, envs, verbosity)
    }

    pub fn deploy(
        self,
        args: Vec<&'a str>,
        envs: BTreeMap<OsString, OsString>,
        verbosity: Verbosity,
    ) -> Result<()> {
        if let BuildToolKind::Npm | BuildToolKind::Yarn = self.kind {
            self.install_node_modules()?;
        }
        self.run("deploy", args, envs, verbosity)
    }

    pub fn clean(self) -> Result<()> {
        self.run(
            "clean",
            Vec::new(),      /* args */
            BTreeMap::new(), /* envs */
            Verbosity::Silent,
        )
    }

    fn run(
        &self,
        subcommand: &'a str,
        mut builder_args: Vec<&'a str>,
        mut envs: BTreeMap<OsString, OsString>,
        verbosity: Verbosity,
    ) -> Result<()> {
        let mut args = Vec::new();

        if let BuildToolKind::Cargo = self.kind {
            args.push(concat!("+", rust_toolchain!()));
        }

        match self.kind {
            BuildToolKind::Cargo => {
                args.push(subcommand);
                if verbosity < Verbosity::Normal {
                    args.push("--quiet");
                } else if verbosity == Verbosity::High {
                    args.push("--verbose");
                } else if verbosity == Verbosity::Debug {
                    args.push("-vvv")
                }
                args.push("--manifest-path");
                args.push(self.project.manifest_path.to_str().unwrap())
            }
            BuildToolKind::Npm => {
                if verbosity < Verbosity::Normal {
                    args.push("--silent");
                } else if verbosity >= Verbosity::Verbose {
                    args.push("--verbose");
                }
                args.push("--prefix");
                args.push(self.workdir.to_str().unwrap());
                args.push(subcommand);
            }
            BuildToolKind::Yarn => {
                if verbosity < Verbosity::Normal {
                    args.push("--silent");
                } else if verbosity >= Verbosity::Verbose {
                    args.push("--verbose");
                }
                args.push("--cwd");
                args.push(self.workdir.to_str().unwrap());
                args.push(subcommand);
            }
        }

        args.append(&mut builder_args);

        for (k, v) in std::env::vars_os() {
            envs.entry(k).or_insert(v);
        }

        run_cmd_internal(self.name(), args, Some(envs), verbosity)
    }

    fn name(&self) -> &str {
        match self.kind {
            BuildToolKind::Cargo => "cargo",
            BuildToolKind::Npm => "npm",
            BuildToolKind::Yarn => "yarn",
        }
    }

    fn install_node_modules(&self) -> Result<()> {
        if !self.workdir.join("node_modules").is_dir() {
            if let Err(e) = self.run(
                "install",
                Vec::new(),      /* args */
                BTreeMap::new(), /* envs */
                Verbosity::Silent,
            ) {
                emit!(cmd.build.error, {
                    "cause": format!("{} install", self.name())
                });
                return Err(e);
            }
        }
        Ok(())
    }
}

pub enum BuildToolKind {
    Cargo,
    Npm,
    Yarn,
}

impl BuildToolKind {
    fn detect(project: &Project) -> Self {
        match project.kind {
            ProjectKind::Wasm => unreachable!("wasm is not buildable"),
            ProjectKind::Rust => BuildToolKind::Cargo,
            ProjectKind::JavaScript { .. } | ProjectKind::TypeScript { .. } => {
                if project
                    .manifest_path
                    .parent()
                    .unwrap()
                    .join("yarn.lock")
                    .is_file()
                    || cmd!("which", "yarn").is_ok()
                {
                    BuildToolKind::Yarn
                } else {
                    BuildToolKind::Npm
                }
            }
        }
    }
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
