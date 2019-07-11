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
        dirs::config_dir()
    }
}
