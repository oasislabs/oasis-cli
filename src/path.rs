use std::path::PathBuf;

pub trait Provider {
    fn config_dir(&self) -> Option<PathBuf>;
}

pub struct SysProvider {}

impl Provider for SysProvider {
    fn config_dir(&self) -> Option<PathBuf> {
        dirs::config_dir()
    }
}
