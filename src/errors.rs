use std::fmt;

pub use failure::Error;
use failure::Fail;

#[derive(Fail, Debug)]
pub enum CliError {
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

    #[fail(display = "destination path `{}` already exists", _0)]
    FileAlreadyExists(String),

    #[fail(display = "unknown toolchain version: `{}`", _0)]
    UnknownToolchain(String),
}

#[derive(Fail, Debug)]
pub enum WorkspaceError {
    #[fail(
        display = "could not find workspace in `{}` or any parent directory",
        _0
    )]
    NotFound(String),

    #[fail(display = "multiple services named `{}` found in workspace", _0)]
    DuplicateService(String),

    #[fail(display = "`{}` has a circular dependency on `{}`", _0, _1)]
    CircularDependency(String, String),
}

#[derive(Fail, Debug)]
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
