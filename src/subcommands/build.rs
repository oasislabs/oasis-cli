use std::{
    collections::hash_map::Entry,
    ffi::OsString,
    path::{Path, PathBuf},
};

use crate::{
    command::{run_cmd, run_cmd_with_env, Verbosity},
    emit,
    error::Error,
    utils::{
        detect_projects, print_status, print_status_in, DevPhase, ProjectKind, RequestedArtifacts,
        Status,
    },
};

pub struct BuildOptions<'a> {
    stack_size: Option<u32>,
    artifacts: RequestedArtifacts<'a>,
    debug: bool,
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
            debug: m.is_present("debug"),
            artifacts: RequestedArtifacts::from_matches(m),
            wasi: m.is_present("wasi"),
            verbosity: Verbosity::from(
                m.occurrences_of("verbose") as i64 - m.occurrences_of("quiet") as i64,
            ),
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
    if let RequestedArtifacts::Explicit(artifacts) = &opts.artifacts {
        if artifacts
            .iter()
            .all(|a| a.ends_with(".wasm") || *a == "a.out")
        {
            print_status(Status::Building, artifacts.join(", "));
            for svc in artifacts.iter() {
                let out_file = Path::new(svc).with_extension("wasm");
                prep_wasm(&Path::new(svc), &out_file, opts.debug)?;
            }
            return Ok(());
        }
    }
    let projects = detect_projects(DevPhase::Build)?;
    if projects.is_empty() {
        return Err(Error::DetectProject(format!("{}", std::env::current_dir()?.display())).into());
    }
    for proj in projects {
        match &proj.kind {
            ProjectKind::Rust(manifest) => {
                build_rust(&opts, &proj.manifest_path, manifest)?;
            }
            ProjectKind::Javascript(package_json) => {
                if opts.artifacts != RequestedArtifacts::Unspecified {
                    continue;
                }
                build_js(&opts, &proj.manifest_path, package_json)?;
            }
        }
    }
    Ok(())
}

fn build_rust(
    opts: &BuildOptions,
    manifest_path: &PathBuf,
    manifest: &cargo_toml::Manifest,
) -> Result<(), failure::Error> {
    let cargo_args = get_cargo_args(&opts, manifest_path, &*manifest)?;

    let mut product_names = if let RequestedArtifacts::Explicit(services) = &opts.artifacts {
        services.clone()
    } else {
        manifest
            .bin
            .iter()
            .filter_map(|bin| bin.name.as_ref().map(|s| s.as_str()))
            .collect()
    };
    product_names.sort();
    let num_products = product_names.len();

    let cargo_envs = get_cargo_envs(&opts)?;

    if opts.verbosity > Verbosity::Quiet {
        print_status_in(
            Status::Building,
            product_names.join(", "),
            manifest_path.parent().unwrap(),
        );
    }

    emit!(cmd.build.start, {
        "project_type": "rust",
        "num_services": num_products,
        "wasi": opts.wasi,
        "stack_size": opts.stack_size,
        "rustflags": std::env::var("RUSTFLAGS").ok(),
    });

    if let Err(e) = run_cmd_with_env("cargo", cargo_args, cargo_envs, opts.verbosity) {
        emit!(cmd.build.error);
        return Err(e);
    };

    let target_dir = get_target_dir(manifest_path);
    // ^ MUST be called after `cargo build` to ensure that a `target` directory exists to be found

    let services_dir = target_dir.join("service");
    if !services_dir.is_dir() {
        std::fs::create_dir_all(&services_dir)?;
    }

    let mut wasm_dir = target_dir.join("wasm32-wasi");
    wasm_dir.push(if opts.debug { "debug" } else { "release" });
    emit!(cmd.build.prep_wasm);

    let wasm_names = product_names
        .into_iter()
        .map(|n| format!("{}.wasm", n))
        .collect::<Vec<_>>();
    if opts.verbosity > Verbosity::Quiet {
        print_status(Status::Preparing, wasm_names.join(", "));
    }
    for wasm_name in wasm_names {
        let wasm_file = wasm_dir.join(&wasm_name);
        if !wasm_file.is_file() {
            continue;
        }
        prep_wasm(&wasm_file, &services_dir.join(&wasm_name), opts.debug)?;
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

    if !opts.debug {
        cargo_args.push("--release");
    }

    if let RequestedArtifacts::Explicit(services) = &opts.artifacts {
        for service_name in services.iter() {
            let manifest_has_service = manifest.bin.iter().any(|bin| {
                *service_name == bin.name.as_ref().map(String::as_str).unwrap_or_default()
            });
            if !manifest_has_service {
                return Err(failure::format_err!(
                    "could not find service binary `{}` in crate",
                    service_name
                ));
            }
            cargo_args.push("--bin");
            cargo_args.push(service_name);
        }
    } else {
        cargo_args.push("--bins");
    }

    if !opts.builder_args.is_empty() {
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

pub fn prep_wasm(input_wasm: &Path, output_wasm: &Path, debug: bool) -> Result<(), failure::Error> {
    let mut module = walrus::Module::from_file(input_wasm)?;

    externalize_mem(&mut module);

    if !debug {
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

fn build_js(
    opts: &BuildOptions,
    manifest_path: &PathBuf,
    package_json: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), failure::Error> {
    let package_dir = manifest_path.parent().unwrap();

    if opts.verbosity > Verbosity::Quiet {
        print_status_in(
            Status::Building,
            package_json
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("app"),
            package_dir,
        );
    }

    emit!(cmd.build.start, { "project_type": "js" });

    run_cmd(
        &"npm",
        vec!["run", "--prefix", package_dir.to_str().unwrap(), "build"],
        opts.verbosity,
    )?;

    emit!(cmd.build.done);
    Ok(())
}
