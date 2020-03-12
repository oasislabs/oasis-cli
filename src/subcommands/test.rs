use std::{collections::BTreeMap, ffi::OsString};

use crate::{
    command::{BuildTool, Verbosity},
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
    for target in targets.iter().filter(|t| t.is_testable()) {
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
            ProjectKind::JavaScript { .. } | ProjectKind::TypeScript { .. } => {
                print_status();
                test_javascript(target, &opts)?;
            }
            ProjectKind::Wasm => {}
        }
    }
    Ok(())
}

fn test_rust(target: &Target, opts: &TestOptions) -> Result<()> {
    let mut args = Vec::new();

    if opts.release {
        args.push("--release");
    }

    if target.is_buildable() {
        args.push("--bin");
    } else if target.is_testable() {
        args.push("--test");
    }
    args.push(&target.name);

    if !opts.tester_args.is_empty() {
        args.push("--");
        args.extend(opts.tester_args.iter());
    }

    let mut envs: BTreeMap<_, _> = std::env::vars_os().collect();
    envs.insert(
        OsString::from("RUSTC_WRAPPER"),
        OsString::from("oasis-build"),
    );

    emit!(cmd.test.start, {
        "project_type": target.project.kind.name(),
        "release": opts.release,
        "rustflags": std::env::var("RUSTFLAGS").ok(),
    });

    if let Err(e) = BuildTool::for_target(target).test(args, envs, opts.verbosity) {
        emit!(cmd.test.error);
        return Err(e);
    };

    emit!(cmd.test.done);
    Ok(())
}

fn test_javascript(target: &Target, opts: &TestOptions) -> Result<()> {
    emit!(cmd.test.start, {
        "project_type": target.project.kind.name(),
        "tester_args": opts.tester_args,
    });

    let mut args = Vec::new();
    if !opts.tester_args.is_empty() {
        args.push("--");
        args.extend(opts.tester_args.iter());
    }

    let mut envs = BTreeMap::new();
    envs.insert(
        OsString::from("OASIS_PROFILE"),
        OsString::from(&opts.profile),
    );
    if let Err(e) = BuildTool::for_target(target).test(args, envs, opts.verbosity) {
        emit!(cmd.test.error);
        return Err(e);
    }

    emit!(cmd.test.done);
    Ok(())
}
