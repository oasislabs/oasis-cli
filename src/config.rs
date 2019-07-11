use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    io::{self, BufReader, Read as _, Write as _},
    path::{Path, PathBuf},
};

use crate::{dialogue, error::Error, path};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
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
        Logging {
            id: generate_uuid(),
            path_stdout: PathBuf::new(),
            path_stderr: PathBuf::new(),
            enabled: false,
            dir: PathBuf::new(),
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

struct DefaultOpts {
    telemetry_enabled: bool,
    local_private_key: String,
    path_provider: Option<Box<dyn path::Provider>>,
}

fn generate_uuid() -> String {
    let mut buf = [0u8; uuid::adapter::Hyphenated::LENGTH];
    uuid::Uuid::new_v4()
        .to_hyphenated()
        .encode_lower(&mut buf[..])
        .to_owned()
}

impl Config {
    fn default_with_options(options: DefaultOpts) -> Self {
        let path_provider = match options.path_provider {
            Some(path_provider) => path_provider,
            None => box path::SysProvider {},
        };

        let mut config = Config::default();

        if options.telemetry_enabled {
            config.telemetry.enabled = true;
            config.logging.enabled = true;
        }

        let log_dir = match path_provider.config_dir() {
            None => PathBuf::new(),
            Some(config_dir) => config_dir.join("oasis").join("log"),
        };

        config.logging.dir = log_dir;

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

    fn present_dialogue_for_opts() -> Result<DefaultOpts, failure::Error> {
        dialogue::introduction();

        let telemetry_enabled =
            dialogue::confirm("Would like to help us by providing telemetry data?", false)?;
        let local_private_key =
            dialogue::ask_string("What private key would you like to use for local deployments?")?;

        Ok(DefaultOpts {
            telemetry_enabled,
            local_private_key,
            path_provider: None,
        })
    }

    fn write_to_file_with_dialogue(path: &Path) -> Result<(), failure::Error> {
        let opts = Self::present_dialogue_for_opts()?;
        let config = Self::default_with_options(opts);
        let file = OpenOptions::new().write(true).create(true).open(path)?;
        let mut writer = io::BufWriter::new(file);
        let content = toml::to_string_pretty(&config)?;
        writer.write_all(&content.into_bytes())?;
        info!("configuration file {} has been generated. Edit the configuration file in case modifications are required.", path.to_str().unwrap());
        Ok(())
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
            Self::write_to_file_with_dialogue(path)?;
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

#[cfg(test)]
mod tests {

    use crate::{
        config::{Config, DefaultOpts, Endpoint},
        path::Provider as PathProvider,
    };
    use std::path::PathBuf;

    pub struct CustomProvider {
        pub config_dir: Option<PathBuf>,
    }

    impl PathProvider for CustomProvider {
        fn config_dir(&self) -> Option<PathBuf> {
            match &self.config_dir {
                Some(config_dir) => Some(config_dir.clone()),
                None => None,
            }
        }
    }

    #[test]
    fn test_defaults_with_options_telemetry_enabled() -> Result<(), failure::Error> {
        let config = Config::default_with_options(DefaultOpts {
            telemetry_enabled: true,
            local_private_key: String::from("1234"),
            path_provider: Some(box CustomProvider {
                config_dir: Some(PathBuf::from("/config")),
            }),
        });

        assert_eq!(PathBuf::from(""), config.logging.path_stdout);
        assert_eq!(PathBuf::from(""), config.logging.path_stderr);
        assert_eq!(PathBuf::from("/config/oasis/log"), config.logging.dir);
        assert_eq!(config.logging.id.len() == 36, true);
        assert_eq!(true, config.logging.enabled);
        assert_eq!(
            "https://gollum.devnet2.oasiscloud.io/",
            config.telemetry.endpoint
        );
        assert_eq!(true, config.telemetry.enabled);
        assert_eq!(50, config.telemetry.min_files);

        assert_eq!(2, config.profiles.len());
        let local = config.profiles.get("local").unwrap();
        assert_eq!("1234", local.private_key);
        assert_eq!("http://localhost:8546/", local.endpoint);
        assert_eq!(Endpoint::Local, local.endpoint_type);

        let default = config.profiles.get("default").unwrap();
        assert_eq!("", default.private_key);
        assert_eq!("https://gateway.devnet.oasiscloud.io", default.endpoint);
        assert_eq!(Endpoint::Remote, default.endpoint_type);
        Ok(())
    }

    #[test]
    fn test_defaults_with_options_telemetry_disabled() -> Result<(), failure::Error> {
        let config = Config::default_with_options(DefaultOpts {
            telemetry_enabled: false,
            local_private_key: String::from("1234"),
            path_provider: Some(box CustomProvider {
                config_dir: Some(PathBuf::from("/config")),
            }),
        });

        assert_eq!(PathBuf::from(""), config.logging.path_stdout);
        assert_eq!(PathBuf::from(""), config.logging.path_stderr);
        assert_eq!(PathBuf::from("/config/oasis/log"), config.logging.dir);
        assert_eq!(config.logging.id.len() == 36, true);
        assert_eq!(false, config.logging.enabled);
        assert_eq!(
            "https://gollum.devnet2.oasiscloud.io/",
            config.telemetry.endpoint
        );
        assert_eq!(false, config.telemetry.enabled);
        assert_eq!(50, config.telemetry.min_files);

        assert_eq!(2, config.profiles.len());
        let local = config.profiles.get("local").unwrap();
        assert_eq!("1234", local.private_key);
        assert_eq!("http://localhost:8546/", local.endpoint);
        assert_eq!(Endpoint::Local, local.endpoint_type);

        let default = config.profiles.get("default").unwrap();
        assert_eq!("", default.private_key);
        assert_eq!("https://gateway.devnet.oasiscloud.io", default.endpoint);
        assert_eq!(Endpoint::Remote, default.endpoint_type);
        Ok(())
    }
}
