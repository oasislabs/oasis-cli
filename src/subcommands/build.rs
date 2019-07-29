use std::{
    collections::hash_map::Entry,
    ffi::OsString,
    path::{Path, PathBuf},
};

use colored::*;

use crate::{
    command::{run_cmd_with_env, Verbosity},
    emit,
    utils::{detect_project_type, ProjectType},
};

pub struct BuildOptions<'a> {
    stack_size: Option<u32>,
    services: Vec<String>,
    release: bool,
    wasi: bool,
    verbosity: Verbosity,
    builder_args: Vec<&'a str>,
}

impl<'a> BuildOptions<'a> {
    pub fn new(m: &'a clap::ArgMatches) -> Result<Self, failure::Error> {
        Ok(Self {
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
            wasi: m.is_present("wasi"),
            verbosity: Verbosity::from(m.occurrences_of("verbose")),
            builder_args: m.values_of("builder_args").unwrap_or_default().collect(),
        })
    }
}

impl<'a> super::ExecSubcommand for BuildOptions<'a> {
    fn exec(self) -> Result<(), failure::Error> {
        build(self)
    }
}

pub fn build(opts: BuildOptions) -> Result<(), failure::Error> {
    let mut manifest_path = PathBuf::new();
    match detect_project_type(&mut manifest_path)? {
        ProjectType::Rust(manifest) => build_rust(opts, &manifest_path, manifest),
        ProjectType::Javascript(_) => Ok(()),
        ProjectType::Unknown => match opts.services.as_slice() {
            [svc] if svc.ends_with(".wasm") || svc == "a.out" => {
                let out_file = Path::new(svc).with_extension("wasm");
                prep_wasm(&Path::new(svc), &out_file, opts.release)?;
                Ok(())
            }
            _ => Err(failure::format_err!("could not detect Oasis project type.")),
        },
    }
}

fn build_rust(
    opts: BuildOptions,
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

    let cargo_envs = get_cargo_envs(&opts)?;

    if opts.verbosity == Verbosity::Normal {
        eprintln!("    {} {}", "Building".cyan(), product_names.join(", "));
    } else if opts.verbosity > Verbosity::Normal {
        eprintln!(
            "    {} {} with manifest ({})",
            "Building".cyan(),
            product_names.join(", "),
            manifest_path.display()
        );
    }

    emit!(cmd.build.start, {
        "project_type": "rust",
        "num_services": num_products,
        "release": opts.release,
        "wasi": opts.wasi,
        "stack_size": opts.stack_size,
        "rustflags": std::env::var("RUSTFLAGS").ok(),
    });

    if run_cmd_with_env("cargo", cargo_args, opts.verbosity, cargo_envs).is_err() {
        emit!(cmd.build.error);
    };

    let target_dir = get_target_dir(manifest_path);
    // ^ MUST be called after `cargo build` to ensure that a `target` directory exists to be found

    let services_dir = target_dir.join("service");
    if !services_dir.is_dir() {
        std::fs::create_dir_all(&services_dir)?;
    }

    let mut wasm_dir = target_dir.join("wasm32-wasi");
    wasm_dir.push(if opts.release { "release" } else { "debug" });
    emit!(cmd.build.prep_wasm);

    let wasm_names = product_names
        .into_iter()
        .map(|n| n + ".wasm")
        .collect::<Vec<_>>();
    if opts.verbosity >= Verbosity::Normal {
        eprintln!("   {} {}", "Preparing".cyan(), wasm_names.join(","));
    }
    for wasm_name in wasm_names {
        let wasm_file = wasm_dir.join(&wasm_name);
        if !wasm_file.is_file() {
            continue;
        }
        prep_wasm(&wasm_file, &services_dir.join(&wasm_name), opts.release)?;
    }

    emit!(cmd.build.done);
    Ok(())
}

fn get_cargo_args<'a>(
    opts: &'a BuildOptions,
    manifest_path: &'a PathBuf,
    manifest: &'a cargo_toml::Manifest,
) -> Result<Vec<&'a str>, failure::Error> {
    let mut cargo_args = vec!["build", "--target=wasm32-wasi"];
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

    if !opts.builder_args.is_empty() {
        cargo_args.push("--");
        cargo_args.extend(opts.builder_args.iter());
    }
    cargo_args.push("--manifest-path");
    cargo_args.push(manifest_path.as_os_str().to_str().unwrap());

    Ok(cargo_args)
}

fn get_cargo_envs(
    opts: &BuildOptions,
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
    if !opts.wasi {
        envs.insert(
            OsString::from("RUSTC_WRAPPER"),
            OsString::from("oasis-build"),
        );
    }
    Ok(envs)
}

fn test_for_target_dir(path: &PathBuf) -> Option<PathBuf> {
    let maybe_target = path.join("target");
    if maybe_target.join("wasm32-wasi").is_dir() {
        Some(maybe_target)
    } else {
        None
    }
}

pub fn get_target_dir(manifest_path: &PathBuf) -> PathBuf {
    // Ideally this would use `cargo --build-plan`, but that resets incremental compilation,
    // for some reason. This is the next best thing.
    std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .ok()
        .or_else(|| test_for_target_dir(manifest_path))
        .or_else(|| {
            manifest_path.parent().and_then(|workdir| {
                workdir
                    .ancestors()
                    .find_map(|d| test_for_target_dir(&d.to_path_buf()))
            })
        })
        .unwrap_or_else(|| PathBuf::from("target"))
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
                if custom.name().starts_with("oasis") {
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
    let mem_export_id = match module.exports.iter().find(|e| e.name == "memory") {
        Some(mem) => mem.id(),
        None => return,
    };
    module.exports.delete(mem_export_id);

    let mut mem = module.memories.iter_mut().nth(0).unwrap();
    mem.import = Some(module.imports.add("env", "memory", mem.id()));
}
