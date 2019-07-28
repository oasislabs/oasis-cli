use std::ffi::OsString;

use colored::*;

use crate::{
    command::{run_cmd_with_env, Verbosity},
    emit,
    utils::{detect_project_type, ProjectType},
};

pub struct TestOptions<'a> {
    services: Vec<String>,
    release: bool,
    verbosity: Verbosity,
    tester_args: Vec<&'a str>,
}

impl<'a> TestOptions<'a> {
    pub fn new(m: &'a clap::ArgMatches) -> Result<Self, failure::Error> {
        Ok(Self {
            release: m.is_present("release"),
            services: m.values_of_lossy("SERVICE").unwrap_or_default(),
            verbosity: Verbosity::from(m.occurrences_of("verbose")),
            tester_args: m.values_of("tester_args").unwrap().collect(),
        })
    }
}

impl<'a> super::ExecSubcommand for TestOptions<'a> {
    fn exec(self) -> Result<(), failure::Error> {
        test(self)
    }
}

pub fn test(opts: TestOptions) -> Result<(), failure::Error> {
    match detect_project_type() {
        ProjectType::Rust(manifest) => test_rust(opts, manifest),
        _ => Err(failure::format_err!("could not detect Oasis project type.")),
    }
}

fn test_rust(opts: TestOptions, manifest: Box<cargo_toml::Manifest>) -> Result<(), failure::Error> {
    let cargo_args = get_cargo_args(&opts, &*manifest)?;

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

    if opts.verbosity >= Verbosity::Normal {
        eprintln!("     {} {}", "Testing".cyan(), product_names.join(", "));
    }

    emit!(cmd.test.start, {
        "project_type": "rust",
        "num_services": num_products,
        "release": opts.release,
        "rustflags": std::env::var("RUSTFLAGS").ok(),
    });

    if run_cmd_with_env("cargo", cargo_args, opts.verbosity, cargo_envs).is_err() {
        emit!(cmd.test.error);
    };

    emit!(cmd.test.done);
    Ok(())
}

fn get_cargo_args<'a>(
    opts: &'a TestOptions,
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

    if opts.release {
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

    if !opts.tester_args.is_empty() {
        cargo_args.push("--");
        cargo_args.extend(opts.tester_args.iter());
    }

    Ok(cargo_args)
}
