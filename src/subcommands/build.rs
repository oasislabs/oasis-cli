use std::{
    collections::{btree_map::Entry, BTreeMap},
    ffi::OsString,
    fs,
    io::Write as _,
    path::Path,
    process::Command,
    str,
};

use crate::{
    command::{BuildTool, Verbosity},
    emit, ensure_dir,
    errors::Result,
    gen::typescript as ts,
    utils::{print_status, print_status_in, Status},
    workspace::{Artifacts, ProjectKind, Target, Workspace},
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
    pub fn new(m: &'a clap::ArgMatches) -> Result<Self> {
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
    fn exec(self) -> Result<()> {
        let workspace = crate::workspace::Workspace::populate()?;
        let targets = workspace.collect_targets(&self.targets)?;
        build(&workspace, &targets, self)
    }
}

pub fn build(workspace: &Workspace, targets: &[&Target], opts: BuildOptions) -> Result<()> {
    for target in workspace
        .construct_build_plan(targets)?
        .iter()
        .filter(|t| t.is_buildable())
    {
        let proj = target.project;
        if opts.verbosity > Verbosity::Quiet {
            print_status_in(
                Status::Building,
                &target.name,
                proj.manifest_path.parent().unwrap(),
            );
        }

        if target.yields_artifact(Artifacts::SERVICE) {
            match proj.kind {
                ProjectKind::Rust => build_rust_service(target, &opts)?,
                ProjectKind::Wasm => {
                    let out_file = Path::new(&target.name).with_extension("wasm");
                    prep_wasm(&Path::new(&target.name), &out_file, opts.debug)?;
                }
                ProjectKind::JavaScript { .. } | ProjectKind::TypeScript { .. } => {
                    unreachable!("[tj]s services don't yet exist")
                }
            }
        }

        if target.yields_artifact(Artifacts::TYPESCRIPT_CLIENT) {
            build_typescript_client(&target, &opts)?;
        }

        if target.yields_artifact(Artifacts::APP) {
            match proj.kind {
                ProjectKind::JavaScript { .. } => build_javascript_app(target, &opts)?,
                ProjectKind::TypeScript { .. } => build_typescript_app(workspace, &target, &opts)?,
                ProjectKind::Rust => build_rust_app(&target, &opts)?,
                ProjectKind::Wasm => unreachable!("there's no such thing as a Wasm app"),
            }
        }
    }
    Ok(())
}

fn build_rust_service(target: &Target, opts: &BuildOptions) -> Result<()> {
    let mut args = vec!["--target=wasm32-wasi"];
    if !opts.debug {
        args.push("--release");
    }
    args.push("--bin");
    args.push(&target.name);
    args.extend(opts.builder_args.iter());

    let mut envs: BTreeMap<OsString, OsString> = BTreeMap::new();
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

    emit!(cmd.build.start, {
        "project_type": target.project.kind.name(),
        "wasi": opts.wasi,
        "stack_size": opts.stack_size,
        "rustflags": std::env::var("RUSTFLAGS").ok(),
    });

    if let Err(e) = BuildTool::for_target(target).build(args, envs, opts.verbosity) {
        emit!(cmd.build.error);
        return Err(e);
    };

    let wasm_name = format!("{}.wasm", target.name);

    if opts.verbosity > Verbosity::Quiet {
        print_status(Status::Preparing, &wasm_name);
    }

    let mut wasm_dir = target.project.target_dir.join("wasm32-wasi");
    wasm_dir.push(if opts.debug { "debug" } else { "release" });
    let wasm_file = wasm_dir.join(&wasm_name);
    if !wasm_file.is_file() {
        warn!("{} is not a regular file", wasm_file.display());
        return Ok(());
    };
    emit!(cmd.build.prep_wasm);
    prep_wasm(
        &wasm_file,
        &ensure_dir!(target.artifacts_dir())?.join(&wasm_name),
        opts.debug,
    )?;
    emit!(cmd.build.done);

    Ok(())
}

fn build_rust_app(target: &Target, opts: &BuildOptions) -> Result<()> {
    let mut args = Vec::new();
    if !opts.debug {
        args.push("--release");
    }
    args.push("--bin");
    args.push(&target.name);
    args.extend(opts.builder_args.iter());

    let mut envs: BTreeMap<OsString, OsString> = BTreeMap::new();
    envs.insert(
        OsString::from("RUSTC_WRAPPER"),
        OsString::from("oasis-build"),
    );

    emit!(cmd.build.start, {
        "project_type": format!("{} app", target.project.kind.name()),
    });

    if let Err(e) = BuildTool::for_target(target).build(args, envs, opts.verbosity) {
        emit!(cmd.build.error);
        return Err(e);
    };

    emit!(cmd.build.done);

    Ok(())
}

/// Remove a trailing newline from a byte string.
fn strip_trailing_newline(mut input: Vec<u8>) -> Vec<u8> {
    while input[..].ends_with(&[b'\n']) || input[..].ends_with(&[b'\r']) {
        input.pop();
    }
    input
}

pub fn prep_wasm(input_wasm: &Path, output_wasm: &Path, debug: bool) -> Result<()> {
    let mut module = walrus::Module::from_file(input_wasm)?;

    externalize_mem(&mut module);

    module.imports.iter_mut().for_each(|imp| {
        if imp.module.starts_with("wasi_snapshot_preview") {
            imp.module = "wasi_unstable".to_string();
        }
    });

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

    // Add a section with version info for current git repo.
    let git_sha = match Command::new("git").args(&["rev-parse", "HEAD"]).output() {
        Err(_) => "(git rev-parse failed)".as_bytes().to_vec(),
        Ok(output) => strip_trailing_newline(output.stdout),
    };
    module.customs.add(walrus::RawCustomSection {
        name: "oasis_version".to_string(),
        data: format!(
            r#"{{"sha":"{}","serviceName":"{}"}}"#,
            str::from_utf8(&git_sha[..])?,
            input_wasm.file_stem().unwrap_or_default().to_string_lossy()
        )
        .as_bytes()
        .to_vec(),
    });

    module.emit_wasm_file(output_wasm)?;

    Ok(())
}

fn externalize_mem(module: &mut walrus::Module) {
    let mem_export_id = match module.exports.iter().find(|e| e.name == "memory") {
        Some(mem) => mem.id(),
        None => return,
    };
    module.exports.delete(mem_export_id);

    let mut mem = module.memories.iter_mut().next().unwrap();
    mem.import = Some(module.imports.add("env", "memory", mem.id()));
}

fn build_javascript_app(target: &Target, opts: &BuildOptions) -> Result<()> {
    emit!(cmd.build.start, { "project_type": target.project.kind.name() });

    if let Err(e) = BuildTool::for_target(target).build(
        opts.builder_args.clone(),
        BTreeMap::new(), /* envs */
        opts.verbosity,
    ) {
        emit!(cmd.build.error);
        return Err(e);
    }

    emit!(cmd.build.done);
    Ok(())
}

fn build_typescript_app(workspace: &Workspace, target: &Target, opts: &BuildOptions) -> Result<()> {
    emit!(cmd.build.start, {
        "project_type": format!("{} app", target.project.kind.name()),
    });

    let clients_dir = ensure_dir!(target.clients_dir())?;
    for dep in workspace.dependencies_of(target)? {
        let ts_filename = format!("{}.ts", ts::module_name(&dep.name));
        let ts_client = clients_dir.join(&ts_filename);
        fs::copy(dep.artifacts_dir().join(&ts_filename), &ts_client)?;
    }

    if let Err(e) = BuildTool::for_target(target).build(
        opts.builder_args.clone(),
        BTreeMap::new(), /* envs */
        opts.verbosity,
    ) {
        emit!(cmd.build.error);
        return Err(e);
    }

    emit!(cmd.build.done);
    Ok(())
}

fn build_typescript_client(target: &Target, _opts: &BuildOptions) -> Result<()> {
    let wasm_path = target
        .wasm_path()
        .expect("service target must yield a wasm artifact");
    let bytecode = fs::read(&wasm_path)
        .map_err(|e| anyhow::anyhow!("could not read `{}`: {}", wasm_path.display(), e))?;

    let iface = crate::subcommands::ifextract::extract_interface(
        oasis_rpc::import::ImportLocation::Path(wasm_path.clone()),
        target.manifest_dir(),
    )?
    .pop()
    .unwrap();

    let ts_file =
        ensure_dir!(target.artifacts_dir())?.join(format!("{}.ts", ts::module_name(&target.name)));
    let mut out_file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&ts_file)
        .map_err(|e| anyhow::format_err!("could not open `{}`: {}", ts_file.display(), e))?;
    let output_error_handler =
        |e| anyhow::format_err!("could not generate `{}`: {}", ts_file.display(), e);
    out_file
        .write_all(
            format!(
                "// This file was AUTOGENERATED from {}.\n\
                 // It contains a client for the `{}` interface.\n\
                 // DO NOT EDIT. To regenerate, run `oasis build <myfile>.rs`.\n\n",
                wasm_path.display(),
                iface.name
            )
            .as_bytes(),
        )
        .map_err(output_error_handler)?;
    out_file
        .write_all(ts::generate(&iface, &bytecode).to_string().as_bytes())
        .map_err(output_error_handler)?;
    crate::cmd!("npx", "prettier", "--write", &ts_file).ok();
    Ok(())
}
