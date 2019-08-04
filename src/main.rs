#![feature(box_syntax, concat_idents, proc_macro_hygiene)]

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
use subcommands::*;

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
            (@arg quiet: +multiple -q --quiet "Decrease verbosity")
            (@arg NAME: +required "Package name")
            (@group type =>
                (@arg rust: --rust "Create a new Rust service")
            )
        )
        (@subcommand build =>
            (about: "Build services for the Oasis platform")
            (@arg debug: --debug "Build without optimizations")
            (@arg verbose: +multiple -v --verbose "Increase verbosity")
            (@arg quiet: +multiple -q --quiet "Decrease verbosity")
            (@arg stack_size: +takes_value --stack-size "Set the amount of linear memory allocated to program stack (in bytes)")
            (@arg wasi: --wasi "Build a vanilla WASI service")
            (@arg SERVICE: +multiple "Specify which service(s) to build")
            (@arg builder_args: +raw "Args to pass to language-specific build tool")
        )
        (@subcommand test =>
            (about: "Run tests against a simulated Oasis runtime")
            (@arg debug: --debug "Build without optimizations")
            (@arg verbose: +multiple -v --verbose "Increase verbosity")
            (@arg quiet: +multiple -q --quiet "Decrease verbosity")
            (@arg SERVICE: +multiple "Specify which service(s) to build")
            (@arg tester_args: +raw "Args to pass to language-specific test tool")
        )
        (@subcommand clean =>
            (about: "Remove build products")
        )
        (@subcommand chain =>
            (about: "Run a local Oasis blockchain")
        )
        (@subcommand ifextract =>
            (about: "Extract interface definition(s) from a service.wasm")
            (@arg out_dir: -o --out +takes_value "Where to write the interface.json(s). Defaults to current directory. Pass `-` to write to stdout.")
            (@arg SERVICE_URL: +required "The URL of the service.wasm file(s)")
        )
        (@subcommand deploy =>
            (about: "Deploy a service to the Oasis blockchain")
        )
        (@subcommand config =>
            (@setting ArgsNegateSubcommands)
            (@setting SubcommandsNegateReqs)
            (@subcommand telemetry =>
                (about: "Manage telemetry settings")
                (@subcommand enable => (about: "Enable collection of anonymous usage statistics"))
                (@subcommand disable => (about: "Disable collection of anonymous usage statistics"))
                (@subcommand status => (about: "Check telemetry status"))
                (@subcommand upload => (@setting Hidden))
            )
            (@arg NAME: +required "The name of the profile to modify.")
            (@arg KEY: +required "The configuration key to set. Must be `mnemonic`, `private_key`, or `endpoint`")
            (@arg VALUE: +required "The configuration value to set")
        )
    );

    let mut config = Config::load().unwrap_or_else(|err| {
        warn!("could not load config file: {}", err);
        Config::new()
    });

    if let Err(err) = telemetry::init(&config) {
        warn!("could not enable telemetry: {}", err);
    };

    // Store `help` for later since `get_matches` takes ownership.
    let mut help = std::io::Cursor::new(Vec::new());
    app.write_long_help(&mut help).unwrap();

    let app_m = app.get_matches();
    let result = match app_m.subcommand() {
        ("init", Some(m)) => InitOptions::new(&m).exec(),
        ("build", Some(m)) => BuildOptions::new(&m).exec(),
        ("chain", Some(_)) => run_chain(),
        ("test", Some(m)) => TestOptions::new(&m).exec(),
        ("clean", Some(_)) => clean(),
        ("ifextract", Some(m)) => ifextract(
            m.value_of("SERVICE_URL").unwrap(),
            std::path::Path::new(m.value_of("out_dir").unwrap_or(".")),
        ),
        ("deploy", Some(_)) => deploy(),
        ("config", Some(m)) => match m.subcommand() {
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
                        if config.telemetry().enabled {
                            "enabled"
                        } else {
                            "disabled"
                        }
                    );
                    println!("Usage data is being written to `{}`", p.display());
                }),
                ("upload", _) => telemetry::upload(),
                _ => {
                    println!("{}", m.usage());
                    Ok(())
                }
            },
            _ => {
                if let Err(e) = config.edit_profile(
                    m.value_of("NAME").unwrap(),
                    m.value_of("KEY").unwrap(),
                    m.value_of("VALUE").unwrap(),
                ) {
                    println!("{}", e.to_string());
                }
                Ok(())
            }
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
