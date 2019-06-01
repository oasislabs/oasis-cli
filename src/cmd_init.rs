use crate::utils::{run_cmd, Verbosity};

/// Creates an Oasis project in a directory.
pub fn init(dir: &str, project_type: &str) -> Result<(), failure::Error> {
    match project_type {
        "rust" => init_rust(dir),
        _ => Err(failure::format_err!(
            "Unknown project type: `{}`",
            project_type
        )),
    }
}

fn init_rust(dir: &str) -> Result<(), failure::Error> {
    run_cmd("cargo", &["init", "--bin", dir], Verbosity::Silent)?;
    // TODO: should clone starter repo with `.cargo/config` and whatnot
    // also ensure that idl-gen is insstalled
    Ok(())
}
