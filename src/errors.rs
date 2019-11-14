use std::fmt;

pub use anyhow::{Error, Result};

#[derive(thiserror::Error, Debug)]
pub enum CliError {
    #[error("errored to open logging file `{}`", .0)]
    OpenLogFile(String),

    #[error("process `{}` exited with code `{}`", .0, .1)]
    ProcessExit(String, i32),

    #[error("errored to parse `{}`: `{}`", .0, .1)]
    ConfigParse(String, String),

    #[error("could not run `{}`. Please add it to your PATH", .0)]
    ExecNotFound(String),

    #[error("could not read file `{}`: `{}`", .0, .1)]
    ReadFile(String, String),

    #[error("destination path `{}` already exists", .0)]
    FileAlreadyExists(String),

    #[error("unknown toolchain version: `{}`", .0)]
    UnknownToolchain(String),
}

#[derive(thiserror::Error, Debug)]
pub enum WorkspaceError {
    #[error("could not find workspace in `{}` or any parent directory", .0)]
    NoWorkspace(String),

    #[error("could not find dependency `{}` in the current workspace", .0)]
    MissingDependency(String),

    #[error("`{}` has a circular dependency on `{}`", .0, .1)]
    CircularDependency(String, String),
}

#[derive(thiserror::Error, Debug)]
pub struct ProfileError {
    pub name: String,
    pub kind: ProfileErrorKind,
}

impl fmt::Display for ProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ProfileErrorKind::MissingProfile => write!(f, "`profile.{}` does not exist", self.name),
            ProfileErrorKind::MissingKey(key) => {
                write!(f, "`profile.{}` is missing field: `{}`.", self.name, key)
            }
            ProfileErrorKind::InvalidKey(key, cause) => {
                write!(f, "`profile.{}.{}` is invalid: {}", self.name, key, cause)
            }
        }
    }
}

#[derive(Debug)]
pub enum ProfileErrorKind {
    MissingProfile,
    MissingKey(&'static str),
    InvalidKey(&'static str, String),
}
