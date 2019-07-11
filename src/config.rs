use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    io::{self, BufReader, Read as _, Write as _},
    path::{Path, PathBuf},
};

use crate::error::Error;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Endpoint {
    Local,
    Remote,
    Undefined,
}

impl From<String> for Endpoint {
    fn from(s: String) -> Endpoint {
        match s.as_ref() {
            "local" => Endpoint::Local,
            "remote" => Endpoint::Remote,
            _ => Endpoint::Undefined,
        }
    }
}

impl ToString for Endpoint {
    fn to_string(&self) -> String {
        match self {
            Endpoint::Local => "local".to_string(),
            Endpoint::Remote => "remote".to_string(),
            Endpoint::Undefined => "undefined".to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Profile {
    #[serde(default)]
    pub private_key: String,
    #[serde(default)]
    pub endpoint: String,
    #[serde(default = Endpoint::Undefined)]
    pub endpoint_type: Endpoint,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Telemetry {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub endpoint: String,
    #[serde(default)]
    pub min_files: usize,
}

impl Default for Telemetry {
    fn default() -> Self {
        Telemetry {
            enabled: false,
            endpoint: String::from("https://gollum.devnet2.oasiscloud.io/"),
            min_files: 50,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Logging {
    #[serde(skip)]
    pub path_stdout: PathBuf,
    #[serde(skip)]
    pub path_stderr: PathBuf,
    pub dir: PathBuf,
    pub enabled: bool,
    pub id: String,
}

impl Default for Logging {
    fn default() -> Logging {
        let dir = match dirs::config_dir() {
            None => PathBuf::new(),
            Some(config_dir) => config_dir.join("oasis").join("log"),
        };

        Logging {
            id: generate_uuid(),
            path_stdout: PathBuf::new(),
            path_stderr: PathBuf::new(),
            enabled: false,
            dir,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(skip)]
    pub timestamp: i64,
    #[serde(skip)]
    pub id: u64,
    #[serde(default)]
    pub logging: Logging,
    #[serde(alias = "profile")]
    pub profiles: HashMap<String, Profile>,
    #[serde(default)]
    pub telemetry: Telemetry,
}

impl Default for Config {
    fn default() -> Self {
        let mut profiles = HashMap::new();
        profiles.insert(
            "local".to_string(),
            Profile {
                private_key: String::new(),
                endpoint: String::from("http://localhost:8546/"),
                endpoint_type: Endpoint::Local,
            },
        );

        profiles.insert(
            "default".to_string(),
            Profile {
                private_key: String::new(),
                endpoint: String::from("https://gateway.devnet.oasiscloud.io"),
                endpoint_type: Endpoint::Remote,
            },
        );

        Config {
            id: rand::random(),
            timestamp: chrono::Utc::now().timestamp(),
            logging: Logging::default(),
            profiles,
            telemetry: Telemetry::default(),
        }
    }
}

fn generate_uuid() -> String {
    let mut buf = [0u8; uuid::adapter::Hyphenated::LENGTH];
    uuid::Uuid::new_v4()
        .to_hyphenated()
        .encode_lower(&mut buf[..])
        .to_owned()
}

struct DefaultOptions {
    telemetry_enabled: bool,
    local_private_key: String,
}

impl Config {
    fn default_with_options(options: DefaultOptions) -> Self {
        let mut config = Config::default();

        if options.telemetry_enabled {
            config.telemetry.enabled = true;
            config.logging.enabled = true;
        }

        // replace the existing local profile with the new
        // configuration
        let local_profile = config.profiles.get("local").unwrap();
        let new_local_profile = Profile {
            private_key: options.local_private_key,
            endpoint: local_profile.endpoint.clone(),
            endpoint_type: local_profile.endpoint_type,
        };

        config
            .profiles
            .insert("local".to_string(), new_local_profile);
        config
    }

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

    fn dialogue() -> Result<DefaultOptions, failure::Error> {
        println!("Welcome to Oasis Development Environment! My name is Oli and I can help you set up things.");
        println!("We will set up the configuration options that you will use");
        println!("");
        println!("We hope to collect telemetry data from the logging generated by user commands. This telemetry data");
        println!("will give us insights on what problems users encounter and be able to improve the software more quickly.");

        let telemetry_enabled =
            Config::ask_yn("Would like to help us by providing telemetry data?")?;
        let local_private_key =
            Config::ask_string("What private key would you like to use for local deployments?")?;

        Ok(DefaultOptions {
            telemetry_enabled,
            local_private_key,
        })
    }

    fn ask_string(question: &str) -> Result<String, failure::Error> {
        let mut s = String::new();

        println!("{}", question);
        let _ = io::stdin().read_line(&mut s)?;
        Ok(s.trim_end().to_string())
    }

    fn ask_yn(question: &str) -> Result<bool, failure::Error> {
        let mut s = String::new();

        loop {
            println!("{} [y/N]", question);
            let rbytes = io::stdin().read_line(&mut s)?;
            if rbytes != 2 {
                println!("please answer [y/N]");
                continue;
            }

            match s.as_ref() {
                "y\n" => return Ok(true),
                "N\n" => return Ok(false),
                _ => {
                    println!("please answer [y/N]");
                    continue;
                }
            }
        }
    }

    fn generate_with_defaults(path: &Path, opts: DefaultOptions) -> Result<(), failure::Error> {
        let config = Self::default_with_options(opts);
        let file = OpenOptions::new().write(true).create(true).open(path)?;
        let mut writer = io::BufWriter::new(file);
        let content = toml::to_string_pretty(&config)?;
        writer.write_all(&content.into_bytes())?;
        info!("configuration file {} has been generated. Edit the configuration file in case modifications are required.", path.to_str().unwrap());
        Ok(())
    }

    fn generate_with_dialogue(path: &Path) -> Result<(), failure::Error> {
        let opts = Config::dialogue()?;
        Self::generate_with_defaults(path, opts)
    }

    pub fn load(path: &Path) -> Result<Self, failure::Error> {
        debug!(
            "loading configuration file from path {}",
            path.to_str().unwrap()
        );

        if !path.exists() {
            info!(
                "no configuration file found. Generating configuration file {}",
                path.to_str().unwrap(),
            );
            Self::generate_with_dialogue(path)?;
        }

        let res = File::open(path);

        match res {
            Ok(file) => Config::read_config(file),
            Err(err) => {
                Err(Error::ConfigParse(path.to_str().unwrap().to_string(), err.to_string()).into())
            }
        }
    }
}
