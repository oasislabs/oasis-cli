use std::{collections::hash_map::Entry, ffi::OsString, path::PathBuf};

use colored::*;

use crate::utils::{detect_project_type, run_cmd_with_env, ProjectType, Verbosity};

pub struct BuildOptions {
    pub stack_size: u32,
    pub services: Vec<String>,
    pub release: bool,
    pub verbosity: Verbosity,
}

impl BuildOptions {
    pub fn new(m: &clap::ArgMatches) -> Result<Self, failure::Error> {
        Ok(Self {
            stack_size: match value_t!(m, "stack_size", u32) {
                Ok(stack_size) => stack_size,
                Err(clap::Error {
                    kind: clap::ErrorKind::ArgumentNotFound,
                    ..
                }) => 0,
                Err(err) => return Err(err.into()),
            },
            release: m.is_present("release"),
            services: m.values_of_lossy("SERVICE").unwrap_or_default(),
            verbosity: Verbosity::from(m.occurrences_of("verbose")),
        })
    }
}

/// Builds a project for the Oasis platform
pub fn build(opts: BuildOptions) -> Result<(), failure::Error> {
    match detect_project_type() {
        ProjectType::Rust(manifest) => build_rust(opts, manifest),
        ProjectType::Unknown => Err(failure::format_err!("could not detect Oasis project type.")),
    }
}

fn build_rust(
    opts: BuildOptions,
    manifest: Box<cargo_toml::Manifest>,
) -> Result<(), failure::Error> {
    let mut cargo_args = vec!["build", "--target=wasm32-wasi", "--color=always"];
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

    let target_dir = PathBuf::from(
        std::env::var_os("CARGO_TARGET_DIR")
            .unwrap_or_else(|| OsString::from("target".to_string())),
    );

    let services_dir = target_dir.join("service");
    if !services_dir.is_dir() {
        std::fs::create_dir_all(&services_dir)?;
    }

    let mut envs = std::env::vars_os().collect::<std::collections::HashMap<_, _>>();
    let stack_size_flag = OsString::from(format!("-C link-args=-zstack-size={}", opts.stack_size));
    match envs.entry(OsString::from("RUSTFLAGS")) {
        Entry::Occupied(mut ent) => ent.get_mut().push(stack_size_flag),
        Entry::Vacant(ent) => {
            ent.insert(stack_size_flag);
        }
    }
    envs.insert(OsString::from("RUSTC_WRAPPER"), OsString::from("idl-gen"));
    envs.insert(
        OsString::from("GEN_IDL_FOR"),
        OsString::from(product_names.join(",")),
    );
    envs.insert(
        OsString::from("IDL_TARGET_DIR"),
        services_dir.as_os_str().to_owned(),
    );

    if opts.verbosity >= Verbosity::Normal {
        eprintln!(
            "    {} service{}",
            "Building".cyan(),
            if num_products > 1 { "s" } else { "" }
        );
    }
    run_cmd_with_env("cargo", cargo_args, opts.verbosity, envs)?;

    let mut wasm_dir = target_dir.join("wasm32-wasi");
    wasm_dir.push(if opts.release { "release" } else { "debug" });
    for product_name in product_names {
        let wasm_name = product_name + ".wasm";
        let wasm_file = wasm_dir.join(&wasm_name);
        if !wasm_file.is_file() {
            continue;
        }
        if opts.verbosity >= Verbosity::Normal {
            eprintln!("    {} {}", "Preparing".cyan(), wasm_name,);
        }
        oasis_cli::build::prep_wasm(&wasm_file, &services_dir.join(&wasm_name), opts.release)?;
    }

    Ok(())
}
