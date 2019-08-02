pub fn run_chain() -> Result<(), failure::Error> {
    crate::emit!(cmd.chain);
    crate::command::run_cmd(
        "oasis-chain",
        std::env::args().skip(2), // oasis chain ...
        crate::command::Verbosity::Normal,
    )
}
