use std::{collections::BTreeMap, ffi::OsString, path::Path};

use crate::{
    command::{run_cmd_with_env, Verbosity},
    config::Config,
    emit,
    errors::Error,
    utils::{print_status_in, Status},
    workspace::{ProjectKind, Target, Workspace},
};

pub struct TestOptions<'a> {
    pub targets: Vec<&'a str>,
    pub debug: bool,
    pub profile: &'a str,
    pub verbosity: Verbosity,
    pub tester_args: Vec<&'a str>,
}

impl<'a> TestOptions<'a> {
    pub fn new(m: &'a clap::ArgMatches, config: &Config) -> Result<Self, Error> {
        let profile_name = m.value_of("profile").unwrap();
        if let Err(e) = config.profile(profile_name) {
            return Err(e.into());
        }
        Ok(Self {
            debug: m.is_present("debug"),
            targets: m.values_of("TARGETS").unwrap_or_default().collect(),
            profile: profile_name,
            verbosity: Verbosity::from(
                m.occurrences_of("verbose") as i64 - m.occurrences_of("quiet") as i64,
            ),
            tester_args: m.values_of("tester_args").unwrap_or_default().collect(),
        })
    }
}

impl<'a> super::ExecSubcommand for TestOptions<'a> {
    fn exec(self) -> Result<(), Error> {
        let workspace = Workspace::populate()?;
        let targets = workspace.collect_targets(&self.targets)?;
        let build_opts = super::BuildOptions {
            targets: self.targets.clone(),
            debug: false,
            verbosity: self.verbosity,
            stack_size: None,
            wasi: false,
            builder_args: Vec::new(),
        };
        super::build(&workspace, &targets, build_opts)?;
        test(&targets, self)
    }
}

pub fn test(targets: &[&Target], opts: TestOptions) -> Result<(), failure::Error> {
    for target in targets {
        let proj = &target.project;
        let print_status = || {
            if opts.verbosity > Verbosity::Quiet {
                print_status_in(
                    Status::Testing,
                    &target.name,
                    proj.manifest_path.parent().unwrap(),
                );
            }
        };
        match &proj.kind {
            ProjectKind::Rust { .. } => {
                print_status();
                test_rust(target, &proj.manifest_path, &opts)?;
            }
            ProjectKind::JavaScript { testable, .. } if *testable => {
                print_status();
                test_js(&proj.manifest_path, &opts)?;
            }
            _ => (),
        }
    }
    Ok(())
}

fn test_rust(target: &Target, manifest_path: &Path, opts: &TestOptions) -> Result<(), Error> {
    let cargo_args = get_cargo_args(target, manifest_path, &opts)?;

    let mut cargo_envs: BTreeMap<_, _> = std::env::vars_os().collect();
    cargo_envs.insert(
        OsString::from("RUSTC_WRAPPER"),
        OsString::from("oasis-build"),
    );

    emit!(cmd.test.start, {
        "project_type": "rust",
        "debug": opts.debug,
        "rustflags": std::env::var("RUSTFLAGS").ok(),
    });

    if let Err(e) = run_cmd_with_env("cargo", cargo_args, cargo_envs, opts.verbosity) {
        emit!(cmd.test.error);
        return Err(e);
    };

    emit!(cmd.test.done);
    Ok(())
}

fn get_cargo_args<'a>(
    target: &'a Target,
    manifest_path: &'a Path,
    opts: &'a TestOptions,
) -> Result<Vec<&'a str>, failure::Error> {
    let mut cargo_args = vec!["test"];
    if opts.verbosity < Verbosity::Normal {
        cargo_args.push("--quiet");
    } else if opts.verbosity == Verbosity::High {
        cargo_args.push("--verbose");
    } else if opts.verbosity == Verbosity::Debug {
        cargo_args.push("-vvv")
    }

    if !opts.debug {
        cargo_args.push("--release");
    }

    cargo_args.push("--bin");
    cargo_args.push(&target.name);

    cargo_args.push("--manifest-path");
    cargo_args.push(manifest_path.as_os_str().to_str().unwrap());

    if !opts.tester_args.is_empty() {
        cargo_args.push("--");
        cargo_args.extend(opts.tester_args.iter());
    }

    Ok(cargo_args)
}

fn test_js(manifest_path: &Path, opts: &TestOptions) -> Result<(), Error> {
    let package_dir = manifest_path.parent().unwrap();

    emit!(cmd.test.start, {
        "project_type": "js",
        "tester_args": opts.tester_args,
    });

    let mut npm_args = vec!["test", "--prefix", package_dir.to_str().unwrap(), "--"];
    if opts.verbosity < Verbosity::Normal {
        npm_args.push("--silent");
    } else if opts.verbosity >= Verbosity::Verbose {
        npm_args.push("--verbose");
    }
    npm_args.extend(opts.tester_args.iter());

    let mut npm_envs: BTreeMap<_, _> = std::env::vars_os().collect();
    npm_envs.insert(
        OsString::from("OASIS_PROFILE"),
        OsString::from(&opts.profile),
    );
    if let Err(e) = run_cmd_with_env("npm", npm_args, npm_envs, opts.verbosity) {
        emit!(cmd.test.error);
        return Err(e);
    }

    emit!(cmd.test.done);
    Ok(())
}
