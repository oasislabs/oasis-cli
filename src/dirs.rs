//! The functions in this module are based on those from the `dirs` crate except they always
//! return directories according to the XDG specification even if on a non-Linux OS.
//! The functions assume that the user has a home directory. Make sure to call `has_home_dir`
//! before using any of the panicking functions in this module.

use std::{env, path::PathBuf};

pub fn is_absolute_path<S: Into<std::ffi::OsString>>(path: S) -> Option<PathBuf> {
    let path = PathBuf::from(path.into());
    if path.is_absolute() {
        Some(path)
    } else {
        None
    }
}

pub fn has_home_dir() -> bool {
    env::var_os("HOME").is_some()
}

pub fn home_dir() -> PathBuf {
    PathBuf::from(env::var_os("HOME").unwrap())
}

pub fn config_dir() -> PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .and_then(is_absolute_path)
        .unwrap_or_else(|| home_dir().join(".config"))
}

pub fn data_dir() -> PathBuf {
    env::var_os("XDG_DATA_HOME")
        .and_then(is_absolute_path)
        .unwrap_or_else(|| home_dir().join(".local/share"))
}

pub fn cache_dir() -> PathBuf {
    env::var_os("XDG_CACHE_HOME")
        .and_then(is_absolute_path)
        .unwrap_or_else(|| home_dir().join(".cache"))
}

pub fn bin_dir() -> PathBuf {
    env::var_os("XDG_BIN_HOME")
        .and_then(is_absolute_path)
        .unwrap_or_else(|| {
            let mut d = data_dir();
            d.pop();
            d.push("bin");
            d
        })
}

#[macro_export]
macro_rules! ensure_dir {
    ($dir:ident$( .push($subdir:expr) )? ) => {{
        use crate::dirs::*;
        #[allow(unused_mut)]
        let mut dir = concat_idents!($dir, _dir)();
        $( dir.push($subdir); )?
        if dir.is_file() {
            Err(anyhow!(
                "{} dir `{}` is a file",
                stringify!($dir),
                dir.display()
            ))
        } else {
            if !dir.is_dir() {
                std::fs::create_dir_all(&dir)?
            }
            Ok(dir)
        }
    }};
}

#[macro_export]
macro_rules! oasis_dir {
    ($dir:ident) => {
        $crate::ensure_dir!($dir.push("oasis"));
    };
}
