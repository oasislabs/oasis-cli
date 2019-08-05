//! The functions in this module are based on those from the `dirs` crate except they always
//! return directories according to the XDG specification even if on a non-Linux OS.

use std::{env, path::PathBuf};

pub fn is_absolute_path<S: Into<std::ffi::OsString>>(path: S) -> Option<PathBuf> {
    let path = PathBuf::from(path.into());
    if path.is_absolute() {
        Some(path)
    } else {
        None
    }
}

pub fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
}

pub fn config_dir() -> Option<PathBuf> {
    env::var_os("XDG_CONFIG_HOME")
        .and_then(is_absolute_path)
        .or_else(|| home_dir().map(|h| h.join(".config")))
}

pub fn data_dir() -> Option<PathBuf> {
    env::var_os("XDG_DATA_HOME")
        .and_then(is_absolute_path)
        .or_else(|| home_dir().map(|h| h.join(".local/share")))
}
