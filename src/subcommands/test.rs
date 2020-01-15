use std::{collections::BTreeMap, ffi::OsString};

use crate::{
    command::{run_builder, Builder, Verbosity},
    config::Config,
    emit,
    errors::Result,
    utils::{print_status_in, Status},
    workspace::{ProjectKind, Target, Workspace},
};

pub struct TestOptions<'a> {
    pub targets: Vec<&'a str>,
    pub release: bool,
    pub profile: &'a str,
    pub verbosity: Verbosity,
    pub tester_args: Vec<&'a str>,
}

impl<'a> TestOptions<'a> {
    pub fn new(m: &'a clap::ArgMatches, config: &Config) -> Result<Self> {
        let profile_name = m.value_of("profile").unwrap();
        if let Err(e) = config.profile(profile_name) {
            return Err(e.into());
        }
        Ok(Self {
            release: m.is_present("release"),
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
    fn exec(self) -> Result<()> {
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

pub fn test(targets: &[&Target], opts: TestOptions) -> Result<()> {
    for target in targets.iter().filter(|t| t.is_test()) {
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
            ProjectKind::Rust => {
                print_status();
                test_rust(target, &opts)?;
            }
            ProjectKind::JavaScript => {
                print_status();
                test_javascript(target, &opts)?;
            }
            ProjectKind::TypeScript => {
                print_status();
                test_typescript(target, &opts)?;
            }
            ProjectKind::Wasm => {}
        }
    }
    Ok(())
}

fn test_rust(target: &Target, opts: &TestOptions) -> Result<()> {
    let cargo_args = get_cargo_args(target, &opts)?;

    let mut cargo_envs: BTreeMap<_, _> = std::env::vars_os().collect();
    cargo_envs.insert(
        OsString::from("RUSTC_WRAPPER"),
        OsString::from("oasis-build"),
    );

    emit!(cmd.test.start, {
        "project_type": "rust",
        "release": opts.release,
        "rustflags": std::env::var("RUSTFLAGS").ok(),
    });

    if let Err(e) = run_builder(
        Builder::for_target(target),
        cargo_args,
        Some(cargo_envs),
        opts.verbosity,
    ) {
        emit!(cmd.test.error);
        return Err(e);
    };

    emit!(cmd.test.done);
    Ok(())
}

fn get_cargo_args<'a>(target: &'a Target, opts: &'a TestOptions) -> Result<Vec<&'a str>> {
    let mut cargo_args = vec!["test"];
    if opts.verbosity < Verbosity::Normal {
        cargo_args.push("--quiet");
    } else if opts.verbosity == Verbosity::High {
        cargo_args.push("--verbose");
    } else if opts.verbosity == Verbosity::Debug {
        cargo_args.push("-vvv")
    }

    if opts.release {
        cargo_args.push("--release");
    }

    if target.is_service() {
        cargo_args.push("--bin");
    } else if target.is_test() {
        cargo_args.push("--test");
    }
    cargo_args.push(&target.name);

    cargo_args.push("--manifest-path");
    cargo_args.push(target.project.manifest_path.as_os_str().to_str().unwrap());

    if !opts.tester_args.is_empty() {
        cargo_args.push("--");
        cargo_args.extend(opts.tester_args.iter());
    }

    Ok(cargo_args)
}

fn test_javascript(target: &Target, opts: &TestOptions) -> Result<()> {
    let package_dir = target.project.manifest_path.parent().unwrap();

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

    let mut npm_envs = BTreeMap::new();
    npm_envs.insert(
        OsString::from("OASIS_PROFILE"),
        OsString::from(&opts.profile),
    );
    if let Err(e) = run_builder(
        Builder::for_target(target),
        npm_args,
        Some(npm_envs),
        opts.verbosity,
    ) {
        emit!(cmd.test.error);
        return Err(e);
    }

    emit!(cmd.test.done);
    Ok(())
}

fn test_typescript(target: &Target, opts: &TestOptions) -> Result<()> {
    unimplemented!()
}
