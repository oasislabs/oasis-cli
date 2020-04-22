mod build;
mod chain;
mod clean;
mod deploy;
mod ifextract;
mod init;
mod test;
pub mod toolchain;

use crate::errors::Error;

pub use build::{build, BuildOptions};
pub use chain::{run_chain, ChainOptions};
pub use clean::clean;
pub use deploy::{deploy, DeployOptions};
pub use ifextract::ifextract;
pub use init::{init, InitOptions};
pub use test::{test, TestOptions};

pub trait ExecSubcommand {
    fn exec(self) -> Result<(), Error>;
}

impl<T: ExecSubcommand> ExecSubcommand for Result<T, Error> {
    fn exec(self) -> Result<(), Error> {
        self?.exec()
    }
}
