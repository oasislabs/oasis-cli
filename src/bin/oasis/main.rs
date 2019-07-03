#[macro_use]
extern crate clap;

mod cmd_build;
mod cmd_clean;
mod cmd_doctor;
mod cmd_init;
mod utils;

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
        (@subcommand doctor =>
            (about: "Verify the system has installed the expected rust toolchain and was compile target version ")
        )
    );

    // Store `help` for later since `get_matches` takes ownership.
    let mut help = std::io::Cursor::new(Vec::new());
    app.write_long_help(&mut help).unwrap();

    let app_m = app.get_matches();

    let result = match app_m.subcommand() {
        ("init", Some(m)) => cmd_init::init(m.value_of("NAME").unwrap_or("."), "rust"),
        ("build", Some(m)) => cmd_build::BuildOptions::new(&m).and_then(cmd_build::build),
        ("clean", Some(_)) => cmd_clean::clean(),
        ("doctor", Some(_)) => cmd_doctor::doctor(),
        _ => {
            println!("{}", String::from_utf8(help.into_inner()).unwrap());
            return;
        }
    };

    match result {
        Err(err) => {
            use colored::*;
            eprintln!("{}: {}", "error".red(), err.to_string());
        }
        Ok(s) => {
            if s.len() > 0 {
                use colored::*;
                eprintln!("{}: {}", "success".green(), s);
            }
        }
    }
}
