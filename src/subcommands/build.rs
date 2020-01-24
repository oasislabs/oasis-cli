use std::{
    collections::{btree_map::Entry, BTreeMap},
    ffi::OsString,
    path::Path,
};

use crate::{
    command::{BuildTool, Verbosity},
    emit,
    errors::Result,
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

        if target.yields_artifact(Artifacts::SERVICE | Artifacts::RUST_CLIENT | Artifacts::APP) {
            match proj.kind {
                ProjectKind::Rust => build_rust(target, &opts)?,
                ProjectKind::JavaScript | ProjectKind::TypeScript => {
                    build_javascript(&target, &opts)?
                }
                ProjectKind::Wasm => {
                    let out_file = Path::new(&target.name).with_extension("wasm");
                    prep_wasm(&Path::new(&target.name), &out_file, opts.debug)?;
                }
            }
        } else if target.yields_artifact(Artifacts::TYPESCRIPT_CLIENT) {
            build_typescript_client(&target, &opts)?;
        }
    }
    Ok(())
}

fn build_rust(target: &Target, opts: &BuildOptions) -> Result<()> {
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

    let target_dir = &target.project.target_dir;
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

pub fn prep_wasm(input_wasm: &Path, output_wasm: &Path, debug: bool) -> Result<()> {
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

fn build_javascript(target: &Target, opts: &BuildOptions) -> Result<()> {
    emit!(cmd.build.start, { "project_type": target.project.kind.name() });

    BuildTool::for_target(target).build(
        opts.builder_args.clone(),
        BTreeMap::new(), /* envs */
        opts.verbosity,
    )?;

    emit!(cmd.build.done);
    Ok(())
}

fn build_typescript_client(target: &Target, _opts: &BuildOptions) -> Result<()> {
    use crate::gen::typescript as ts;
    let iface = crate::subcommands::ifextract::extract_interface(
        oasis_rpc::import::ImportLocation::Path(
            target
                .wasm_path()
                .expect("service target must yield a wasm artifact"),
        ),
        target.manifest_dir(),
    )?
    .pop()
    .unwrap();
    println!("{}", ts::generate(&iface));
    Ok(())
}
