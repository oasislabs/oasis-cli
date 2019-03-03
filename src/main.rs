#[macro_use]
extern crate clap;

include!("utils.rs");

fn main() {
    let mut app = clap_app!(oasis =>
        (about: "Oasis developer tools")
        (@subcommand init =>
            (about: "Create a new Oasis package")
            (@arg NAME: +required "Package name")
            (@group type =>
                (@arg rust: --rust "Create a new Rust project")
            )
        )
        (@subcommand build =>
            (about: "Build a package for the Oasis platform")
            (@arg release: --release "Build with optimizations")
            (@arg verbose: +multiple -v --verbose "Increase verbosity")
        )
        (@subcommand clean =>
            (about: "Remove build products")
        )
    );

    // Store `help` for later since `get_matches` takes ownership.
    let mut help = std::io::Cursor::new(Vec::new());
    app.write_long_help(&mut help).unwrap();

    let app_m = app.get_matches();

    std::process::exit(match app_m.subcommand() {
        ("init", Some(init_m)) => init(
            init_m.value_of("NAME").unwrap_or("."),
            ProjectType::Rust(None),
        ),
        ("build", Some(build_m)) => build(
            build_m.is_present("release"),
            build_m.occurrences_of("verbose") as i64,
        ),
        ("clean", Some(_clean_m)) => clean(),
        _ => {
            println!("{}", String::from_utf8(help.into_inner()).unwrap());
            -1
        }
    })
}

/// Creates an Oasis project in a directory.
fn init(dir: &str, project_type: ProjectType) -> i32 {
    match project_type {
        ProjectType::Rust(_) => {
            run_cmd!("cargo", &["init", "--lib", dir], None, -1);
            let mut xargo_toml = std::path::PathBuf::from(dir);
            xargo_toml.push("Xargo.toml");
            std::fs::write(&xargo_toml, include_str!("../Xargo.toml")).unwrap();
            0
        }
        ProjectType::Unknown => unreachable!(),
    }
}

/// Builds a project for the Oasis platform
fn build(_release: bool, verbosity: i64) -> i32 {
    match detect_project_type() {
        ProjectType::Unknown => {
            eprintln!("Could not detect Oasis project type.");
            -1
        }
        ProjectType::Rust(manifest) => {
            let manifest = manifest.unwrap();
            let mut xargo_args = vec!["build", "--target=wasm32-unknown-unknown", "--color=always"];
            if verbosity == 0 {
                xargo_args.push("--quiet");
            } else if verbosity > 1 {
                xargo_args.push("--verbose");
            }
            xargo_args.push("--release"); // TODO: make conditional when wasm-build supports --debug
            run_cmd!(
                "xargo",
                xargo_args,
                Some(vec![("RUSTFLAGS", "-Z force-unstable-if-unmarked")]),
                -1 /* silence sysroot compilation */
            );

            let exe_name = manifest
                .lib
                .as_ref()
                .or_else(|| manifest.bin.iter().nth(0))
                .and_then(|p| p.name.clone());
            if exe_name.is_none() {
                eprintln!("Could not determine package name.");
                return -1;
            }
            let mut wasm_build_args = vec![
                "--target=wasm32-unknown-unknown".to_string(),
                "target".to_string(),
                exe_name.unwrap(),
            ];
            manifest
                .package
                .and_then(|pkg| pkg.metadata)
                .as_ref()
                .and_then(|metadata| metadata.get("oasis"))
                .and_then(|oasis_metadata| oasis_metadata.get("max-mem"))
                .map(|max_mem| wasm_build_args.push(format!("--max-mem={}", max_mem)));
            run_cmd!("wasm-build", wasm_build_args, None, verbosity);
            0
        }
    }
}

fn clean() -> i32 {
    match detect_project_type() {
        ProjectType::Unknown => 0,
        ProjectType::Rust(_) => {
            run_cmd!("cargo", &["clean"], None, 0);
            0
        }
    }
}
