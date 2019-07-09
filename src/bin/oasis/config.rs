use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    io::{self, Read as _, Write as _},
    path,
};

use crate::error::Error;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Profile {
    pub private_key: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Logging {
    #[serde(skip)]
    pub path_stdout: path::PathBuf,
    #[serde(skip)]
    pub path_stderr: path::PathBuf,
    pub dir: path::PathBuf,
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(skip)]
    pub timestamp: i64,
    #[serde(skip)]
    pub id: u64,
    pub logging: Logging,
    #[serde(alias = "profile")]
    pub profiles: HashMap<String, Profile>,
}

impl Config {
    fn read_config(file: fs::File) -> Result<Self, failure::Error> {
        let mut reader = io::BufReader::new(file);
        let mut content = String::new();
        reader.read_to_string(&mut content)?;
        let mut config: Config = toml::from_str(&content)?;

        config.id = rand::random::<u64>();
        config.timestamp = chrono::Utc::now().timestamp();

        config.logging.path_stdout = path::Path::new(&config.logging.dir)
            .join(format!("{}.{}.stdout", config.timestamp, config.id));
        config.logging.path_stderr = path::Path::new(&config.logging.dir)
            .join(format!("{}.{}.stderr", config.timestamp, config.id));
        Ok(config)
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
        let content = toml::to_string(&Config {
            id: 0,
            timestamp: 0,
            logging: Logging {
                path_stdout: path::PathBuf::new(),
                path_stderr: path::PathBuf::new(),
                enabled: false,
                dir: log_dir,
            },
            profiles,
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
