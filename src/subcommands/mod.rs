mod build;
mod chain;
mod clean;
mod deploy;
mod init;
mod interface;
mod test;
pub mod toolchain;

use failure::Fallible;

pub use build::{build, BuildOptions};
pub use chain::run_chain;
pub use clean::clean;
pub use deploy::{deploy, DeployOptions};
pub use init::{init, InitOptions};
pub use interface::{ifattach, ifextract};
pub use test::{test, TestOptions};

pub trait ExecSubcommand {
    fn exec(self) -> Fallible<()>;
}

impl<T: ExecSubcommand> ExecSubcommand for Fallible<T> {
    fn exec(self) -> Fallible<()> {
        self?.exec()
    }
}
