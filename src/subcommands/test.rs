use std::{ffi::OsString, path::PathBuf};

use colored::*;

use crate::{
    command::{run_cmd, run_cmd_with_env, Verbosity},
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
    let mut manifest_path = PathBuf::new();
    match detect_project_type(&mut manifest_path)? {
        ProjectType::Rust(manifest) => test_rust(opts, &manifest_path, manifest),
        ProjectType::Javascript(manifest) => {
            manifest_path.pop();
            test_js(opts, &manifest_path, manifest)
        }
        _ => Err(failure::format_err!("could not detect Oasis project type.")),
    }
}

fn test_rust(
    opts: TestOptions,
    manifest_path: &PathBuf,
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

    if opts.verbosity == Verbosity::Normal {
        eprintln!("     {} {}", "Testing".cyan(), product_names.join(", "));
    } else if opts.verbosity > Verbosity::Normal {
        eprintln!(
            "     {} {} with manifest ({})",
            "Testing".cyan(),
            product_names.join(", "),
            manifest_path.display()
        );
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

fn test_js(
    opts: TestOptions,
    manifest_path: &PathBuf,
    _package_json: serde_json::Value,
) -> Result<(), failure::Error> {
    if opts.verbosity == Verbosity::Normal {
        eprintln!("     {}", "Testing".cyan());
    } else if opts.verbosity > Verbosity::Normal {
        eprintln!(
            "     {} with package.json {}",
            "Testing".cyan(),
            manifest_path.display()
        );
    }

    emit!(cmd.test.start, {
        "project_type": "Javascript",
        "tester_args": opts.tester_args,
    });

    let mut tester_args = vec!["run"];
    tester_args.push("--prefix");
    tester_args.push(manifest_path.as_os_str().to_str().unwrap());
    if !opts.tester_args.is_empty() {
        tester_args.push("--");
        tester_args.extend(opts.tester_args.iter());
    }
    if run_cmd("npm", tester_args, opts.verbosity).is_err() {
        emit!(cmd.test.error);
    };

    emit!(cmd.test.done);
    Ok(())
}

fn get_cargo_args<'a>(
    opts: &'a TestOptions,
    manifest_path: &'a PathBuf,
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
    cargo_args.push("--manifest-path");
    cargo_args.push(manifest_path.as_os_str().to_str().unwrap());

    Ok(cargo_args)
}
