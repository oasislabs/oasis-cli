#[macro_use]
extern crate clap;

mod cmd_build;
mod cmd_clean;
mod cmd_ifextract;
mod cmd_init;
mod command;
mod config;
mod utils;

use colored::*;
use std::{fs, path::Path};

fn main() {
    let mut app = clap_app!(oasis =>
        (about: crate_description!())
        (version: crate_version!())
        (@setting InferSubcommands)
        (@subcommand init =>
            (about: "Create a new Oasis package")
            (@arg NAME: +required "Package name")
            (@group type =>
                (@arg rust: --rust "Create a new Rust project")
            )
        )
        (@subcommand build =>
            (about: "Build services for the Oasis platform")
            (@arg release: --release "Build with optimizations")
            (@arg verbose: +multiple -v --verbose "Increase verbosity")
            (@arg stack_size: --stack-size "Set the Wasm stack size")
            (@arg hardmode: --hardmode "Build a vanilla WASI service (that doesn't use Mantle)")
            (@arg SERVICE: +multiple "Specify which service(s) to build")
        )
        (@subcommand test =>
            (about: "Run integration tests against a simulated Oasis runtime.")
            (@arg verbose: +multiple -v --verbose "Increase verbosity")
        )
        (@subcommand clean =>
            (about: "Remove build products")
        )
        (@subcommand ifextract =>
            (about: "Extract interface definition(s) from a Mantle service.wasm")
            (@arg out_dir: -o --out +takes_value "Where to write the interface.json(s). Defaults to current directory. Pass `-` to write to stdout.")
            (@arg SERVICE_URL: +required "The URL of the service.wasm file(s)")
        )
    );

    let config_dir = must_initialize();
    let config = generate_config(config_dir);

    // Store `help` for later since `get_matches` takes ownership.
    let mut help = std::io::Cursor::new(Vec::new());
    app.write_long_help(&mut help).unwrap();

    let app_m = app.get_matches();

    let result = match app_m.subcommand() {
        ("init", Some(m)) => cmd_init::init(&config, m.value_of("NAME").unwrap_or("."), "rust"),
        ("build", Some(m)) => cmd_build::BuildOptions::new(config, &m).and_then(cmd_build::build),
        ("clean", Some(_)) => cmd_clean::clean(&config),
        ("ifextract", Some(m)) => cmd_ifextract::ifextract(
            m.value_of("SERVICE_URL").unwrap(),
            Path::new(m.value_of("out_dir").unwrap_or(".")),
        ),
        _ => {
            println!("{}", String::from_utf8(help.into_inner()).unwrap());
            return;
        }
    };

    if let Err(err) = result {
        eprintln!("{}: {}", "error".red(), err.to_string());
    }
}

fn must_initialize() -> String {
    match initialize() {
        Err(err) => panic!("ERROR: failed to initialize call `{}`", err.to_string()),
        Ok(dir) => dir,
    }
}

fn initialize() -> Result<String, failure::Error> {
    let config_dir = match dirs::config_dir() {
        None => panic!("ERROR: no config direction found for user"),
        Some(config_dir) => config_dir.to_str().unwrap().to_string(),
    };

    let oasis_path = Path::new(&config_dir).join("oasis");
    if !oasis_path.exists() {
        fs::create_dir(oasis_path)?;
    }

    let logging_path = Path::new(&config_dir).join("oasis").join("log");
    if !logging_path.exists() {
        fs::create_dir(logging_path)?;
    }

    Ok(Path::new(&config_dir)
        .join("oasis")
        .to_str()
        .unwrap()
        .to_string())
}

fn generate_config(oasis_dir: String) -> config::Config {
    let id = rand::random::<u64>();
    let timestamp = chrono::Utc::now().timestamp();
    let logdir = Path::new(&oasis_dir)
        .join("log")
        .to_str()
        .unwrap()
        .to_string();

    config::Config {
        id,
        timestamp,
        logging: config::Logging {
            dir: logdir.clone(),
            path_stdout: Path::new(&logdir)
                .join(format!("{}.{}.stdout", timestamp, id))
                .to_str()
                .unwrap()
                .to_string(),
            path_stderr: Path::new(&logdir)
                .join(format!("{}.{}.stderr", timestamp, id))
                .to_str()
                .unwrap()
                .to_string(),
            enabled: true,
        },
    }
}
