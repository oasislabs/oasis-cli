use std::{
    ffi::OsStr,
    io::{self, Read, Write},
    process::Stdio,
    thread,
};

use crate::error::Error;

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

pub fn run_cmd(
    name: &'static str,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    verbosity: Verbosity,
) -> Result<(), failure::Error> {
    run_cmd_with_env(name, args, verbosity, std::env::vars_os())
}

pub fn run_cmd_with_env(
    name: &'static str,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    verbosity: Verbosity,
    envs: impl IntoIterator<Item = (impl AsRef<OsStr>, impl AsRef<OsStr>)>,
) -> Result<(), failure::Error> {
    let (stdout, stderr) = if verbosity < Verbosity::Normal {
        (Sink::Ignored, Sink::Ignored)
    } else if verbosity == Verbosity::Verbose {
        (Sink::Inherited, Sink::Ignored)
    } else {
        (Sink::Inherited, Sink::Inherited)
    };

    run_cmd_with_env_and_output(name, args, envs, stdout, stderr)
}

pub enum Sink<'a> {
    Ignored,
    Inherited,
    Piped(&'a mut (dyn Write + Send)),
}

impl<'a> Sink<'a> {
    pub fn as_stdio(&self) -> Stdio {
        match self {
            Sink::Ignored => Stdio::null(),
            Sink::Inherited => Stdio::inherit(),
            Sink::Piped(_) => Stdio::piped(),
        }
    }
}

pub fn run_cmd_with_env_and_output(
    name: &str,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    envs: impl IntoIterator<Item = (impl AsRef<OsStr>, impl AsRef<OsStr>)>,
    stdout: Sink,
    stderr: Sink,
) -> Result<(), failure::Error> {
    let mut cmd = std::process::Command::new(name)
        .args(args)
        .envs(envs)
        .stdout(stdout.as_stdio())
        .stderr(stderr.as_stdio())
        .spawn()
        .map_err(|e| match e.kind() {
            io::ErrorKind::NotFound => Error::ExecNotFound(name.to_string()).into(),
            _ => failure::Error::from(e),
        })?;

    macro_rules! handle {
        ($stream:ident) => {
            match $stream {
                Sink::Piped(sink) => cmd.$stream.take().map(|source| {
                    handle_output(source, unsafe {
                        // The main thread waits on subprocess, so writer is effectively
                        // static for the life of the subprocess.
                        std::mem::transmute::<
                            &mut (dyn Write + Send),
                            &mut (dyn Write + Send + 'static),
                        >(sink)
                    })
                }),
                _ => None,
            }
        };
    }

    let stdout_handle = handle!(stdout);
    let stderr_handle = handle!(stderr);

    let status = cmd.wait()?;

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
    mut sink: impl Write + Send + 'static,
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
