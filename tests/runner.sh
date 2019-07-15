#!/bin/bash

set -e

export OASIS_TEST_BASE=/tmp/oasis-cli.$RANDOM
export OASIS_CLI_BINARY=$(pwd)/target/debug/oasis

# just use the HOME directory as the base directory
# for configuration and data generated by the cli
export XDG_CONFIG_HOME=
export XDG_DATA_HOME=

if [ $(uname) == "Darwin" ]; then
    export HOME=$OASIS_TEST_BASE
    export OASIS_CONFIG_DIR=$HOME/Library/Preferences
    export OASIS_DATA_DIR=$HOME/Library/"Application Support"
else
    if [ $(uname) == "Linux" ]; then
        export HOME=$OASIS_TEST_BASE
        export OASIS_CONFIG_DIR=$HOME/.config
        export OASIS_DATA_DIR=$HOME/.local/share
    fi
fi

mkdir -p $OASIS_TEST_BASE

rustup default nightly
if [ ! -f $OASIS_CLI_BINARY ]; then
    cargo build
fi

source tests/tests.sh
test_all