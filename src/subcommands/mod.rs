mod build;
mod chain;
mod clean;
mod deploy;
mod ifextract;
mod init;
mod test;
pub mod toolchain;

pub use build::{build, BuildOptions};
pub use chain::run_chain;
pub use clean::clean;
pub use deploy::{deploy, DeployOptions};
pub use ifextract::ifextract;
pub use init::{init, InitOptions};
pub use test::{test, TestOptions};

pub trait ExecSubcommand {
    fn exec(self) -> Result<(), failure::Error>;
}

impl<T: ExecSubcommand> ExecSubcommand for Result<T, failure::Error> {
    fn exec(self) -> Result<(), failure::Error> {
        self?.exec()
    }
}
