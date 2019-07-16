pub struct DeployOptions {
    pub dashboard_mode: bool,
}

impl DeployOptions {
    pub fn new(m: &clap::ArgMatches) -> Result<Self, failure::Error> {
        Ok(Self {
            dashboard_mode: m.is_present("dashboard"),
        })
    }
}

pub fn deploy(opts: DeployOptions) -> Result<(), failure::Error> {
    if !opts.dashboard_mode {
        return Err(failure::format_err!(
            "deploy only supports execution with option --dashboard"
        ));
    }

    webbrowser::open("https://dashboard.oasiscloud.io/newcontract")
        .map(|_| {})
        .map_err(|e| {
            failure::format_err!(
                "failed to open browser for contract deployment `{}`",
                e.to_string()
            )
        })
}
