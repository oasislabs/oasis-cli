use std::{ffi::OsString, path::Path};

use crate::{
    command::{run_cmd_with_env, Verbosity},
    config::Config,
    emit,
    utils::{detect_projects, print_status_in, DevPhase, ProjectKind, RequestedArtifacts, Status},
};

pub struct TestOptions<'a> {
    artifacts: RequestedArtifacts<'a>,
    debug: bool,
    profile: &'a str,
    verbosity: Verbosity,
    tester_args: Vec<&'a str>,
}

impl<'a> TestOptions<'a> {
    pub fn new(m: &'a clap::ArgMatches, config: &Config) -> Result<Self, failure::Error> {
        let profile_name = m.value_of("profile").unwrap();
        if let Err(e) = config.profile(profile_name) {
            return Err(e.into());
        }
        Ok(Self {
            debug: m.is_present("debug"),
            artifacts: RequestedArtifacts::from_matches(m),
            profile: profile_name,
            verbosity: Verbosity::from(
                m.occurrences_of("verbose") as i64 - m.occurrences_of("quiet") as i64,
            ),
            tester_args: m.values_of("tester_args").unwrap_or_default().collect(),
        })
    }
}

impl<'a> super::ExecSubcommand for TestOptions<'a> {
    fn exec(self) -> Result<(), failure::Error> {
        test(self)
    }
}

pub fn test(opts: TestOptions) -> Result<(), failure::Error> {
    for proj in detect_projects(DevPhase::Test)? {
        match proj.kind {
            ProjectKind::Rust(manifest) => test_rust(&opts, &proj.manifest_path, manifest)?,
            ProjectKind::Javascript(manifest) => {
                if opts.artifacts != RequestedArtifacts::Unspecified {
                    continue;
                }
                test_js(&opts, &proj.manifest_path, manifest)?
            }
        }
    }
    Ok(())
}

fn test_rust(
    opts: &TestOptions,
    manifest_path: &Path,
    manifest: Box<cargo_toml::Manifest>,
) -> Result<(), failure::Error> {
    let cargo_args = get_cargo_args(&opts, manifest_path)?;

    let mut product_names = if let RequestedArtifacts::Explicit(services) = &opts.artifacts {
        services.clone()
    } else {
        manifest
            .bin
            .iter()
            .filter_map(|bin| bin.name.as_ref().map(|n| n.as_str()))
            .collect()
    };
    product_names.sort();
    let num_products = product_names.len();

    let mut cargo_envs = std::env::vars_os().collect::<std::collections::HashMap<_, _>>();
    cargo_envs.insert(
        OsString::from("RUSTC_WRAPPER"),
        OsString::from("oasis-build"),
    );

    if opts.verbosity > Verbosity::Quiet {
        print_status_in(
            Status::Testing,
            product_names.join(", "),
            manifest_path.parent().unwrap(),
        );
    }

    emit!(cmd.test.start, {
        "project_type": "rust",
        "num_services": num_products,
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
    opts: &'a TestOptions,
    manifest_path: &'a Path,
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

    if let RequestedArtifacts::Explicit(services) = &opts.artifacts {
        for service_name in services.iter() {
            cargo_args.push("--bin");
            cargo_args.push(service_name);
        }
    } else {
        cargo_args.push("--bins");
    }

    cargo_args.push("--manifest-path");
    cargo_args.push(manifest_path.as_os_str().to_str().unwrap());

    if !opts.tester_args.is_empty() {
        cargo_args.push("--");
        cargo_args.extend(opts.tester_args.iter());
    }

    Ok(cargo_args)
}

fn test_js(
    opts: &TestOptions,
    manifest_path: &Path,
    package_json: serde_json::Map<String, serde_json::Value>,
) -> Result<(), failure::Error> {
    let package_dir = manifest_path.parent().unwrap();

    if opts.verbosity > Verbosity::Quiet {
        print_status_in(
            Status::Testing,
            package_json["name"].as_str().unwrap(),
            package_dir,
        );
    }

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

    let mut npm_envs = std::env::vars_os().collect::<std::collections::HashMap<_, _>>();
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
