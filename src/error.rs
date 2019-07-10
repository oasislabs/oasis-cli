use failure::Fail;

#[derive(Fail, Debug)]
pub enum Error {
    #[fail(display = "failed to open logging file `{}`", _0)]
    OpenLogFile(String),
    #[fail(display = "process `{}` exited with code `{}`", _0, _1)]
    ProcessExit(String, i32),
    #[fail(
        display = "could not run `{}`, please make sure it is in your PATH.",
        _0
    )]
    ExecNotFound(String),
    #[fail(display = "Failed to join thread")]
    JoinThread,
    #[fail(display = "Unknown project type: `{}`", _0)]
    UnknownProjectType(String),
    #[fail(display = "error `{}`", _0)]
    Unknown(String),
}
