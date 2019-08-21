use crate::{help, subcommands::toolchain};

pub struct App<'a, 'b> {
    version: String,
    app: clap::App<'a, 'b>,
}

pub fn build_app<'a, 'b>() -> App<'a, 'b> {
    let mut app = App::with_version();

    let version_str = unsafe { std::mem::transmute::<&str, &'static str>(app.version.as_str()) };
    // ^ This is fine because `version` will never move and is dropped with the App that uses it.

    app.app = clap_app!(oasis =>
        (about: crate_description!())
        (version: version_str)
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
            (@arg verbose: +multiple -v --verbose "Increase verbosity")
            (@arg quiet: +multiple -q --quiet "Decrease verbosity")
            (@arg debug: --debug "Build without optimizations")
            (@arg profile: -p --profile default_value[local]
                "Set testing profile. Run `oasis config profile` \nto list available profiles.")
            (@arg SERVICE: +multiple "Specify which service(s) to test")
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
            (about: "Deploy services to the Oasis blockchain")
            (@arg verbose: +multiple -v --verbose "Increase verbosity")
            (@arg quiet: +multiple -q --quiet "Decrease verbosity")
            (@arg profile: -p --profile default_value[default]
                "Set testing profile. Run `oasis config profile` \nto list available profiles.")
            (@arg deployer_args: +raw "Args to pass to language-specific deployment tool")
        )
        (@subcommand config =>
            (about: "View and edit configuration options")
            (@arg KEY: +required "The configuration key to set")
            (@arg VALUE: "The new configuration value")
        )
        (@subcommand upload_metrics => (@setting Hidden))
        (@subcommand gen_completions => (@setting Hidden))
    )
    .subcommand(
        // this is here because the macro doesn't support "-" in names
        clap::SubCommand::with_name("set-toolchain")
            .about("Set the Oasis toolchain version")
            .after_help(help::SET_TOOLCHAIN)
            .arg(
                clap::Arg::with_name("VERSION")
                    .takes_value(true)
                    .required(true),
            ),
    );

    app
}

pub fn gen_completions() -> Result<(), failure::Error> {
    do_gen_completions(clap::Shell::Zsh, "_oasis")?;
    do_gen_completions(clap::Shell::Bash, "completions.sh")?;
    Ok(())
}

fn do_gen_completions(
    shell: clap::Shell,
    completions_file: &'static str,
) -> Result<(), failure::Error> {
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .open(crate::oasis_dir!(data)?.join(completions_file))?;
    build_app().gen_completions_to("oasis", shell, &mut f);
    Ok(())
}

impl<'a, 'b> std::ops::Deref for App<'a, 'b> {
    type Target = clap::App<'a, 'b>;

    fn deref(&self) -> &Self::Target {
        &self.app
    }
}

impl<'a, 'b> std::ops::DerefMut for App<'a, 'b> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.app
    }
}

impl<'a, 'b> App<'a, 'b> {
    fn with_version() -> Self {
        let mut version = String::with_capacity(20);
        version.push_str(crate_version!());
        if let Ok(r) = toolchain::installed_release() {
            version.push_str(" (toolchain ");
            version.push_str(r.name());
            version.push_str(")\n");
        }
        Self {
            version,
            app: clap::App::new(""),
        }
    }

    pub fn get_matches(self) -> clap::ArgMatches<'a> {
        self.app.get_matches()
    }
}
