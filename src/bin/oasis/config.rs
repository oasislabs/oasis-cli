use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Logging {
    pub path_stdout: PathBuf,
    pub path_stderr: PathBuf,
    pub dir: PathBuf,
    pub enabled: bool,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub timestamp: i64,
    pub id: u64,
    pub logging: Logging,
}
