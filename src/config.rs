use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use crate::{dialogue, emit, error::Error};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(alias = "profile", default)]
    pub profiles: HashMap<String, Profile>,
    #[serde(default)]
    pub telemetry: Telemetry,
}

impl Default for Config {
    fn default() -> Self {
        let mut profiles = HashMap::new();
        profiles.insert(
            "profile.local".to_string(),
            Profile {
                mnemonic: Some(
                    "range drive remove bleak mule satisfy mandate east lion minimum unfold ready"
                        .to_string(),
                ),
                private_key: None,
                endpoint: "ws://localhost:8546".to_string(),
            },
        );

        profiles.insert(
            "profile.default".to_string(),
            Profile {
                mnemonic: None,
                private_key: None,
                endpoint: "https://gateway.devnet.oasiscloud.io".to_string(),
            },
        );

        Config {
            profiles,
            telemetry: Telemetry::default(),
        }
    }
}

impl Config {
    pub fn load() -> Result<Self, failure::Error> {
        let config_path = Self::default_path()?;
        if !config_path.exists() {
            Self::generate(&config_path)
        } else {
            debug!("loading configuration from `{}`", config_path.display());
            Self::read_from_file(&config_path)
        }
    }

    pub fn save(&self) -> Result<(), failure::Error> {
        self.write_to_file(Self::default_path()?)
    }

    pub fn enable_telemetry(&mut self, enabled: bool) {
        self.telemetry.enabled = enabled;
    }

    pub fn default_path() -> Result<PathBuf, failure::Error> {
        let mut config_path = crate::oasis_dir!(config)?;
        config_path.push("config.toml");
        Ok(config_path)
    }

    pub fn edit_profile(
        &mut self,
        profile_name: &str,
        key: &str,
        value: &str,
    ) -> Result<(), failure::Error> {
        let mut profile = match self.profiles.get_mut(profile_name) {
            Some(profile) => profile,
            None => return Err(failure::format_err!("no profile named `{}`", profile_name)),
        };
        emit!(cmd.config, { "key": key });
        match key {
            "mnemonic" => {
                profile.mnemonic = Some(value.to_string());
                profile.private_key = None;
            }
            "private_key" => {
                profile.private_key = Some(value.to_string());
                profile.mnemonic = None;
            }
            "endpoint" => {
                profile.endpoint = value.to_string();
            }
            _ => return Err(failure::format_err!("unknown profile parameter: `{}`", key)),
        }
        Ok(())
    }
}

impl Config {
    fn generate(path: &Path) -> Result<Self, failure::Error> {
        let mut config = Self::default();

        dialogue::introduction();
        config.telemetry.enabled = match crate::oasis_dir!(data) {
            Ok(telemetry_dir) => dialogue::prompt_telemetry(&telemetry_dir)?,
            Err(_) => false,
        };

        config.write_to_file(path)?;

        println!("Created new configuration file at `{}`.\n", path.display());

        std::thread::sleep(std::time::Duration::from_millis(700));
        // ^ give the user some time to ack the creation of the new file

        Ok(config)
    }

    fn read_from_file(path: &Path) -> Result<Self, failure::Error> {
        let config_bytes = fs::read(path)
            .map_err(|err| Error::ReadFile(path.display().to_string(), err.to_string()))?;
        Ok(toml::from_slice(&config_bytes)
            .map_err(|err| Error::ConfigParse(path.display().to_string(), err.to_string()))?)
    }

    fn write_to_file(&self, path: impl AsRef<Path>) -> Result<(), failure::Error> {
        Ok(std::fs::write(path, toml::to_string_pretty(self)?)?)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Profile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mnemonic: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private_key: Option<String>,
    pub endpoint: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Telemetry {
    pub enabled: bool,
    pub user_id: String,
}

impl Default for Telemetry {
    fn default() -> Self {
        let mut user_id = Vec::with_capacity(uuid::adapter::Hyphenated::LENGTH);
        unsafe { user_id.set_len(user_id.capacity()) };
        let _ = uuid::Uuid::new_v4()
            .to_hyphenated()
            .encode_lower(&mut user_id);

        Telemetry {
            enabled: false,
            user_id: String::from_utf8(user_id).unwrap(),
        }
    }
}
