#![feature(box_syntax, concat_idents)]

#[macro_use]
extern crate clap;
#[macro_use]
extern crate log;

mod command;
mod config;
mod dialogue;
mod error;
mod subcommands;
#[macro_use]
mod telemetry;
#[macro_use]
mod utils;

use config::Config;
use subcommands::{build, clean, deploy, ifextract, init, BuildOptions, InitOptions};

fn main() {
    env_logger::Builder::from_default_env()
        .format(log_format)
        .init();

    let mut app = clap_app!(oasis =>
        (about: crate_description!())
        (version: crate_version!())
        (@setting InferSubcommands)
        (@subcommand init =>
            (about: "Create a new Oasis package")
            (@arg NAME: +required "Package name")
            (@group type =>
                (@arg rust: --rust "Create a new Rust service")
            )
        )
        (@subcommand build =>
            (about: "Build services for the Oasis platform")
            (@arg release: --release "Build with optimizations")
            (@arg verbose: +multiple -v --verbose "Increase verbosity")
            (@arg stack_size: --stack-size "Set the Wasm stack size")
            (@arg hardmode: --hardmode "Build a vanilla WASI service")
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
            (about: "Extract interface definition(s) from a service.wasm")
            (@arg out_dir: -o --out +takes_value "Where to write the interface.json(s). Defaults to current directory. Pass `-` to write to stdout.")
            (@arg SERVICE_URL: +required "The URL of the service.wasm file(s)")
        )
        (@subcommand deploy =>
            (about: "Deploy the current contract on a blockchain")
            (@arg dashboard: --dashboard "If set, the deploy command will open the oasis dashboard for the deployment of the blockchain.")
        )
        (@subcommand telemetry =>
            (about: "Manage telemetry settings")
            (@setting Hidden)
            (@subcommand enable => (about: "Enable collection of anonymous usage statistics"))
            (@subcommand disable => (about: "Disable collection of anonymous usage statistics"))
            (@subcommand status => (about: "Check telemetry status"))
            (@subcommand upload => (@setting Hidden))
        )
    );

    let mut config = Config::load().unwrap_or_else(|err| {
        warn!("could not load config file: {}", err);
        Config::default()
    });

    if let Err(err) = telemetry::init(&config) {
        warn!("could not enable telemetry: {}", err);
    };

    // Store `help` for later since `get_matches` takes ownership.
    let mut help = std::io::Cursor::new(Vec::new());
    app.write_long_help(&mut help).unwrap();

    let app_m = app.get_matches();
    let result = match app_m.subcommand() {
        ("init", Some(m)) => InitOptions::new(&config, &m).and_then(init),
        ("build", Some(m)) => BuildOptions::new(&config, &m).and_then(build),
        ("clean", Some(_)) => clean(),
        ("ifextract", Some(m)) => ifextract(
            m.value_of("SERVICE_URL").unwrap(),
            std::path::Path::new(m.value_of("out_dir").unwrap_or(".")),
        ),
        ("deploy", Some(_)) => deploy(),
        ("telemetry", Some(m)) => match m.subcommand() {
            ("enable", _) => {
                config.enable_telemetry(true);
                println!("Telemetry is enabled.");
                Ok(())
            }
            ("disable", _) => {
                config.enable_telemetry(false);
                println!("Telemetry is disabled.");
                Ok(())
            }
            ("status", _) => telemetry::metrics_path().map(|p| {
                println!(
                    "Telemetry is {}.",
                    if config.telemetry.enabled {
                        "enabled"
                    } else {
                        "disabled"
                    }
                );
                println!("Usage data is being written to `{}`", p.display());
            }),
            ("upload", _) => telemetry::upload(),
            _ => Ok(()),
        },
        _ => {
            println!("{}", String::from_utf8(help.into_inner()).unwrap());
            return;
        }
    }
    .and_then(|_| config.save());

    if let Err(err) = result {
        emit!(error, {
            "args": std::env::args().collect::<Vec<_>>().join(" "),
            "error": err.to_string()
        });
        error!("{}", err);
        std::process::exit(1);
    }
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
