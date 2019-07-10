use std::{
    ffi::OsStr,
    io::{self, Read, Write},
    thread,
};

use crate::{config::Config, error::Error};

#[derive(Clone, Copy, PartialOrd, PartialEq)]
pub enum Verbosity {
    Silent,
    Normal,
    Verbose,
    High,
    Debug,
}

impl From<u64> for Verbosity {
    fn from(num_vs: u64) -> Self {
        match num_vs {
            0 => Verbosity::Normal,
            1 => Verbosity::Verbose,
            2 => Verbosity::High,
            _ => Verbosity::Debug,
        }
    }
}

pub struct StdioHandlers {
    pub stdout: Box<dyn Write + Send>,
    pub stderr: Box<dyn Write + Send>,
}

pub fn run_cmd(
    config: &Config,
    name: &'static str,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    verbosity: Verbosity,
) -> Result<(), failure::Error> {
    run_cmd_with_env(config, name, args, verbosity, std::env::vars_os())
}

pub fn run_cmd_with_env(
    config: &Config,
    name: &'static str,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    verbosity: Verbosity,
    envs: impl IntoIterator<Item = (impl AsRef<OsStr>, impl AsRef<OsStr>)>,
) -> Result<(), failure::Error> {
    let (stdout, stderr): (Box<dyn Write + Send>, Box<dyn Write + Send>) =
        if verbosity < Verbosity::Normal {
            (box io::sink(), box io::sink())
        } else if verbosity == Verbosity::Verbose {
            (box io::stdout(), box io::sink())
        } else {
            (box io::stdout(), box io::stderr())
        };

    run_cmd_with_env_and_output(config, name, args, envs, StdioHandlers { stdout, stderr })
}

pub fn run_cmd_with_env_and_output(
    config: &Config,
    name: &str,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    envs: impl IntoIterator<Item = (impl AsRef<OsStr>, impl AsRef<OsStr>)>,
    stdio_handlers: StdioHandlers,
) -> Result<(), failure::Error> {
    let mut child = std::process::Command::new(name)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .args(args)
        .envs(envs)
        .spawn()
        .map_err(|e| match e.kind() {
            io::ErrorKind::NotFound => Error::ExecNotFound(name.to_string()),
            _ => Error::Unknown(e.to_string()),
        })?;

    let log_conf = &config.logging;

    macro_rules! wrap_with_logger {
        ($stream:ident, $log_path:expr) => {
            match child.$stream.take() {
                Some(stream) => {
                    let output_handler = log_tee(
                        &$log_path,
                        stringify!(stream),
                        &config.logging.id,
                        stdio_handlers.$stream,
                        log_conf.enabled,
                    )?;
                    Some(handle_output(stream, output_handler))
                }
                None => None,
            }
        };
    }

    let stdout_handle = wrap_with_logger!(stdout, log_conf.path_stdout);
    let stderr_handle = wrap_with_logger!(stderr, log_conf.path_stderr);

    let status = child
        .wait()
        .map_err(|err| Error::Unknown(err.to_string()))?;

    if let Some(handle) = stdout_handle {
        handle.join().map_err(|_| Error::JoinThread)?;
    }
    if let Some(handle) = stderr_handle {
        handle.join().map_err(|_| Error::JoinThread)?;
    }

    if status.success() {
        Ok(())
    } else {
        Err(Error::ProcessExit(name.to_string(), status.code().unwrap()).into())
    }
}

fn handle_output(
    mut source: impl Read + Send + 'static,
    mut sink: Box<dyn Write + Send + 'static>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut buf = [0; 4096];
        loop {
            let nbytes = source.read(&mut buf).unwrap();
            if nbytes == 0 {
                sink.flush().unwrap();
                break;
            } else {
                sink.write_all(&buf[..nbytes]).unwrap();
            }
        }
    })
}

fn log_tee(
    log_path: &std::path::Path,
    logger_name: impl AsRef<str>,
    logger_id: impl AsRef<str>,
    handler: Box<dyn Write + Send + 'static>,
    logging_enabled: bool,
) -> Result<Box<dyn Write + Send>, failure::Error> {
    let log_file: Box<dyn Write> = if logging_enabled {
        box std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)
            .map_err(|e| Error::OpenLogFile(e.to_string()))?
    } else {
        box io::sink()
    };

    Ok(box crate::logger::Logger::new(
        logger_name.as_ref().to_string(),
        logger_id.as_ref().to_string(),
        handler,
        log_file,
    )?)
}
