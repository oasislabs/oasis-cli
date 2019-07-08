use bytebuffer::ByteBuffer;
use chrono::Utc;
use std::{
    ffi::OsStr,
    fs,
    io::{self, Read, Write},
    process,
    sync::mpsc::channel,
    thread,
};

use crate::config::Config;

const KB: usize = 1 << 10;

#[derive(Clone, Copy, PartialOrd, PartialEq)]
pub enum Verbosity {
    Silent,
    Normal,
    Verbose,
    High,
    Debug,
}

struct OutputLogger<'a> {
    buf: ByteBuffer,
    name: &'a str,
    logtype: String,
    out: Box<dyn Write>,
    logger: Box<dyn Write>,
}

unsafe impl<'a> Send for OutputLogger<'a> {}

impl<'a> OnOutputCallback for OutputLogger<'a> {
    fn onstart(&mut self) {}

    fn ondata(&mut self, output: &[u8]) {
        let _ = self.out.write(output);
        let abytes = 3 * KB - self.buf.len();
        let cbytes = std::cmp::min(abytes, output.len());
        if cbytes > 0 {
            self.buf.write_bytes(&output[..cbytes]);
        }
    }

    fn onend(&mut self) {
        if self.buf.len() > 0 {
            let _ = writeln!(
                self.logger,
                "{{\"date\": \"{}\", \"name\": \"{}\", \"logtype\": \"{}\", data\": \"{}\"}}",
                Utc::now(),
                self.name,
                self.logtype,
                String::from_utf8(self.buf.to_bytes())
                    .unwrap()
                    .replace("\n", "\\n")
            );
        }
    }
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

pub trait OnOutputCallback {
    fn onstart(&mut self);
    fn ondata(&mut self, output: &[u8]);
    fn onend(&mut self);
}

pub struct CommandProps {
    pub stdout: process::Stdio,
    pub stderr: process::Stdio,
    pub on_stdout_callback: Option<Box<dyn OnOutputCallback + Send>>,
    pub on_stderr_callback: Option<Box<dyn OnOutputCallback + Send>>,
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
    use std::process::Stdio;
    let (stdout, stderr) = if verbosity < Verbosity::Normal {
        (Stdio::null(), Stdio::null())
    } else if verbosity == Verbosity::Verbose {
        (Stdio::null(), Stdio::piped())
    } else {
        (Stdio::piped(), Stdio::piped())
    };

    let mut on_stdout_callback: Option<Box<(dyn OnOutputCallback + std::marker::Send + 'static)>> =
        None;
    let mut on_stderr_callback: Option<Box<(dyn OnOutputCallback + std::marker::Send + 'static)>> =
        None;

    if config.logging.enabled && verbosity >= Verbosity::Verbose {
        let logfile_stdout = fs::OpenOptions::new()
            .read(false)
            .append(true)
            .write(true)
            .create(true)
            .open(&config.logging.path_stdout)
            .map_err(|e| {
                failure::format_err!("failed to open logging file stdout {}", e.to_string())
            })?;

        on_stdout_callback = Some(Box::new(OutputLogger {
            buf: ByteBuffer::new(),
            name: name,
            logtype: "stdout".to_string(),
            out: Box::new(io::stdout()),
            logger: Box::new(logfile_stdout),
        }));
    }

    if config.logging.enabled && verbosity > Verbosity::Verbose {
        let logfile_stderr = fs::OpenOptions::new()
            .read(false)
            .append(true)
            .write(true)
            .create(true)
            .open(&config.logging.path_stderr)
            .map_err(|e| {
                failure::format_err!("failed to open logging file stderr {}", e.to_string())
            })?;

        on_stderr_callback = Some(Box::new(OutputLogger {
            buf: ByteBuffer::new(),
            name: name,
            logtype: "stderr".to_string(),
            out: Box::new(io::stderr()),
            logger: Box::new(logfile_stderr),
        }));
    }

    run(
        name,
        args,
        envs,
        CommandProps {
            stdout,
            stderr,
            on_stdout_callback: on_stdout_callback,
            on_stderr_callback: on_stderr_callback,
        },
    )
}

fn run(
    name: &str,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    envs: impl IntoIterator<Item = (impl AsRef<OsStr>, impl AsRef<OsStr>)>,
    props: CommandProps,
) -> Result<(), failure::Error> {
    let mut cmd = process::Command::new(name);
    let (stdout_sender, stdout_receiver) = channel();
    let (stderr_sender, stderr_receiver) = channel();

    let mut child = cmd
        .stdout(props.stdout)
        .stderr(props.stderr)
        .args(args)
        .envs(envs)
        .spawn()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => failure::format_err!(
                "Could not run `{}`, please make sure it is in your PATH.",
                name
            ),
            _ => failure::format_err!("{}", e.to_string()),
        })?;

    let mut stdout_thread = None;
    let mut stderr_thread = None;

    if let Some(mut stdout) = child.stdout.take() {
        if let Some(mut on_stdout_callback) = props.on_stdout_callback {
            stdout_thread = Some(thread::spawn(move || {
                let mut buffer = [0; 4096];
                on_stdout_callback.onstart();

                loop {
                    match stdout.read(&mut buffer[..]) {
                        Ok(rbytes) => {
                            if rbytes == 0 {
                                on_stdout_callback.onend();
                                let _ = stdout_sender.send(None);
                                return;
                            }
                            on_stdout_callback.ondata(&buffer[0..rbytes]);
                        }
                        Err(err) => {
                            let _ = stdout_sender.send(Some(err));
                            return;
                        }
                    }
                }
            }));
        }
    }

    if let Some(mut stderr) = child.stderr.take() {
        if let Some(mut on_stderr_callback) = props.on_stderr_callback {
            stderr_thread = Some(thread::spawn(move || {
                let mut buffer = [0; 4096];
                on_stderr_callback.onstart();

                loop {
                    match stderr.read(&mut buffer[..]) {
                        Ok(rbytes) => {
                            if rbytes == 0 {
                                on_stderr_callback.onend();
                                let _ = stderr_sender.send(None);
                                return;
                            }
                            on_stderr_callback.ondata(&buffer[0..rbytes]);
                        }
                        Err(err) => {
                            let _ = stderr_sender.send(Some(err));
                            return;
                        }
                    }
                }
            }));
        }
    }

    if let Some(thread) = stdout_thread {
        match stdout_receiver.recv() {
            Err(err) => {
                return Err(failure::format_err!(
                    "Failed to receive stdout thread result `{}`",
                    err.to_string()
                ))
            }
            Ok(opt) => match opt {
                None => {}
                Some(err) => println!(
                    "WARN: error on capturing stdout output `{}`",
                    err.to_string()
                ),
            },
        }
        let _ = thread.join();
    }

    if let Some(thread) = stderr_thread {
        match stderr_receiver.recv() {
            Err(err) => {
                return Err(failure::format_err!(
                    "Failed to receive stderr thread result `{}`",
                    err.to_string()
                ))
            }
            Ok(opt) => match opt {
                None => {}
                Some(err) => println!(
                    "WARN: error on capturing stdout output `{}`",
                    err.to_string()
                ),
            },
        }

        let _ = thread.join();
    }

    let status = child
        .wait()
        .map_err(|e| failure::format_err!("{}", e.to_string()))?;

    if status.success() {
        Ok(())
    } else {
        Err(failure::format_err!(
            "Processes `{}` exited with code `{}`",
            name,
            status.code().unwrap()
        ))
    }
}
