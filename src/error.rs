use failure::Fail;

#[derive(Fail, Debug)]
pub enum Error {
    #[fail(display = "failed to open logging file `{}`", _0)]
    OpenLogFile(String),
    #[fail(display = "process `{}` exited with code `{}`", _0, _1)]
    ProcessExit(String, i32),
    #[fail(display = "failed to parse `{}`: `{}`", _0, _1)]
    ConfigParse(String, String),
    #[fail(
        display = "could not run `{}`, please make sure it is in your PATH.",
        _0
    )]
    ExecNotFound(String),
    #[fail(display = "could not read file `{}`: `{}`.", _0, _1)]
    ReadFile(String, String),
    #[fail(display = "failed to join thread")]
    JoinThread,
    #[fail(display = "no project in `{}` or any parent directory", _0)]
    DetectProject(String),
    #[fail(display = "destination path `{}` already exists.", _0)]
    FileAlreadyExists(String),
}
