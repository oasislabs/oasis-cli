#[macro_use]
extern crate clap;

enum ProjectType {
    Rust(Option<cargo_toml::Manifest>),
    Unknown,
}

fn main() {
    let mut app = clap::clap_app!(oasis =>
        (about: "Oasis developer tools")
        (@subcommand init =>
            (about: "Create a new Oasis package.")
            (@arg NAME: +required "Package name")
            (@group type =>
                (@arg rust: --rust "Create a new Rust project")
            )
        )
        (@subcommand build =>
            (about: "Build a package for the Oasis platform.")
            (@arg release: --release "Build with optimizations")
            (@arg verbose: +multiple -v --verbose "Increase verbosity")
        )
    );

    let mut help = std::io::Cursor::new(Vec::new());
    app.write_long_help(&mut help).unwrap();

    let app_m = app.get_matches();

    std::process::exit(match app_m.subcommand() {
        ("build", Some(build_m)) => build(
            build_m.is_present("release"),
            build_m.occurrences_of("verbose") as i64,
        ),
        ("init", Some(init_m)) => init(init_m.value_of("name").unwrap(), ProjectType::Rust(None)),
        _ => {
            println!("{}", String::from_utf8(help.into_inner()).unwrap());
            -1
        }
    })
}

fn run_cmd<S: AsRef<std::ffi::OsStr>>(
    cmd: &str,
    args: Vec<S>,
    envs: Vec<(S, S)>,
    verbosity: i64,
) -> Result<i32, String> {
    use std::process::Stdio;
    let (stdout, stderr) = if verbosity < 0 {
        (Stdio::null(), Stdio::null())
    } else if verbosity == 0 {
        (Stdio::null(), Stdio::inherit())
    } else {
        (Stdio::inherit(), Stdio::inherit())
    };
    let status = std::process::Command::new(cmd)
        .stdout(stdout)
        .stderr(stderr)
        .args(args)
        .envs(envs.into_iter())
        .spawn()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => format!(
                "Could not run {}, please make sure it is in your PATH.",
                cmd
            ),
            _ => e.to_string(),
        })?
        .wait()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(0)
    } else {
        Ok(status.code().unwrap_or(-1))
    }
}

macro_rules! run_cmd {
    ($($arg:expr),+) => {
        match run_cmd($($arg),+) {
            Err(err) => {
                eprintln!("oasis-build failed. {}", err);
                return -1
            }
            Ok(status_code) if status_code != 0 => return status_code,
            _ => ()
        }
    }
}

fn ensure_xargo_toml() {
    let xargo_toml = std::path::Path::new("Xargo.toml");
    if !xargo_toml.exists() {
        std::fs::write(xargo_toml, include_str!("../Xargo.toml")).unwrap();
    }
}

/// Initializes an Oasis project
fn init(_name: &str, project_type: ProjectType) -> i32 {
    match project_type {
        ProjectType::Rust(_) => {
            ensure_xargo_toml();
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
            ensure_xargo_toml();
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
                vec![("RUSTFLAGS", "-Z force-unstable-if-unmarked")],
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
            run_cmd!("wasm-build", wasm_build_args, vec![], verbosity);
            0
        }
    }
}

fn detect_project_type() -> ProjectType {
    let cargo_toml = std::path::Path::new("Cargo.toml");
    if cargo_toml.exists() {
        let mut manifest = cargo_toml::Manifest::from_path(cargo_toml).unwrap();
        manifest
            .complete_from_path(std::path::Path::new("."))
            .unwrap();
        ProjectType::Rust(Some(manifest))
    } else {
        ProjectType::Unknown
    }
}
