#![feature(bind_by_move_pattern_guards, concat_idents)]

#[macro_use]
extern crate anyhow;
#[macro_use]
extern crate clap;
#[macro_use]
extern crate log;
#[macro_use]
extern crate serde;

mod cli;
mod command;
mod config;
mod dialogue;
mod dirs;
mod errors;
mod gen;
mod help;
mod subcommands;
mod telemetry;
mod utils;
mod workspace;

use subcommands::*;

fn main() {
    env_logger::from_env(env_logger::Env::default().default_filter_or("info"))
        .format(log_format)
        .init();

    if !dirs::has_home_dir() {
        error!("could not determine home directory. Please ensure that $HOME is set.");
        // ^ this is a nice way of saying "wtf m8?"
        std::process::exit(1);
    }

    let mut config = config::Config::load().unwrap_or_else(|err| {
        warn!("could not load config file: {}", err);
        Default::default()
    });

    if let Err(err) = telemetry::init(&config) {
        warn!("could not enable telemetry: {}", err);
    };

    // `oasis chain` is handled before args are parsed so that we can forward
    // the raw args to `oasis-chain`. Clap won't allow collecting unknown flags.
    let mut args = std::env::args().skip(1);
    if args.next().as_ref().map(|s| s.as_str()) == Some("chain") {
        run_chain(args.collect::<Vec<_>>()).ok();
        return;
    }

    let app_m = cli::build_app().get_matches();
    let result = match app_m.subcommand() {
        ("init", Some(m)) => InitOptions::new(&m).exec(),
        ("build", Some(m)) => BuildOptions::new(&m).exec(),
        ("test", Some(m)) => TestOptions::new(&m, &config).exec(),
        ("clean", Some(m)) => clean(
            &m.values_of("TARGETS")
                .unwrap_or_default()
                .collect::<Vec<_>>(),
        ),
        ("ifextract", Some(m)) => ifextract(
            m.value_of("IMPORT_LOC").unwrap(),
            std::path::Path::new(m.value_of("out_dir").unwrap_or(".")),
        ),
        ("deploy", Some(m)) => DeployOptions::new(&m, &config).exec(),
        ("config", Some(m)) => {
            let key = m.value_of("KEY").unwrap();
            match m.value_of("VALUE") {
                Some(v) => config.edit(key, v),
                None => {
                    if let Some(v) = config.get(key) {
                        println!("{}", v.trim())
                    }
                    Ok(())
                }
            }
        }
        ("set-toolchain", Some(m)) => toolchain::set(m.value_of("VERSION").unwrap()),
        ("upload_metrics", _) => telemetry::upload(),
        _ => {
            cli::build_app()
                .write_long_help(&mut std::io::stdout())
                .unwrap();
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
