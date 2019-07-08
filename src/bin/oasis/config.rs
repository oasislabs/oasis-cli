use std::collections::HashMap;
use std::{path, fs, io};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct Wallet {
    pub private_key: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Profile {
    pub name: String,
    pub wallet: Wallet,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Profiles {
    pub profiles: HashMap<String, Profile>,
    pub default: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Logging {
    pub path_stdout: path::PathBuf,
    pub path_stderr: path::PathBuf,
    pub dir: path::PathBuf,
    pub enabled: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub timestamp: i64,
    pub id: u64,
    pub logging: Logging,
    pub profiles: Profiles,
}

impl Config {
    pub fn default() -> Self {
        let id = rand::random::<u64>();
        let timestamp = chrono::Utc::now().timestamp();

        Config{
            id: id,
            timestamp: timestamp,
            logging: Logging {
                path_stdout: path::PathBuf::new(),
                path_stderr: path::PathBuf::new(),
                dir: path::PathBuf::new(),
                enabled: false,
            },
            profiles: Profiles{
                profiles: HashMap::new(),
                default: None,
            }
        }
    }

    fn read_config(file: fs::File) -> Result<Self, failure::Error> {

    }

    pub fn load(path: &str) -> Result<Self, failure::Error> {
        let res = fs::OpenOptions::new()
            .read(true)
            .open(path::Path::new(path));

        match res {
            Ok(file) => Config::read_config(file),
            Err(err) => {
                match err.kind() {
                    io::ErrorKind::NotFound => {
                        println!("WARN: no configuration file...");
                        Ok(Config::default())
                    }
                    _ => return Err(failure::format_err!("failed to read config file `{}` with error `{}`",
                    path, err.to_string()))
                }
            },
        }
    }
}
