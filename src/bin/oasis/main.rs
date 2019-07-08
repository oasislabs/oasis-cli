#[macro_use]
extern crate clap;

use std::{fs, path};

mod cmd_build;
mod cmd_clean;
mod cmd_ifextract;
mod cmd_init;
mod command;
mod config;
mod utils;

struct Env {
    pub home: Option<String>,
}

impl Env {
    fn generate() -> Env {
        Env {
            home: match dirs::home_dir() {
                None => None,
                Some(home) => Some(home.to_str().unwrap().to_string()),
            },
        }
    }
}

fn ensure_initialization(env: &Env) -> Result<(), failure::Error> {
    match &env.home {
        None => println!("WARN: no home directoy found for user"),
        Some(home) => {
            let oasis_path = path::Path::new(home).join(".oasis");
            if !oasis_path.exists() {
                fs::create_dir(oasis_path)?;
            }
        }
    }

    Ok(())
}

fn generate_config(env: &Env) -> config::Config {
    let home = match &env.home {
        None => {
            return config::Config {
                logging: config::Logging {
                    path_stdout: String::new(),
                    path_stderr: String::new(),
                    enabled: false,
                },
            }
        }
        Some(home) => home,
    };

    config::Config {
        logging: config::Logging {
            path_stdout: path::Path::new(home)
                .join(".oasis")
                .join("logging.stdout")
                .to_str()
                .unwrap()
                .to_string(),
            path_stderr: path::Path::new(home)
                .join(".oasis")
                .join("logging.stderr")
                .to_str()
                .unwrap()
                .to_string(),
            enabled: true,
        },
    }
}

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

    let env = Env::generate();
    if let Err(err) = ensure_initialization(&env) {
        use colored::*;
        eprintln!("{}: {}", "error".red(), err.to_string());
        std::process::exit(1);
    }

    let config = generate_config(&env);

    // Store `help` for later since `get_matches` takes ownership.
    let mut help = std::io::Cursor::new(Vec::new());
    app.write_long_help(&mut help).unwrap();

    let app_m = app.get_matches();

    let result = match app_m.subcommand() {
        ("init", Some(m)) => cmd_init::init(&config, m.value_of("NAME").unwrap_or("."), "rust"),
        ("build", Some(m)) => cmd_build::BuildOptions::new(&config, &m).and_then(cmd_build::build),
        ("clean", Some(_)) => cmd_clean::clean(&config),
        ("ifextract", Some(m)) => cmd_ifextract::ifextract(
            m.value_of("SERVICE_URL").unwrap(),
            std::path::Path::new(m.value_of("out_dir").unwrap_or(".")),
        ),
        _ => {
            println!("{}", String::from_utf8(help.into_inner()).unwrap());
            return;
        }
    };

    if let Err(err) = result {
        use colored::*;
        eprintln!("{}: {}", "error".red(), err.to_string());
    }
}
