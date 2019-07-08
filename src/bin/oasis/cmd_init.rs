use crate::{
    command::{run_cmd, Verbosity},
    config::Config,
};

/// Creates an Oasis project in a directory.
pub fn init(config: &Config, dir: &str, project_type: &str) -> Result<(), failure::Error> {
    match project_type {
        "rust" => init_rust(config, dir),
        _ => Err(failure::format_err!(
            "Unknown project type: `{}`",
            project_type
        )),
    }
}

fn init_rust(config: &Config, dir: &str) -> Result<(), failure::Error> {
    run_cmd(config, "cargo", &["init", "--bin", dir], Verbosity::Silent)?;
    // TODO: should clone starter repo with `.cargo/config` and whatnot
    // also ensure that compiler plugin is insstalled
    Ok(())
}
