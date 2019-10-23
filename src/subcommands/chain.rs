pub fn run_chain(chain_args: Vec<String>) -> failure::Fallible<()> {
    crate::emit!(cmd.chain, { "args": chain_args });
    crate::command::run_cmd(
        "oasis-chain",
        chain_args.iter().map(|a| a.as_str()).collect::<Vec<_>>(),
        crate::command::Verbosity::Normal,
    )
}
