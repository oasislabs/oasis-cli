use std::collections::HashMap;

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
    pub default: String,
}

#[derive(Clone, Debug)]
pub struct Logging {
    pub path_stdout: String,
    pub path_stderr: String,
    pub enabled: bool,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub profiles: Profile,
    pub logging: Logging,
}

impl Config {
    fn load() -> Result<Self, failure::Error> {
        
    }
}
