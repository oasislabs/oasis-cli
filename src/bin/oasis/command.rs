use std::{
    ffi::OsStr,
    io::{self, Read, Write},
    thread,
};

use bytebuffer::ByteBuffer;
use chrono::Utc;

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
    fn onstart(&mut self) -> Result<(), failure::Error> {
        Ok(())
    }

    fn ondata(&mut self, output: &[u8]) -> Result<(), failure::Error> {
        self.out.write_all(output)?;
        let abytes = 3 * KB - self.buf.len();
        let cbytes = std::cmp::min(abytes, output.len());
        if cbytes > 0 {
            self.buf.write_bytes(&output[..cbytes]);
        }

        Ok(())
    }

    fn onend(&mut self) -> Result<(), failure::Error> {
        if self.buf.len() > 0 {
            writeln!(
                self.logger,
                "{{\"date\": \"{}\", \"name\": \"{}\", \"logtype\": \"{}\", data\": \"{}\"}}",
                Utc::now(),
                self.name,
                self.logtype,
                String::from_utf8(self.buf.to_bytes())
                    .unwrap()
                    .replace("\n", "\\n")
            )?;
        }

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
    fn onstart(&mut self) -> Result<(), failure::Error>;
    fn ondata(&mut self, output: &[u8]) -> Result<(), failure::Error>;
    fn onend(&mut self) -> Result<(), failure::Error>;
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

    let mut on_stdout_callback: Option<Box<(dyn OnOutputCallback + std::marker::Send + 'static)>> =
        None;
    let mut on_stderr_callback: Option<Box<(dyn OnOutputCallback + std::marker::Send + 'static)>> =
        None;

    if config.logging.enabled && verbosity >= Verbosity::Verbose {
        let logfile_stdout = std::fs::OpenOptions::new()
            .read(false)
            .append(true)
            .create(true)
            .open(&config.logging.path_stdout)
            .map_err(|e| {
                failure::format_err!("failed to open logging file stdout {}", e.to_string())
            })?;

        on_stdout_callback = Some(Box::new(OutputLogger {
            buf: ByteBuffer::new(),
            name,
            logtype: "stdout".to_string(),
            out: stdout,
            logger: Box::new(logfile_stdout),
        }));
    }

    if config.logging.enabled && verbosity > Verbosity::Verbose {
        let logfile_stderr = std::fs::OpenOptions::new()
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
            name,
            logtype: "stderr".to_string(),
            out: stderr,
            logger: Box::new(logfile_stderr),
        }));
    }

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
) -> Result<Option<thread::JoinHandle<()>>, failure::Error> {
    if let Some(mut out) = out {
        if let Some(mut on_output_callback) = on_output_callback {
            return Ok(Some(thread::spawn(move || {
                let mut buffer = [0; 4096];
                if let Err(err) = on_output_callback.onstart() {
                    println!(
                        "WARN: error occurred on starting output collection `{}`",
                        err.to_string()
                    );
                }

                loop {
                    match out.read(&mut buffer[..]) {
                        Ok(rbytes) => {
                            if rbytes == 0 {
                                if let Err(err) = on_output_callback.onend() {
                                    println!(
                                        "WARN: error occurred on ending output collection `{}`",
                                        err.to_string()
                                    );
                                }

                                if let Err(err) = sender.send(None) {
                                    println!(
                                        "ERROR: failed to return successful result from thread `{}`",
                                        err.to_string()
                                    );
                                }
                                return;
                            }
                            if let Err(err) = on_output_callback.ondata(&buffer[0..rbytes]) {
                                println!(
                                    "WARN: error occurred on output collection `{}`",
                                    err.to_string()
                                );
                            }
                        }
                        Err(err) => {
                            if let Err(err) = sender.send(Some(failure::format_err!(
                                "failed to receive error `{}`",
                                err.to_string()
                            ))) {
                                println!(
                                    "ERROR: failed to return error result from thread `{}`",
                                    err.to_string()
                                );
                            }
                            return;
                        }
                    }
                }
            })));
        }
    }

    Ok(None)
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
                Some(err) => println!("WARN: error on capturing output `{}`", err.to_string()),
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

    let stdout_thread: Option<thread::JoinHandle<()>> = None;
    let stderr_thread: Option<thread::JoinHandle<()>> = None;

    collect_output(child.stdout.take(), props.on_stdout_callback, stdout_sender)?;
    collect_output(child.stderr.take(), props.on_stderr_callback, stderr_sender)?;

    finish_collection(stdout_thread, stdout_receiver)?;
    finish_collection(stderr_thread, stderr_receiver)?;

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
