#!/bin/bash

set -e

export OASIS_TEST_BASE=/tmp/oasis-cli.$RANDOM
export OASIS_CLI_BINARY=$(pwd)/target/debug/oasis
export ORIGINAL_XDG_CONFIG_DIR=$XDG_CONFIG_DIR
export XDG_CONFIG_DIR=$OASIS_TEST_BASE

mkdir -p $OASIS_TEST_BASE

if [ ! -f $OASIS_CLI_BINARY ]; then
    echo "could not find oasis cli binary at "$OASIS_CLI_BINARY
    exit 1
fi

source tests/tests.sh
test_all

export XDG_CONFIG_DIR=$ORIGINAL_XDG_CONFIG_DIR
