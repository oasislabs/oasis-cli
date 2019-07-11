#![feature(box_syntax)]

#[macro_use]
extern crate clap;
#[macro_use]
extern crate log;

mod command;
mod config;
mod error;
mod logger;
mod subcommands;
mod telemetry;
mod utils;

use std::{
    fs,
    path::{Path, PathBuf},
};

use subcommands::{build, clean, ifextract, init, BuildOptions};

fn main() {
    let mut log_builder = env_logger::Builder::from_default_env();
    log_builder.format(log_format);
    log_builder.init();

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

    let config_dir = ensure_oasis_dirs().unwrap();
    let config = parse_config(config_dir).unwrap();

    if let Err(err) = telemetry::push(&config.telemetry, &config.logging.dir) {
        debug!("failed to collect telemetry `{}`", err.to_string());
    }

    // Store `help` for later since `get_matches` takes ownership.
    let mut help = std::io::Cursor::new(Vec::new());
    app.write_long_help(&mut help).unwrap();

    let app_m = app.get_matches();
    let result = match app_m.subcommand() {
        ("init", Some(m)) => init(&config, m.value_of("NAME").unwrap_or("."), "rust"),
        ("build", Some(m)) => BuildOptions::new(config, &m).and_then(build),
        ("clean", Some(_)) => clean(&config),
        ("ifextract", Some(m)) => ifextract(
            m.value_of("SERVICE_URL").unwrap(),
            Path::new(m.value_of("out_dir").unwrap_or(".")),
        ),
        _ => {
            println!("{}", String::from_utf8(help.into_inner()).unwrap());
            return;
        }
    };

    if let Err(err) = result {
        error!("{}", err);
        std::process::exit(1);
    }
}

fn ensure_oasis_dirs() -> Result<PathBuf, failure::Error> {
    let oasis_dir = match dirs::config_dir() {
        Some(mut config_dir) => {
            config_dir.push("oasis");
            config_dir
        }
        None => return Err(failure::format_err!("could not resolve user config dir")),
    };

    let log_dir = oasis_dir.join("log");
    if !log_dir.is_dir() {
        fs::create_dir_all(log_dir)?;
    }

    Ok(oasis_dir)
}

fn parse_config(oasis_dir: PathBuf) -> Result<config::Config, failure::Error> {
    let config_path = Path::new(&oasis_dir).join("config");
    config::Config::load(&config_path)
}

fn log_format(fmt: &mut env_logger::fmt::Formatter, record: &log::Record) -> std::io::Result<()> {
    use colored::*;
    use std::io::Write as _;

    let level = match record.level() {
        log::Level::Trace => "trace".bold().white(),
        log::Level::Debug => "debug".bold().white(),
        log::Level::Info => "info".bold().blue(),
        log::Level::Warn => "warning".bold().yellow(),
        log::Level::Error => "error".bold().red(),
    };

    writeln!(fmt, "{}: {}", level, record.args())
}
