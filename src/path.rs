use std::path::PathBuf;

pub trait Provider {
    fn config_dir(&self) -> Option<PathBuf>;
}

pub struct SysProvider;

impl SysProvider {
    pub fn new() -> Self {
        SysProvider {}
    }
}

impl Provider for SysProvider {
    fn config_dir(&self) -> Option<PathBuf> {
        match std::env::var("OASIS_CONFIG_DIR"){
            Ok(s) => Some(PathBuf::from(&s)),
            Err(_) => dirs::config_dir(),
        }
    }
}
