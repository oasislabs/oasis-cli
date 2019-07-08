use std::collections::HashMap;
use std::{path, fs, io};

#[derive(Clone, Debug)]
pub struct Wallet {
    pub private_key: String,
}

#[derive(Clone, Debug)]
pub struct Profile {
    pub wallet: Wallet,
}

#[derive(Clone, Debug)]
pub struct Profiles {
    pub profiles: HashMap<String, Profile>,
    pub default: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Logging {
    pub path_stdout: String,
    pub path_stderr: String,
    pub dir: String,
    pub enabled: bool,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub profiles: Profiles,
    pub timestamp: i64,
    pub id: u64,
    pub logging: Logging,
}

impl Config {
    pub fn default() -> Self {
        Config{
            logging: Logging {
                path_stdout: String::new(),
                path_stderr: String::new(),
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

    fn generate_config() -> Result<Self, failure::Error> {
        Ok(Config::default())
    }

    pub fn load(home: String) -> Result<Self, failure::Error> {
        let config_path = path::Path::new(&home).join(".oasis").join("config");
        let res = fs::OpenOptions::new()
            .read(true)
            .open(config_path);

        match res {
            Ok(file) => Config::read_config(file),
            Err(err) => {
                match err.kind() {
                    io::ErrorKind::NotFound => {
                        println!("WARN: failed to read config file, generating...");
                        Config::generate_config()
                    }
                    _ => return Err(failure::format_err!("failed to read config file `{}` with error `{}`",
                    config_path.to_str().unwrap(), err.to_string()))
                }
            },
        }
    }
}
