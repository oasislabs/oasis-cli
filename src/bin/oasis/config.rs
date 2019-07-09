use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    io::{self, Read as _, Write as _},
    path,
};

use crate::error::Error;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Profile {
    pub private_key: String,
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

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LoggingParse {
    pub dir: path::PathBuf,
    pub enabled: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ConfigParse {
    pub logging: LoggingParse,
    pub profile: HashMap<String, Profile>,
}

impl Config {
    fn read_config(file: fs::File) -> Result<Self, failure::Error> {
        let mut reader = io::BufReader::new(file);
        let mut content = String::new();
        reader.read_to_string(&mut content)?;
        let config: ConfigParse = toml::from_str(&content)?;

        let id = rand::random::<u64>();
        let timestamp = chrono::Utc::now().timestamp();

        Ok(Config {
            id,
            timestamp,
            logging: Logging {
                path_stdout: path::Path::new(&config.logging.dir)
                    .join(format!("{}.{}.stdout", timestamp, id)),
                path_stderr: path::Path::new(&config.logging.dir)
                    .join(format!("{}.{}.stderr", timestamp, id)),
                dir: config.logging.dir,
                enabled: config.logging.enabled,
            },
            profiles: Profiles {
                default: if config.profile.contains_key("default") {
                    Some("default".to_string())
                } else {
                    None
                },
                profiles: config.profile,
            },
        })
    }

    fn generate(path: &str) -> Result<(), failure::Error> {
        let config_dir = match dirs::config_dir() {
            None => return Err(Error::ConfigDirNotFound.into()),
            Some(config_dir) => config_dir.to_str().unwrap().to_string(),
        };
        let log_dir = path::Path::new(&config_dir).join("oasis").join("log");

        let file = fs::OpenOptions::new().write(true).create(true).open(path)?;

        let mut profiles = HashMap::new();
        profiles.insert(
            "default".to_string(),
            Profile {
                private_key: String::new(),
            },
        );

        let mut writer = io::BufWriter::new(file);
        let content = toml::to_string(&ConfigParse {
            logging: LoggingParse {
                enabled: false,
                dir: log_dir,
            },
            profile: profiles,
        })?;
        writer.write_all(&content.into_bytes())?;
        Ok(())
    }

    pub fn load(path: &str) -> Result<Self, failure::Error> {
        let config_path = path::Path::new(path);

        if !config_path.exists() {
            info!(
                "no configuration file found. Generating configuration file {}",
                path
            );
            Config::generate(path)?;
        }

        let res = fs::OpenOptions::new().read(true).open(config_path);

        match res {
            Ok(file) => Config::read_config(file),
            Err(err) => Err(Error::ConfigParse(path.to_string(), err.to_string()).into()),
        }
    }
}
