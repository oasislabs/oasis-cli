use std::fmt;

use failure::Fail;

#[derive(Fail, Debug)]
pub enum Error {
    #[fail(display = "failed to open logging file `{}`", _0)]
    OpenLogFile(String),
    #[fail(display = "process `{}` exited with code `{}`", _0, _1)]
    ProcessExit(String, i32),
    #[fail(display = "failed to parse `{}`: `{}`", _0, _1)]
    ConfigParse(String, String),
    #[fail(display = "could not run `{}`. Please add it to your PATH", _0)]
    ExecNotFound(String),
    #[fail(display = "could not read file `{}`: `{}`", _0, _1)]
    ReadFile(String, String),
    #[fail(display = "no project in `{}` or any parent directory", _0)]
    DetectProject(String),
    #[fail(display = "destination path `{}` already exists", _0)]
    FileAlreadyExists(String),
    #[fail(display = "unknown toolchain version: `{}`", _0)]
    UnknownToolchain(String),
}

#[derive(Fail, Debug)]
pub struct ProfileError {
    pub name: String,
    pub kind: ProfileErrorKind,
}

impl fmt::Display for ProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ProfileErrorKind::Missing => write!(f, "`profile.{}` does not exist", self.name),
            ProfileErrorKind::Invalid { key, cause } => {
                let key_str = match key {
                    Some(k) => format!(".{}", k),
                    None => "".to_string(),
                };
                write!(
                    f,
                    "`profile.{}{}` is invalid: {}",
                    self.name, key_str, cause
                )
            }
        }
    }
}

#[derive(Debug)]
pub enum ProfileErrorKind {
    Missing,
    Invalid {
        key: Option<&'static str>,
        cause: String,
    },
}
