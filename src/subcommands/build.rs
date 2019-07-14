use std::{
    collections::hash_map::Entry,
    ffi::OsString,
    path::{Path, PathBuf},
};

use colored::*;

use crate::{
    command::{run_cmd_with_env, run_cmd_with_env_and_output, Sink, Verbosity},
    config::Config,
    emit,
    utils::{detect_project_type, ProjectType},
};

pub struct BuildOptions<'a> {
    pub config: &'a Config,
    pub stack_size: Option<u32>,
    pub services: Vec<String>,
    pub release: bool,
    pub hardmode: bool,
    pub verbosity: Verbosity,
}

impl<'a> BuildOptions<'a> {
    pub fn new(config: &'a Config, m: &clap::ArgMatches) -> Result<Self, failure::Error> {
        Ok(Self {
            config,
            stack_size: match value_t!(m, "stack_size", u32) {
                Ok(stack_size) => Some(stack_size),
                Err(clap::Error {
                    kind: clap::ErrorKind::ArgumentNotFound,
                    ..
                }) => None,
                Err(err) => return Err(err.into()),
            },
            release: m.is_present("release"),
            services: m.values_of_lossy("SERVICE").unwrap_or_default(),
            hardmode: m.is_present("hardmode"),
            verbosity: Verbosity::from(m.occurrences_of("verbose")),
        })
    }
}

/// Builds a project for the Oasis platform
pub fn build(opts: BuildOptions) -> Result<(), failure::Error> {
    match detect_project_type() {
        ProjectType::Rust(manifest) => build_rust(opts, manifest),
        ProjectType::Unknown => match opts.services.as_slice() {
            [svc] if svc.ends_with(".wasm") || svc == "a.out" => {
                let svc = Path::new(svc);
                let parent = svc.parent();
                let out_file = if parent.is_none() || parent.unwrap().to_str().unwrap().is_empty() {
                    svc.with_extension("wasm")
                } else {
                    svc.to_path_buf()
                };
                prep_wasm(&svc, &out_file, opts.release)?;
                Ok(())
            }
            _ => Err(failure::format_err!("could not detect Oasis project type.")),
        },
    }
}

fn build_rust(
    opts: BuildOptions,
    manifest: Box<cargo_toml::Manifest>,
) -> Result<(), failure::Error> {
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

    let envs = get_cargo_envs(&opts)?;

    if opts.verbosity >= Verbosity::Normal {
        eprintln!(
            "    {} service{}",
            "Building".cyan(),
            if num_products > 1 { "s" } else { "" }
        );
    }
    let target_dir = get_target_dir(&cargo_args)?;

    let services_dir = target_dir.join("service");
    if !services_dir.is_dir() {
        std::fs::create_dir_all(&services_dir)?;
    }

    emit!(cmd.build.start, {
        "project_type": "rust",
        "num_services": num_products,
        "release": opts.release,
        "hardmode": opts.hardmode,
        "stack_size": opts.stack_size,
        "rustflags": std::env::var("RUSTFLAGS").ok(),
    });

    if run_cmd_with_env("cargo", cargo_args, opts.verbosity, envs).is_err() {
        emit!(cmd.build.error);
    };

    let mut wasm_dir = target_dir.join("wasm32-wasi");
    wasm_dir.push(if opts.release { "release" } else { "debug" });
    emit!(cmd.build.prep_wasm);
    for product_name in product_names {
        let wasm_name = product_name + ".wasm";
        let wasm_file = wasm_dir.join(&wasm_name);
        if !wasm_file.is_file() {
            continue;
        }
        if opts.verbosity >= Verbosity::Normal {
            eprintln!("    {} {}", "Preparing".cyan(), wasm_name,);
        }
        prep_wasm(&wasm_file, &services_dir.join(&wasm_name), opts.release)?;
    }

    emit!(cmd.build.done);
    Ok(())
}

fn get_cargo_args<'a>(
    opts: &'a BuildOptions,
    manifest: &'a cargo_toml::Manifest,
) -> Result<Vec<&'a str>, failure::Error> {
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

    Ok(cargo_args)
}

fn get_cargo_envs<'a>(
    opts: &'a BuildOptions,
) -> Result<std::collections::HashMap<OsString, OsString>, failure::Error> {
    let mut envs = std::env::vars_os().collect::<std::collections::HashMap<_, _>>();
    if let Some(stack_size) = opts.stack_size {
        let stack_size_flag = OsString::from(format!(" -C link-args=-zstack-size={}", stack_size));
        match envs.entry(OsString::from("RUSTFLAGS")) {
            Entry::Occupied(mut ent) => ent.get_mut().push(stack_size_flag),
            Entry::Vacant(ent) => {
                ent.insert(stack_size_flag);
            }
        }
    }
    if !opts.hardmode {
        envs.insert(
            OsString::from("RUSTC_WRAPPER"),
            OsString::from("mantle-build"),
        );
    }
    Ok(envs)
}

pub fn get_target_dir(cargo_args: &[&str]) -> Result<PathBuf, failure::Error> {
    let build_plan_args = cargo_args
        .iter()
        .chain(&["-Zunstable-options", "--build-plan"])
        .collect::<Vec<_>>();
    let mut build_plan_str = Vec::new();
    run_cmd_with_env_and_output(
        "cargo",
        build_plan_args,
        std::env::vars_os(),
        Sink::Piped(&mut build_plan_str),
        Sink::Ignored,
    )?;
    serde_json::from_slice::<serde_json::Value>(&build_plan_str)
        .ok()
        .as_ref()
        .and_then(|plan| plan.get("invocations"))
        .and_then(serde_json::Value::as_array)
        .and_then(|invs| invs.last())
        .and_then(|inv| inv.get("args"))
        .cloned()
        .and_then(|args| serde_json::from_value::<Vec<String>>(args).ok())
        .and_then(
            |args| match args.iter().position(|a| a.as_str() == "--out-dir") {
                Some(pos) => Some(PathBuf::from(&args[pos + 1])), // .../<relase_mode>/deps
                None => None,
            },
        )
        .and_then(|p| p.parent().and_then(Path::parent).map(Path::to_path_buf))
        .ok_or_else(|| crate::error::Error::UnknownTargetDir.into())
}

pub fn prep_wasm(
    input_wasm: &Path,
    output_wasm: &Path,
    release: bool,
) -> Result<(), failure::Error> {
    let mut module = walrus::Module::from_file(input_wasm)?;

    externalize_mem(&mut module);

    if release {
        let customs_to_delete = module
            .customs
            .iter()
            .filter_map(|(id, custom)| {
                if custom.name().starts_with("mantle") {
                    None
                } else {
                    Some(id)
                }
            })
            .collect::<Vec<_>>();
        for id in customs_to_delete {
            module.customs.delete(id);
        }
    }

    module.emit_wasm_file(output_wasm)?;

    Ok(())
}

fn externalize_mem(module: &mut walrus::Module) {
    let mem_export_id = module
        .exports
        .iter()
        .find(|e| e.name == "memory")
        .unwrap()
        .id();
    module.exports.delete(mem_export_id);

    let mut mem = module.memories.iter_mut().nth(0).unwrap();
    mem.import = Some(module.imports.add("env", "memory", mem.id()));
}
