#[derive(Clone, Debug)]
pub struct Logging {
    pub path_stdout: String,
    pub path_stderr: String,
    pub dir: String,
    pub enabled: bool,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub timestamp: i64,
    pub id: u64,
    pub logging: Logging,
}
