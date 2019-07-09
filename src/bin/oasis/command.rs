use std::{
    ffi::OsStr,
    io::{self, Read, Write},
    thread,
};

use chrono::Utc;

use crate::config::Config;

#[derive(Clone, Copy, PartialOrd, PartialEq)]
pub enum Verbosity {
    Silent,
    Normal,
    Verbose,
    High,
    Debug,
}

struct OutputLogger<'a> {
    name: &'a str,
    out: Box<dyn Write>,
    logger: Box<dyn Write>,
}

unsafe impl<'a> Send for OutputLogger<'a> {}

impl<'a> OnOutputCallback for OutputLogger<'a> {
    fn on_start(&mut self) -> Result<(), failure::Error> {
        write!(
            self.logger,
            "{{\"date\": \"{}\", \"name\": \"{}\", \"data\": \"",
            Utc::now(),
            self.name,
        )?;

        Ok(())
    }

    fn on_data(&mut self, output: &[u8]) -> Result<(), failure::Error> {
        self.out.write_all(output)?;
        write!(
            self.logger,
            "{}",
            regex::escape(
                &String::from_utf8(output.to_vec())
                    .unwrap()
                    .replace("\n", "\\n")
                    .replace("\r", "\\r")
            )
        )?;

        Ok(())
    }

    fn on_end(&mut self) -> Result<(), failure::Error> {
        writeln!(self.logger, "\"}}")?;
        Ok(())
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
    fn on_start(&mut self) -> Result<(), failure::Error>;
    fn on_data(&mut self, output: &[u8]) -> Result<(), failure::Error>;
    fn on_end(&mut self) -> Result<(), failure::Error>;
}

pub struct CommandProps {
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

fn generate_collector(
    enabled: bool,
    verbosity: Verbosity,
    name: &'static str,
    out: Box<dyn Write>,
    path: &std::path::PathBuf,
) -> Result<Option<Box<dyn OnOutputCallback + Send + 'static>>, failure::Error> {
    if enabled && verbosity >= Verbosity::Verbose {
        let logfile = std::fs::OpenOptions::new()
            .read(false)
            .append(true)
            .create(true)
            .open(path)
            .map_err(|e| {
                failure::format_err!("failed to open logging file stdout {}", e.to_string())
            })?;

        Ok(Some(Box::new(OutputLogger {
            name,
            out,
            logger: Box::new(logfile),
        })))
    } else {
        Ok(None)
    }
}

pub fn run_cmd_with_env(
    config: &Config,
    name: &'static str,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    verbosity: Verbosity,
    envs: impl IntoIterator<Item = (impl AsRef<OsStr>, impl AsRef<OsStr>)>,
) -> Result<(), failure::Error> {
    let (stdout, stderr): (Box<dyn Write>, Box<dyn Write>) = if verbosity < Verbosity::Normal {
        (Box::new(io::sink()), Box::new(io::sink()))
    } else if verbosity == Verbosity::Verbose {
        (Box::new(io::stdout()), Box::new(io::sink()))
    } else {
        (Box::new(io::stdout()), Box::new(io::stderr()))
    };

    let on_stdout_callback = generate_collector(
        config.logging.enabled,
        verbosity,
        name,
        stdout,
        &config.logging.path_stdout,
    )?;
    let on_stderr_callback = generate_collector(
        config.logging.enabled,
        verbosity,
        name,
        stderr,
        &config.logging.path_stderr,
    )?;

    run(
        name,
        args,
        envs,
        CommandProps {
            on_stdout_callback,
            on_stderr_callback,
        },
    )
}

fn collect_output<O: Read + Send + 'static>(
    out: Option<O>,
    on_output_callback: Option<Box<dyn OnOutputCallback + Send>>,
    sender: std::sync::mpsc::Sender<Option<failure::Error>>,
) -> Option<thread::JoinHandle<()>> {
    let (mut out, mut on_output_callback) = match (out, on_output_callback) {
        (Some(out), Some(on_output_callback)) => (out, on_output_callback),
        _ => return None,
    };
    Some(thread::spawn(move || {
        let mut buffer = [0; 4096];
        if let Err(err) = on_output_callback.on_start() {
            warn!("error occurred on starting output collection: {}", err);
        }

        loop {
            match out.read(&mut buffer[..]) {
                Ok(rbytes) => {
                    if rbytes == 0 {
                        if let Err(err) = on_output_callback.on_end() {
                            error!("error occurred on ending output collection: {}", err);
                        }
                        if let Err(err) = sender.send(None) {
                            error!("failed to return successful result from thread: {}", err);
                        }
                        return;
                    }
                    if let Err(err) = on_output_callback.on_data(&buffer[0..rbytes]) {
                        error!("error occurred on output collection: {}", err);
                    }
                }
                Err(err) => {
                    if let Err(err) = sender.send(Some(failure::format_err!(
                        "failed to receive error `{}`",
                        err.to_string()
                    ))) {
                        error!("failed to return error result from thread: {}", err);
                    }
                    return;
                }
            }
        }
    }))
}

fn finish_collection(
    thread: Option<thread::JoinHandle<()>>,
    receiver: std::sync::mpsc::Receiver<Option<failure::Error>>,
) -> Result<(), failure::Error> {
    if let Some(thread) = thread {
        match receiver.recv() {
            Err(err) => {
                return Err(failure::format_err!(
                    "Failed to receive thread result `{}`",
                    err.to_string()
                ))
            }
            Ok(opt) => match opt {
                None => {}
                Some(err) => {
                    return Err(failure::format_err!(
                        "Error capturing output `{}`",
                        err.to_string()
                    ))
                }
            },
        }

        thread
            .join()
            .map_err(|_| failure::format_err!("Failed to join thread"))?;
    }

    Ok(())
}

fn run(
    name: &str,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    envs: impl IntoIterator<Item = (impl AsRef<OsStr>, impl AsRef<OsStr>)>,
    props: CommandProps,
) -> Result<(), failure::Error> {
    let mut cmd = std::process::Command::new(name);
    let (stdout_sender, stdout_receiver) = std::sync::mpsc::channel();
    let (stderr_sender, stderr_receiver) = std::sync::mpsc::channel();

    let mut child = cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .args(args)
        .envs(envs)
        .spawn()
        .map_err(|e| match e.kind() {
            io::ErrorKind::NotFound => failure::format_err!(
                "Could not run `{}`, please make sure it is in your PATH.",
                name
            ),
            _ => failure::format_err!("{}", e.to_string()),
        })?;

    let stdout_thread: Option<thread::JoinHandle<()>> =
        collect_output(child.stdout.take(), props.on_stdout_callback, stdout_sender);
    let stderr_thread: Option<thread::JoinHandle<()>> =
        collect_output(child.stderr.take(), props.on_stderr_callback, stderr_sender);

    finish_collection(stdout_thread, stdout_receiver)?;
    finish_collection(stderr_thread, stderr_receiver)?;

    let status = child
        .wait()
        .map_err(|e| failure::format_err!("{}", e.to_string()))?;

    if status.success() {
        Ok(())
    } else {
        Err(failure::format_err!(
            "Process `{}` exited with code `{}`",
            name,
            status.code().unwrap()
        ))
    }
}
