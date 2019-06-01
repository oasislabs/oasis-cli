use std::{
    collections::hash_map::Entry,
    ffi::OsString,
    path::{Path, PathBuf},
};

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
    cargo_args.push(if opts.verbosity <= Verbosity::Normal {
        "--quiet"
    } else if opts.verbosity <= Verbosity::High {
        "--verbose"
    } else {
        "-vvv"
    });
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

    let mut envs = std::env::vars_os().collect::<std::collections::HashMap<_, _>>();
    let stack_size_flag = OsString::from(format!("-C link-args=-zstack-size={}", opts.stack_size));
    match envs.entry(OsString::from("RUSTFLAGS")) {
        Entry::Occupied(mut ent) => ent.get_mut().push(stack_size_flag),
        Entry::Vacant(ent) => {
            ent.insert(stack_size_flag);
        }
    }
    run_cmd_with_env("cargo", cargo_args, opts.verbosity, envs)?;

    let target_dir = PathBuf::from(
        std::env::var_os("CARGO_TARGET_DIR")
            .unwrap_or_else(|| OsString::from("target".to_string())),
    );

    let mut wasm_dir = target_dir.join("wasm32-wasi");
    wasm_dir.push(if opts.release { "release" } else { "debug" });

    let services_dir = target_dir.join("service");
    if !services_dir.is_dir() {
        println!("creating dir!");
        std::fs::create_dir(&services_dir)?;
    }

    let product_names = if opts.services.is_empty() {
        manifest
            .bin
            .iter()
            .filter_map(|bin| bin.name.as_ref().map(String::to_string))
            .collect()
    } else {
        opts.services
    };

    for service_name in product_names {
        let wasm_name = service_name + ".wasm";
        prep_wasm(
            &wasm_dir.join(&wasm_name),
            &services_dir.join(&wasm_name),
            opts.release,
        )?;
    }
    Ok(())
}

fn prep_wasm(input_wasm: &Path, output_wasm: &Path, release: bool) -> Result<(), failure::Error> {
    let mut module = walrus::Module::from_file(input_wasm)?;

    remove_start_fn(&mut module);
    externalize_mem(&mut module);

    if release {
        module.customs = Default::default();
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

fn remove_start_fn(module: &mut walrus::Module) {
    let mut start_fn_ids = None;
    for export in module.exports.iter() {
        if let walrus::ExportItem::Function(fn_id) = export.item {
            if export.name == "_start" {
                start_fn_ids = Some((export.id(), fn_id));
            }
        }
    }
    if let Some((start_export_id, start_fn_id)) = start_fn_ids {
        module.exports.delete(start_export_id);
        module.funcs.delete(start_fn_id);
    }
}
