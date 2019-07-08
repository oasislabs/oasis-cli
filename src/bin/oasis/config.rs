pub struct Logging {
    pub path_stdout: String,
    pub path_stderr: String,
    pub enabled: bool,
}

pub struct Config {
    pub logging: Logging,
}
