use std::{ffi::OsString, path::Path};

use crate::{
    command::{run_cmd_with_env, Verbosity},
    emit,
    utils::{detect_projects, print_status, ProjectKind, Status},
};

pub struct TestOptions<'a> {
    services: Vec<String>,
    debug: bool,
    verbosity: Verbosity,
    tester_args: Vec<&'a str>,
}

impl<'a> TestOptions<'a> {
    pub fn new(m: &'a clap::ArgMatches) -> Result<Self, failure::Error> {
        Ok(Self {
            debug: m.is_present("debug"),
            services: m.values_of_lossy("SERVICE").unwrap_or_default(),
            verbosity: Verbosity::from(m.occurrences_of("verbose")),
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
    for proj in detect_projects()? {
        match proj.kind {
            ProjectKind::Rust(manifest) => test_rust(&opts, &proj.manifest_path, manifest)?,
            ProjectKind::Javascript(manifest) => {
                test_js(&opts, &proj.manifest_path.parent().unwrap(), manifest)?
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
    let cargo_args = get_cargo_args(&opts, manifest_path, &*manifest)?;

    let product_names = if opts.services.is_empty() {
        manifest
            .bin
            .iter()
            .filter_map(|bin| bin.name.as_ref().map(String::to_string))
            .collect()
    } else {
        opts.services.clone()
    };
    let num_products = product_names.len();

    let mut cargo_envs = std::env::vars_os().collect::<std::collections::HashMap<_, _>>();
    cargo_envs.insert(
        OsString::from("RUSTC_WRAPPER"),
        OsString::from("oasis-build"),
    );

    print_status(
        Status::Testing,
        product_names.join(", "),
        Some(manifest_path.parent().unwrap()),
    );

    emit!(cmd.test.start, {
        "project_type": "rust",
        "num_services": num_products,
        "debug": opts.debug,
        "rustflags": std::env::var("RUSTFLAGS").ok(),
    });

    if run_cmd_with_env("cargo", cargo_args, opts.verbosity, cargo_envs).is_err() {
        emit!(cmd.test.error);
    };

    emit!(cmd.test.done);
    Ok(())
}

fn test_js(
    opts: &TestOptions,
    manifest_path: &Path,
    package_json: serde_json::Map<String, serde_json::Value>,
) -> Result<(), failure::Error> {
    print_status(
        Status::Testing,
        package_json["name"].as_str().unwrap(),
        Some(manifest_path.parent().unwrap()),
    );

    emit!(cmd.test.start, {
        "project_type": "js",
        "tester_args": opts.tester_args,
    });

    let mut npm_args = vec!["test"];
    npm_args.push("--prefix");
    npm_args.push(manifest_path.as_os_str().to_str().unwrap());
    if !opts.tester_args.is_empty() {
        npm_args.push("--");
        npm_args.extend(opts.tester_args.iter());
    }
    let mut npm_envs = std::env::vars_os().collect::<std::collections::HashMap<_, _>>();
    npm_envs.insert(OsString::from("OASIS_PROFILE"), OsString::from("local"));
    if run_cmd_with_env("npm", npm_args, opts.verbosity, npm_envs).is_err() {
        emit!(cmd.test.error);
    };

    emit!(cmd.test.done);
    Ok(())
}

fn get_cargo_args<'a>(
    opts: &'a TestOptions,
    manifest_path: &'a Path,
    manifest: &'a cargo_toml::Manifest,
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

    if opts.services.is_empty() {
        cargo_args.push("--bins");
    } else {
        for service_name in opts.services.iter() {
            if !manifest
                .bin
                .iter()
                .any(|bin| Some(service_name) == bin.name.as_ref())
            {
                return Err(failure::format_err!(
                    "could not find service binary `{}` in crate",
                    service_name
                ));
            }
            cargo_args.push("--bin");
            cargo_args.push(service_name);
        }
    }

    cargo_args.push("--manifest-path");
    cargo_args.push(manifest_path.as_os_str().to_str().unwrap());

    if !opts.tester_args.is_empty() {
        cargo_args.push("--");
        cargo_args.extend(opts.tester_args.iter());
    }

    Ok(cargo_args)
}
