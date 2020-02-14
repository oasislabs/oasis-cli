use std::{
    collections::{btree_map::Entry, BTreeMap},
    ffi::OsString,
    fs,
    io::Write as _,
    path::Path,
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

        if target.yields_artifact(Artifacts::SERVICE | Artifacts::RUST_CLIENT) {
            match proj.kind {
                ProjectKind::Rust => build_rust(target, &opts)?,
                ProjectKind::Wasm => {
                    let out_file = Path::new(&target.name).with_extension("wasm");
                    prep_wasm(&Path::new(&target.name), &out_file, opts.debug)?;
                }
                ProjectKind::JavaScript | ProjectKind::TypeScript => {
                    unreachable!("[tj]s services don't yet exist")
                }
            }
        }

        if target.yields_artifact(Artifacts::TYPESCRIPT_CLIENT) {
            build_typescript_client(&target, &opts)?;
        }

        if target.yields_artifact(Artifacts::APP) {
            match proj.kind {
                ProjectKind::JavaScript => build_javascript_app(target, &opts)?,
                ProjectKind::TypeScript => build_typescript_app(workspace, &target, &opts)?,
                ProjectKind::Wasm => unreachable!("there's no such thing as a Wasm app"),
                ProjectKind::Rust => unimplemented!("rust apps are not fully baked"),
            }
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
    emit!(cmd.build.start, { "project_type": target.project.kind.name() });

    let deps = workspace.dependencies_of(target)?;

    if !deps.is_empty() {
        // link deps
        let deps_dir = ensure_dir!(target.deps_dir())?;
        if !deps_dir.join("package.json").exists() {
            crate::cmd!(in &deps_dir, "npm", "init", "-y")?;
            crate::cmd!(in &deps_dir, "npm", "install", "buffer", "oasis-std")?;
            crate::cmd!(in &deps_dir, "npm", "install", "-D", "prettier")?;
        }

        for dep in deps {
            let module_name = ts::module_name(&dep.name);
            let ts_filename = disambiguated_filename(dep, FileType::TypeScript);
            let ts_link = deps_dir.join(&ts_filename);

            let pretty_dep_name = format!("{}.js", dep.name);
            let js_path = deps_dir.join(format!(
                "{}{}",
                module_name,
                FileType::JavaScript.extension()
            ));
            if ts_link.exists() && js_path.exists() {
                print_status(Status::Fresh, &pretty_dep_name);
                continue;
            }
            std::os::unix::fs::symlink(dep.artifacts_dir().join(&ts_filename), &ts_link).or_else(
                |e| {
                    if e.kind() == std::io::ErrorKind::AlreadyExists {
                        Ok(())
                    } else {
                        Err(format_err!("could not link `{}`", ts_link.display()))
                    }
                },
            )?;
            print_status(Status::Building, &pretty_dep_name);
            crate::cmd!(
                in deps_dir,
                "npx",
                "tsc",
                &ts_filename,
                "--pretty",
                "--allowSyntheticDefaultImports",
                "--declaration",
                "--module", "umd",
                "--moduleResolution", "node",
                "--sourceMap",
                "--strict",
                "--target", "es2015"
            )?;
            for ftype in &[
                FileType::JavaScript,
                FileType::SourceMap,
                FileType::Declaration,
            ] {
                let from = deps_dir.join(disambiguated_filename(dep, *ftype));
                let to = deps_dir.join(format!("{}{}", module_name, ftype.extension()));
                fs::rename(&from, &to).map_err(|e| {
                    format_err!(
                        "could not rename `{}` to `{}`: {}",
                        from.display(),
                        to.display(),
                        e
                    )
                })?;
            }
        }
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
    let bytecode_url = url::Url::parse(&format!("file://{}", wasm_path.display())).unwrap();

    let iface = crate::subcommands::ifextract::extract_interface(
        oasis_rpc::import::ImportLocation::Path(wasm_path.to_path_buf()),
        target.manifest_dir(),
    )?
    .pop()
    .unwrap();

    let ts_file = ensure_dir!(target.artifacts_dir())?
        .join(disambiguated_filename(target, FileType::TypeScript));
    if !ts_file.exists() {
        let mut out_file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&ts_file)
            .map_err(|e| anyhow::format_err!("could not open `{}`: {}", ts_file.display(), e))?;
        out_file
            .write_all(ts::generate(&iface, &bytecode_url).to_string().as_bytes())
            .map_err(|e| {
                anyhow::format_err!("could not generate `{}`: {}", ts_file.display(), e)
            })?;
        crate::cmd!("npx", "prettier", "--write", &ts_file)?;
    }
    Ok(())
}

fn disambiguated_filename(target: &Target, file_type: FileType) -> String {
    format!(
        "{}-{}{}",
        ts::module_name(&target.name),
        target.disambiguator().unwrap(),
        file_type.extension(),
    )
}

#[derive(Clone, Copy)]
enum FileType {
    JavaScript,
    TypeScript,
    SourceMap,
    Declaration,
}

impl FileType {
    pub fn extension(&self) -> &str {
        match self {
            FileType::JavaScript => ".js",
            FileType::TypeScript => ".ts",
            FileType::SourceMap => ".js.map",
            FileType::Declaration => ".d.ts",
        }
    }
}
