use std::{
    fmt, fs,
    os::unix::fs::PermissionsExt as _,
    path::{Path, PathBuf},
    str::FromStr,
};

use failure::format_err;
use reqwest::Url;

use crate::{
    dialogue, emit,
    error::{Error, ProfileError, ProfileErrorKind},
};

pub struct Config {
    doc: toml_edit::Document,
    dirty: bool,
}

macro_rules! default_gateway_url {
    () => {
        "https://gateway.devnet.oasiscloud.io"
    };
}
pub static DEFAULT_GATEWAY_URL: &str = default_gateway_url!();

const MNEMONIC_PHRASE_LEN: usize = 12;
const PRIVATE_KEY_BYTES: usize = 32;
const API_TOKEN_BYTES: usize = 32 + std::mem::size_of::<u32>();

macro_rules! profile_config_help {
    () => {
        r#"Available options are:

    gateway      URL of the developer or Web3  gateway used for testing/deployment.

    credential   The API token or private key/mnemonic used to authenticate to the
                 developer or Web3 gateway, respectively.
"#
    };
}

#[rustfmt::skip]
macro_rules! default_config_toml {
    () => {
        concat! {
r#"[profile.default]
gateway = ""#, default_gateway_url!(), r#""

[profile.local]
gateway = "ws://localhost:8546"  # web3
credential = "range drive remove bleak mule satisfy mandate east lion minimum unfold ready"

[telemetry]
enabled = false"#
        }
    };
}

impl Default for Config {
    fn default() -> Self {
        Self {
            doc: toml_edit::Document::from_str(default_config_toml!()).unwrap(),
            dirty: true,
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
        if !Self::skip_generate() && !self.dirty {
            self.write_to_file(Self::default_path()?)
        } else {
            Ok(())
        }
    }

    pub fn get(&self, key: &str) -> Option<String> {
        use toml_edit::{Item, Value};

        emit!(cmd.config.get, { "key": key });

        let mut itm = &self.doc.root;
        for k in key.split('.') {
            itm = match itm.as_table().and_then(|t| t.get(k)) {
                Some(Item::None) | None => return None,
                Some(itm) => itm,
            }
        }

        Some(match itm {
            Item::Value(v) => match &v {
                Value::Integer(repr) => repr.value().to_string(),
                Value::String(repr) => repr.value().to_string(),
                Value::Float(repr) => repr.value().to_string(),
                Value::Boolean(repr) => repr.value().to_string(),
                Value::DateTime(repr) => repr.value().to_string(),
                Value::Array(array) => array.to_string(),
                Value::InlineTable(table) => table.to_string(),
            },
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

        let mut key_comps = key.split('.');

        match key_comps.next() {
            Some("profile") => {
                let profile_name = match key_comps.next() {
                    Some(name) => name,
                    None => {
                        return Err(format_err!(
                            "missing profile name in `profile.<name>.<key>`.",
                        ))
                    }
                };
                let profile = match self
                    .doc
                    .as_table_mut()
                    .entry("profile")
                    .as_table_mut()
                    .map(|ps| ps.entry(profile_name))
                {
                    Some(toml_edit::Item::Table(profile)) => profile,
                    _ => {
                        return Err(ProfileError {
                            name: profile_name.to_string(),
                            kind: ProfileErrorKind::MissingProfile,
                        }
                        .into())
                    }
                };
                let profile_key = key_comps.next();
                if let Some(extra_comp) = key_comps.next() {
                    return Err(format_err!(
                        "unknown profile configuration subkey `{}`",
                        extra_comp
                    ));
                }
                let value = Self::read_value(value);
                let canon_value = match profile_key {
                    Some("credential") => Credential::from_str(&value)
                        .map_err(|e| ProfileError {
                            name: profile_name.to_string(),
                            kind: ProfileErrorKind::InvalidKey("credential", e.to_string()),
                        })?
                        .to_string(),
                    Some("gateway") => parse_gateway_url(&value)
                        .map_err(|e| ProfileError {
                            name: profile_name.to_string(),
                            kind: ProfileErrorKind::InvalidKey("gateway", e.to_string()),
                        })?
                        .to_string(),
                    Some(key) => {
                        return Err(format_err!(
                            "unknown profile configuration key `{}`.\n\n{}",
                            key,
                            profile_config_help!()
                        ));
                    }
                    None => {
                        return Err(format_err!(
                            "missing profile configuration key in `profile.{}.<key>`.\n\n{}",
                            profile_name,
                            profile_config_help!()
                        ));
                    }
                };
                *profile.entry(profile_key.unwrap()) = toml_edit::value(canon_value);
            }
            Some("telemetry") => {
                let telemetry_key = key_comps.next();
                if let Some(extra_comp) = key_comps.next() {
                    return Err(format_err!(
                        "unknown telemetry configuration subkey `{}`.",
                        extra_comp
                    ));
                }
                match telemetry_key {
                    Some("enabled") => self.enable_telemetry(value.parse()?),
                    Some("user_id") => {
                        return Err(format_err!(
                            "we'd prefer if you didn't modify `user_id`. \
                             If you feel strongly\nabout it, you can edit \
                             the config file directly."
                        ))
                    }
                    _ => {
                        return Err(format_err!(
                            "unknown configuration option: `{}`. Available options are `enabled`.",
                            key
                        ))
                    }
                }
            }
            Some(key) => return Err(format_err!("unknown configuration option: `{}`", key)),
            None => {
                return Err(format_err!(
                    "available configuration options are: `profile`, `telemetry`",
                ))
            }
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

    pub fn profile(&self, profile_name: &str) -> Result<Profile, ProfileError> {
        Profile::try_from_table(profile_name, self.profile_raw(profile_name))
    }

    pub fn profile_raw(&self, profile_name: &str) -> Option<&toml_edit::Table> {
        self.doc
            .as_table()
            .get("profile")
            .and_then(|t| t.as_table())
            .and_then(|t| t.get(profile_name))
            .and_then(|t| t.as_table())
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
        Ok(Self { doc, dirty: false })
    }

    fn write_to_file(&self, path: impl AsRef<Path>) -> Result<(), failure::Error> {
        fs::write(&path, self.doc.to_string_in_original_order())?;

        let mut perms = fs::metadata(&path)?.permissions();
        perms.set_mode(0o600 /* o+rw */);
        fs::set_permissions(&path, perms)?;

        Ok(())
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

    fn read_value(value: &str) -> String {
        if value == "-" {
            let mut value = String::new();
            std::io::stdin().read_line(&mut value).unwrap_or_default();
            value.pop(); // remove the '\n', if any
            value
        } else {
            value.to_string()
        }
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
    pub gateway: Url,
    pub credential: Credential,
}

pub enum Credential {
    Mnemonic(String),
    PrivateKey(String),
    ApiToken(String),
}

impl fmt::Display for Credential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Credential::*;
        f.write_str(match self {
            Mnemonic(s) | PrivateKey(s) | ApiToken(s) => s,
        })
    }
}

impl FromStr for Credential {
    type Err = failure::Error;

    fn from_str(mut s: &str) -> Result<Self, Self::Err> {
        if s.starts_with("0x") {
            s = &s[2..];
        }
        if let Ok(key_bytes) = hex::decode(s) {
            if key_bytes.len() == PRIVATE_KEY_BYTES {
                return Ok(Credential::PrivateKey(s.to_string()));
            };
        } else if s.split(' ').count() == MNEMONIC_PHRASE_LEN {
            return Ok(Credential::Mnemonic(s.to_lowercase()));
        } else if let Ok(tok_bytes) = base64::decode(s) {
            if tok_bytes.len() == API_TOKEN_BYTES {
                return Ok(Credential::ApiToken(s.to_string()));
            }
        }
        Err(format_err!("must be a private key, mnemonic, or API token"))
    }
}

impl Profile {
    fn try_from_table(
        profile_name: &str,
        profile_tab: Option<&toml_edit::Table>,
    ) -> Result<Self, ProfileError> {
        macro_rules! err {
            (missing) => {
                ProfileError {
                    name: profile_name.to_string(),
                    kind: ProfileErrorKind::MissingProfile,
                }
            };
            ($key:expr, missing) => {
                ProfileError {
                    name: profile_name.to_string(),
                    kind: ProfileErrorKind::MissingKey($key),
                }
            };
            ($key:expr, $cause:expr) => {
                ProfileError {
                    name: profile_name.to_string(),
                    kind: ProfileErrorKind::InvalidKey($key, $cause.to_string()),
                }
            };
        }

        let profile = match profile_tab {
            Some(tab) => tab,
            None => return Err(err!(missing)),
        };
        Ok(Self {
            gateway: profile
                .get("gateway")
                .and_then(|gw| gw.as_str())
                .ok_or_else(|| err!("gateway", missing))
                .and_then(|gw| parse_gateway_url(gw).map_err(|e| err!("gateway", e)))?,
            credential: Credential::from_str(
                profile
                    .get("credential")
                    .and_then(|c| c.as_str())
                    .ok_or_else(|| err!("credential", missing))?,
            )
            .map_err(|e| err!("credential", e))?,
        })
    }
}

fn parse_gateway_url(url_str: &str) -> Result<Url, failure::Error> {
    let url = Url::parse(url_str)?;
    if !url.has_host() {
        return Err(format_err!("URL must specify a domain"));
    }
    match url.scheme() {
        "ws" | "wss" | "http" | "https" => {}
        scheme => {
            return Err(format_err!(
                "invalid URL scheme `{}`. Must be http(s) or ws(s).",
                scheme
            ));
        }
    }

    Ok(url)
}
