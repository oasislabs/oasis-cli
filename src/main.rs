#![feature(box_syntax, concat_idents, proc_macro_hygiene)]

#[macro_use]
extern crate clap;
#[macro_use]
extern crate log;
#[macro_use]
extern crate serde;

mod command;
mod config;
mod dialogue;
mod dirs;
mod error;
mod help;
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

    if !dirs::has_home_dir() {
        error!("could not determine home directory. Please ensure that $HOME is set.");
        // ^ this is a nice way of saying "wtf m8?"
        std::process::exit(1);
    }

    let mut versions = String::with_capacity(100);
    versions.push_str(crate_version!());
    if let Ok(r) = toolchain::installed_release() {
        use crate::toolchain::Tool;
        let (this_tool, other_tools): (Vec<&Tool>, Vec<&Tool>) =
            r.tools().partition(|t| t.name() == "oasis");
        if let Some(oasis) = this_tool.get(0) {
            versions.push_str(" (");
            versions.push_str(oasis.ver());
            versions.push_str(")");
        }
        if !other_tools.is_empty() {
            for t in other_tools {
                versions.push('\n');
                versions.push_str(t.name());
                versions.push_str(" (");
                versions.push_str(t.ver());
                versions.push_str(")");
            }
        }
        versions.push_str("\ntoolchain: ");
        versions.push_str(r.name());
    }

    let mut app = clap_app!(oasis =>
        (about: crate_description!())
        (version: versions.as_str())
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
            (@arg stack_size: +takes_value --stack-size
                "Set the amount of linear memory allocated to program stack (in bytes)")
            (@arg wasi: --wasi "Build a vanilla WASI service")
            (@arg SERVICE: +multiple "Specify which service(s) to build")
            (@arg builder_args: +raw "Args to pass to language-specific build tool")
        )
        (@subcommand test =>
            (about: "Run tests against a simulated Oasis runtime")
            (@arg debug: --debug "Build without optimizations")
            (@arg verbose: +multiple -v --verbose "Increase verbosity")
            (@arg profile: -p --profile default_value[local]
                "Set testing profile. Run `oasis config profile` \nto list available profiles.")
            (@arg quiet: +multiple -q --quiet "Decrease verbosity")
            (@arg SERVICE: +multiple "Specify which service(s) to build")
            (@arg tester_args: +raw "Args to pass to language-specific test tool")
        )
        (@subcommand clean => (about: "Remove build products"))
        (@subcommand chain =>
            (about: "Run a local Oasis blockchain")
            (@arg chain_args: +multiple "Args to pass to oasis-chain")
        )
        (@subcommand ifextract =>
            (about: "Extract interface definition(s) from a service.wasm")
            (@arg out_dir: -o --out +takes_value
                "Where to write the interface.json(s). \
                 Defaults to current directory. Pass `-` to write to stdout.")
            (@arg SERVICE_URL: +required "The URL of the service.wasm file(s)")
        )
        (@subcommand deploy =>
            (about: "Deploy a service to the Oasis blockchain")
        )
        (@subcommand upload_metrics => (@setting Hidden))
        (@subcommand config =>
            (@arg KEY: +required "The configuration key to set")
            (@arg VALUE: "The new configuration value")
        )
    )
    .subcommand(
        clap::SubCommand::with_name("set-toolchain")
            .about("Set the Oasis toolchain version")
            .after_help(help::SET_TOOLCHAIN)
            .arg(
                clap::Arg::with_name("VERSION")
                    .takes_value(true)
                    .required(true),
            ),
    );

    let mut config = Config::load().unwrap_or_else(|err| {
        warn!("could not load config file: {}", err);
        Config::new()
    });

    if let Err(err) = telemetry::init(&config) {
        warn!("could not enable telemetry: {}", err);
    };

    // Handle `oasis chain` before args are parsed so that we can get the raw
    // args to pass to `oasis-chain`. Among other things, Clap won't allow
    // collecting unknown flags.
    let mut args = std::env::args().skip(1);
    if args.next().as_ref().map(|s| s.as_str()) == Some("chain") {
        run_chain(args.collect::<Vec<_>>()).ok();
        return;
    }

    // Store `help` for later since `get_matches` takes ownership.
    let mut help = std::io::Cursor::new(Vec::new());
    app.write_long_help(&mut help).unwrap();

    let app_m = app.get_matches();
    let result = match app_m.subcommand() {
        ("init", Some(m)) => InitOptions::new(&m).exec(),
        ("build", Some(m)) => BuildOptions::new(&m).exec(),
        ("test", Some(m)) => TestOptions::new(&m, &config).exec(),
        ("clean", Some(_)) => clean(),
        ("ifextract", Some(m)) => ifextract(
            m.value_of("SERVICE_URL").unwrap(),
            std::path::Path::new(m.value_of("out_dir").unwrap_or(".")),
        ),
        ("deploy", Some(_)) => deploy(),
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
