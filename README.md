# oasis-cli

A command line utility for managing Oasis packages.

```
$ cargo install oasis-cli

$ oasis --help

oasis 0.1.0
Oasis developer tools

USAGE:
    oasis [SUBCOMMAND]

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

SUBCOMMANDS:
    build        Build services for the Oasis platform
    clean        Remove build products
    deploy       Deploy a service to the Oasis blockchain
    help         Prints this message or the help of the given subcommand(s)
    ifextract    Extract interface definition(s) from a service.wasm
    init         Create a new Oasis package
    test         Run integration tests against a simulated Oasis runtime.
```
