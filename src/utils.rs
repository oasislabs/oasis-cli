enum ProjectType {
    Rust(Option<cargo_toml::Manifest>),
    Unknown,
}

fn detect_project_type() -> ProjectType {
    let cargo_toml = std::path::Path::new("Cargo.toml");
    if cargo_toml.exists() {
        let mut manifest = cargo_toml::Manifest::from_path(cargo_toml).unwrap();
        manifest
            .complete_from_path(std::path::Path::new("."))
            .unwrap();
        ProjectType::Rust(Some(manifest))
    } else {
        ProjectType::Unknown
    }
}

fn run_cmd<S: AsRef<std::ffi::OsStr>>(
    name: &str,
    args: impl IntoIterator<Item = S>,
    envs: Option<Vec<(S, S)>>,
    verbosity: i64,
) -> Result<i32, String> {
    use std::process::{Command, Stdio};
    let (stdout, stderr) = if verbosity < 0 {
        (Stdio::null(), Stdio::null())
    } else if verbosity == 0 {
        (Stdio::null(), Stdio::inherit())
    } else {
        (Stdio::inherit(), Stdio::inherit())
    };
    let mut cmd = Command::new(name);
    cmd.stdout(stdout).stderr(stderr).args(args);
    if let Some(envs) = envs {
        cmd.envs(envs.into_iter());
    }
    let status = cmd
        .spawn()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => format!(
                "Could not run {}, please make sure it is in your PATH.",
                name
            ),
            _ => e.to_string(),
        })?
        .wait()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(0)
    } else {
        Ok(status.code().unwrap_or(-1))
    }
}

macro_rules! run_cmd {
    ($($arg:expr),+) => {
        match run_cmd($($arg),+) {
            Err(err) => {
                eprintln!("oasis-build failed. {}", err);
                return -1
            }
            Ok(status_code) if status_code != 0 => return status_code,
            _ => ()
        }
    }
}
