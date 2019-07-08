#[derive(Clone, Debug)]
pub struct Logging {
    pub path_stdout: String,
    pub path_stderr: String,
    pub enabled: bool,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub logging: Logging,
}
