use std::{
    fs,
    path::{Path, PathBuf},
    str::FromStr as _,
};

use failure::format_err;
use reqwest::Url;

use crate::{dialogue, emit, error::Error};

pub struct Config {
    doc: toml_edit::Document,
}

static DEFAULT_CONFIG_TOML: &str = r#"
[profile.default]
private_key = ""
endpoint = "https://gateway.devnet.oasiscloud.io"

[profile.local]
mnemonic = "range drive remove bleak mule satisfy mandate east lion minimum unfold ready"
endpoint = "ws://localhost:8546"

[telemetry]
enabled = false
"#;

impl Default for Config {
    fn default() -> Self {
        Self {
            doc: toml_edit::Document::from_str(DEFAULT_CONFIG_TOML).unwrap(),
        }
    }
}

impl Config {
    pub fn new() -> Self {
        let mut config = Self::default();
        *config.doc.as_table_mut().entry("telemetry") =
            toml_edit::Item::Table(Telemetry::new().into());
        config
    }

    pub fn load() -> Result<Self, failure::Error> {
        let config_path = Self::default_path()?;
        if !config_path.exists() {
            if !Self::skip_generate() {
                Self::generate(&config_path)
            } else {
                Ok(Self::default())
            }
        } else {
            debug!("loading configuration from `{}`", config_path.display());
            Self::read_from_file(&config_path)
        }
    }

    pub fn save(&self) -> Result<(), failure::Error> {
        if !Self::skip_generate() {
            self.write_to_file(Self::default_path()?)
        } else {
            Ok(())
        }
    }

    pub fn get(&self, key: &str) -> Option<String> {
        use toml_edit::Item;

        emit!(cmd.config.get, { "key": key });

        let mut itm = &self.doc.root;
        for k in key.split('.') {
            itm = match itm.as_table().and_then(|t| t.get(k)) {
                Some(Item::None) | None => return None,
                Some(itm) => itm,
            }
        }

        Some(match itm {
            Item::Value(v) => v.to_string(),
            Item::Table(t) => t.to_string(),
            Item::ArrayOfTables(ts) => ts
                .iter()
                .map(toml_edit::Table::to_string)
                .collect::<Vec<_>>()
                .join("\n"),
            Item::None => unreachable!(),
        })
    }

    pub fn edit(&mut self, key: &str, value: &str) -> Result<(), failure::Error> {
        emit!(cmd.config.edit, { "key": key });

        match key.split('.').collect::<Vec<&str>>().as_slice() {
            ["profile", profile_name, key] => {
                let profile = match self
                    .doc
                    .as_table_mut()
                    .entry("profile")
                    .as_table_mut()
                    .map(|ps| ps.entry(profile_name))
                {
                    Some(toml_edit::Item::Table(profile)) => profile,
                    _ => return Err(format_err!("No profile named `{}`", profile_name)),
                };
                match *key {
                    "mnemonic" => {
                        profile.remove("private_key");
                    }
                    "private_key" => {
                        profile.remove("mnemonic");
                    }
                    "endpoint" => (),
                    _ => {
                        return Err(format_err!(
                            "unknown configuration option: `{}`. \
                             Valid options are `mnemonic`, `private_key`, and `endpoint`.",
                            key
                        ))
                    }
                }
                *profile.entry(key) = toml_edit::value(value);
            }
            ["telemetry", key] => match *key {
                "enabled" => self.enable_telemetry(value.parse()?),
                "user_id" => {
                    return Err(format_err!(
                        "we'd prefer if you didn't modify `user_id`. \
                         If you feel strongly about it,\nyou can edit \
                         the config file directly."
                    ))
                }
                _ => {
                    return Err(format_err!(
                        "unknown configuration option: `{}`. Valid options are `enabled`.",
                        key
                    ))
                }
            },
            _ => return Err(format_err!("unknown configuration option: `{}`", key)),
        }

        Ok(())
    }

    pub fn telemetry(&self) -> Telemetry {
        self.doc
            .as_table()
            .get("telemetry")
            .and_then(|t| t.as_table())
            .map(|t| t.into())
            .unwrap_or_else(|| Telemetry {
                enabled: false,
                user_id: "".to_string(),
            })
    }

    pub fn profile(&self, profile_name: &str) -> Result<Profile, failure::Error> {
        Profile::try_from_table(
            self.doc
                .as_table()
                .get("profile")
                .and_then(|t| t.as_table())
                .and_then(|t| t.get(profile_name))
                .and_then(|t| t.as_table())
                .ok_or_else(|| format_err!("`profile.{}` does not exist", profile_name))?,
        )
        .map_err(|ppe| ppe.into_error(profile_name))
    }
}

impl Config {
    fn generate(path: &Path) -> Result<Self, failure::Error> {
        let mut config = Self::new();

        dialogue::introduction();
        config.enable_telemetry(match crate::oasis_dir!(data) {
            Ok(telemetry_dir) => dialogue::prompt_telemetry(&telemetry_dir)?,
            Err(_) => false,
        });

        config.write_to_file(path)?;

        println!("Created new configuration file at `{}`.\n", path.display());

        std::thread::sleep(std::time::Duration::from_millis(600));
        // ^ give the user some time to ack the creation of the new file

        Ok(config)
    }

    fn default_path() -> Result<PathBuf, failure::Error> {
        let mut config_path = crate::oasis_dir!(config)?;
        config_path.push("config.toml");
        Ok(config_path)
    }

    fn read_from_file(path: &Path) -> Result<Self, failure::Error> {
        let config_string = fs::read_to_string(path)
            .map_err(|err| Error::ReadFile(path.display().to_string(), err.to_string()))?;
        let doc = toml_edit::Document::from_str(&config_string)
            .map_err(|err| Error::ConfigParse(path.display().to_string(), err.to_string()))?;
        Ok(Self { doc })
    }

    fn write_to_file(&self, path: impl AsRef<Path>) -> Result<(), failure::Error> {
        Ok(std::fs::write(
            path,
            self.doc.to_string_in_original_order(),
        )?)
    }

    fn skip_generate() -> bool {
        std::env::var("OASIS_SKIP_GENERATE_CONFIG")
            .map(|v| v == "1")
            .unwrap_or_default()
    }

    fn enable_telemetry(&mut self, enabled: bool) {
        *self
            .doc
            .as_table_mut()
            .entry("telemetry")
            .or_insert(toml_edit::Item::Table(Telemetry::default().into()))
            .as_table_mut()
            .unwrap()
            .entry("enabled") = toml_edit::value(enabled);
    }
}

#[derive(Default)]
pub struct Telemetry {
    pub enabled: bool,
    pub user_id: String,
}

impl Telemetry {
    fn new() -> Self {
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

impl<T: std::borrow::Borrow<toml_edit::Table>> From<T> for Telemetry {
    fn from(tab: T) -> Self {
        Self {
            enabled: tab
                .borrow()
                .get("enabled")
                .and_then(|e| e.as_bool())
                .unwrap_or_default(),
            user_id: tab
                .borrow()
                .get("user_id")
                .and_then(|u| u.as_str())
                .map(|u| u.to_string())
                .unwrap_or_default(),
        }
    }
}

impl From<Telemetry> for toml_edit::Table {
    fn from(tlm: Telemetry) -> Self {
        let mut tab = Self::new();
        *tab.entry("enabled") = toml_edit::value(tlm.enabled);
        *tab.entry("user_id") = toml_edit::value(tlm.user_id);
        tab
    }
}

pub struct Profile {
    pub endpoint: Url,
    pub secret: Secret,
}

pub enum Secret {
    Mnemonic(String),
    Key(String),
}

impl Profile {
    fn try_from_table(tab: &toml_edit::Table) -> Result<Self, ProfileParseError> {
        let secret = match (tab.get("mnemonic"), tab.get("private_key")) {
            (Some(_), Some(_)) => Err(ProfileParseError {
                key: None,
                cause: "only one of `mnemonic` and `private_key` can be specified".to_string(),
            }),
            (Some(m), _) => m
                .as_str()
                .map(|m| Secret::Mnemonic(m.to_string()))
                .ok_or_else(|| ProfileParseError {
                    key: Some("mnemonic"),
                    cause: "value must be a string".to_string(),
                }),
            (_, Some(k)) => k
                .as_str()
                .map(|k| Secret::Key(k.to_string()))
                .ok_or_else(|| ProfileParseError {
                    key: Some("profile"),
                    cause: "value must be a string".to_string(),
                }),
            (None, None) => Err(ProfileParseError {
                key: None,
                cause: "one of `mnemonic` and `private_key` is required".to_string(),
            }),
        }?;
        Ok(Self {
            endpoint: tab
                .get("endpoint")
                .and_then(|ep| ep.as_str())
                .ok_or_else(|| ProfileParseError {
                    key: None,
                    cause: "`endpoint` is required".to_string(),
                })
                .and_then(|ep| {
                    Url::parse(ep).map_err(|e: reqwest::UrlError| ProfileParseError {
                        key: Some("endpoint"),
                        cause: e.to_string(),
                    })
                })?,
            secret,
        })
    }
}

pub struct ProfileParseError {
    key: Option<&'static str>,
    cause: String,
}

impl ProfileParseError {
    fn into_error(self, profile_name: &str) -> failure::Error {
        let subkey = match self.key {
            Some(k) => format!(".{}", k),
            None => "".to_string(),
        };
        format_err!(
            "`profile.{}{}` is invalid: {}",
            profile_name,
            subkey,
            self.cause
        )
    }
}
