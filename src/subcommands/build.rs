use std::{
    collections::{btree_map::Entry, BTreeMap},
    ffi::OsString,
    path::Path,
};

use failure::Fallible;

use crate::{
    command::{run_cmd, run_cmd_with_env, Verbosity},
    emit,
    utils::{print_status, print_status_in, Status},
    workspace::{ProjectKind, Target, Workspace},
};

pub struct BuildOptions<'a> {
    pub targets: Vec<&'a str>,
    pub debug: bool,
    pub verbosity: Verbosity,
    pub stack_size: Option<u32>,
    pub wasi: bool,
    pub builder_args: Vec<&'a str>,
}

impl<'a> BuildOptions<'a> {
    pub fn new(m: &'a clap::ArgMatches) -> Fallible<Self> {
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
            targets: m.values_of("TARGETS").unwrap_or_default().collect(),
            wasi: m.is_present("wasi"),
            verbosity: Verbosity::from(
                m.occurrences_of("verbose") as i64 - m.occurrences_of("quiet") as i64,
            ),
            builder_args: m.values_of("builder_args").unwrap_or_default().collect(),
        })
    }
}

impl<'a> super::ExecSubcommand for BuildOptions<'a> {
    fn exec(self) -> Fallible<()> {
        let workspace = crate::workspace::Workspace::populate()?;
        let targets = workspace.collect_targets(&self.targets)?;
        build(&workspace, &targets, self)
    }
}

pub fn build(
    workspace: &Workspace,
    targets: &[&Target],
    opts: BuildOptions,
) -> failure::Fallible<()> {
    for target in workspace.construct_build_plan(targets)? {
        let proj = target.project;
        if opts.verbosity > Verbosity::Quiet {
            print_status_in(
                Status::Building,
                &target.name,
                proj.manifest_path.parent().unwrap(),
            );
        }

        match &proj.kind {
            ProjectKind::Rust => build_rust(target, &proj.manifest_path, &proj.target_dir, &opts)?,
            ProjectKind::JavaScript { .. } => build_js(&proj.manifest_path, &opts)?,
            ProjectKind::Wasm => {
                let out_file = Path::new(&target.name).with_extension("wasm");
                prep_wasm(&Path::new(&target.name), &out_file, opts.debug)?;
            }
        }
    }
    Ok(())
}

fn build_rust(
    target: &Target,
    manifest_path: &Path,
    target_dir: &Path,
    opts: &BuildOptions,
) -> failure::Fallible<()> {
    let cargo_args = get_cargo_args(target, &manifest_path, &opts)?;

    let cargo_envs = get_cargo_envs(&opts)?;

    emit!(cmd.build.start, {
        "project_type": "rust",
        "wasi": opts.wasi,
        "stack_size": opts.stack_size,
        "rustflags": std::env::var("RUSTFLAGS").ok(),
    });

    if let Err(e) = run_cmd_with_env("cargo", cargo_args, cargo_envs, opts.verbosity) {
        emit!(cmd.build.error);
        return Err(e);
    };

    let services_dir = target_dir.join("service");
    if !services_dir.is_dir() {
        std::fs::create_dir_all(&services_dir)?;
    }

    let mut wasm_dir = target_dir.join("wasm32-wasi");
    wasm_dir.push(if opts.debug { "debug" } else { "release" });
    emit!(cmd.build.prep_wasm);

    let wasm_name = format!("{}.wasm", target.name);
    if opts.verbosity > Verbosity::Quiet {
        print_status(Status::Preparing, &wasm_name);
    }
    let wasm_file = wasm_dir.join(&wasm_name);
    if !wasm_file.is_file() {
        warn!("{} is not a regular file", wasm_file.display());
        return Ok(());
    }
    prep_wasm(&wasm_file, &services_dir.join(&wasm_name), opts.debug)?;

    emit!(cmd.build.done);
    Ok(())
}

fn get_cargo_args<'a>(
    target: &'a Target,
    manifest_path: &'a Path,
    opts: &'a BuildOptions,
) -> failure::Fallible<Vec<&'a str>> {
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

    cargo_args.push("--bin");
    cargo_args.push(&target.name);

    if !opts.builder_args.is_empty() {
        cargo_args.extend(opts.builder_args.iter());
    }
    cargo_args.push("--manifest-path");
    cargo_args.push(manifest_path.as_os_str().to_str().unwrap());

    Ok(cargo_args)
}

fn get_cargo_envs(opts: &BuildOptions) -> failure::Fallible<BTreeMap<OsString, OsString>> {
    let mut envs: BTreeMap<_, _> = std::env::vars_os().collect();
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

pub fn prep_wasm(input_wasm: &Path, output_wasm: &Path, debug: bool) -> failure::Fallible<()> {
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

fn build_js(manifest_path: &Path, opts: &BuildOptions) -> failure::Fallible<()> {
    let package_dir = manifest_path.parent().unwrap();

    emit!(cmd.build.start, { "project_type": "js" });

    run_cmd(
        &"npm",
        vec!["run", "--prefix", package_dir.to_str().unwrap(), "build"],
        opts.verbosity,
    )?;

    emit!(cmd.build.done);
    Ok(())
}
