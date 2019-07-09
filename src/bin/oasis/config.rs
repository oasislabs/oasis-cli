use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    io::{self, BufReader, Read as _, Write as _},
    path::{Path, PathBuf},
};

use crate::error::Error;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Profile {
    pub private_key: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Logging {
    #[serde(skip)]
    pub path_stdout: PathBuf,
    #[serde(skip)]
    pub path_stderr: PathBuf,
    pub dir: PathBuf,
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

impl Default for Config {
    fn default() -> Self {
        let log_dir = match dirs::config_dir() {
            None => PathBuf::new(),
            Some(config_dir) => config_dir.join("oasis").join("log"),
        };
        let mut profiles = HashMap::new();
        profiles.insert(
            "default".to_string(),
            Profile {
                private_key: String::new(),
            },
        );

        Config {
            id: rand::random(),
            timestamp: chrono::Utc::now().timestamp(),
            logging: Logging {
                path_stdout: PathBuf::new(),
                path_stderr: PathBuf::new(),
                enabled: false,
                dir: log_dir,
            },
            profiles,
        }
    }
}

impl Config {
    fn generate_output_file_path(base: &PathBuf, ext: &str, timestamp: i64, id: u64) -> PathBuf {
        base.join(format!("{}.{}.{}", timestamp, id, ext))
    }

    fn read_config(file: File) -> Result<Self, failure::Error> {
        let mut reader = BufReader::new(file);
        let mut content = String::new();
        reader.read_to_string(&mut content)?;
        let mut config: Config = toml::from_str(&content)?;

        config.id = rand::random();
        config.timestamp = chrono::Utc::now().timestamp();

        config.logging.path_stdout = Self::generate_output_file_path(
            &config.logging.dir,
            "stdout",
            config.timestamp,
            config.id,
        );
        config.logging.path_stderr = Self::generate_output_file_path(
            &config.logging.dir,
            "stderr",
            config.timestamp,
            config.id,
        );
        Ok(config)
    }

    fn generate(path: &Path) -> Result<(), failure::Error> {
        let config = Self::default();
        let file = OpenOptions::new().write(true).create(true).open(path)?;
        let mut writer = io::BufWriter::new(file);
        let content = toml::to_string_pretty(&config)?;
        writer.write_all(&content.into_bytes())?;
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self, failure::Error> {
        let config_path = Path::new(path);

        if !config_path.exists() {
            info!(
                "no configuration file found. Generating configuration file {}",
                path.to_str().unwrap(),
            );
            Self::generate(path)?;
        }

        let res = OpenOptions::new().read(true).open(config_path);

        match res {
            Ok(file) => Config::read_config(file),
            Err(err) => {
                Err(Error::ConfigParse(path.to_str().unwrap().to_string(), err.to_string()).into())
            }
        }
    }
}
