#!/bin/bash

set -e

export OASIS_TEST_BASE=/tmp/oasis-cli.$RANDOM
export OASIS_CLI_BINARY=$(pwd)/target/debug/oasis
export XDG_CONFIG_HOME=$OASIS_TEST_BASE

mkdir -p $OASIS_TEST_BASE

if [ ! -f $OASIS_CLI_BINARY ]; then
    cargo build
fi

source tests/tests.sh
test_all
