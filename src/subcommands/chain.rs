use std::{
    io::{BufRead as _, BufReader},
    process::{Command, Stdio},
    thread::{self, JoinHandle},
};

use colored::{Color, Colorize as _};

use crate::{command::Verbosity, errors::Result};

pub struct ChainOptions {
    pub verbosity: Verbosity,
}

impl ChainOptions {
    pub fn new<'a>(m: &'a clap::ArgMatches) -> Result<Self> {
        Ok(Self {
            verbosity: Verbosity::from(m.occurrences_of("verbose") as i64),
        })
    }
}

impl super::ExecSubcommand for ChainOptions {
    fn exec(self) -> Result<()> {
        run_chain(self)
    }
}

pub fn run_chain(opts: ChainOptions) -> Result<()> {
    let gateway_args = vec![
        "--eth.wallet.private_keys",
        "b5144c6bda090723de712e52b92b4c758d78348ddce9aa80ca8ef51125bfb308",
        //^ zeroth account, with address 0xb8b3666d8fea887d97ab54f571b8e5020c5c8b58
        "--eth.url",
        "ws://localhost:8546",
        "--bind_public.max_body_bytes",
        "1048576", // 1 MiB
        "--bind_private.http_port",
        "1235",
    ];

    // crate::emit!(cmd.chain);
    match opts.verbosity {
        Verbosity::Silent | Verbosity::Quiet => unreachable!(), // no --quiet option
        Verbosity::Normal => {
            let mut chain_subproc = Command::new("oasis-chain").spawn()?;
            let mut gateway_subproc = Command::new("oasis-gateway")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .args(gateway_args)
                .spawn()?;

            gateway_subproc.wait()?;
            chain_subproc.wait()?;
        }
        Verbosity::Verbose | Verbosity::High | Verbosity::Debug => {
            let chain_handle = spawn_muxed("oasis-chain", Vec::new(), Color::Cyan);
            let gateway_handle = spawn_muxed("oasis-gateway", gateway_args, Color::Magenta);
            gateway_handle.join().unwrap();
            chain_handle.join().unwrap();
        }
    }

    Ok(())
}

fn spawn_muxed(command: &'static str, args: Vec<&'static str>, color: Color) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut subproc = Command::new(command)
            .args(&args)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap_or_else(|_| panic!("could not start {}", command));
        let stdout = BufReader::new(subproc.stdout.take().unwrap());
        for line in stdout.lines().filter_map(Result::ok) {
            println!("{} | {}", command.color(color), line);
        }
        subproc.wait().unwrap();
    })
}
