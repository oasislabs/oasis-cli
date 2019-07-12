#!/bin/bash

SUCCESS=0

function before() {
    export OASIS_CONFIG_DIR=$OASIS_TEST_BASE/config
    export OASIS_TEST_PROJECTS=$OASIS_TEST_BASE/projects

    cd $OASIS_TEST_BASE
    mkdir -p $OASIS_TEST_PROJECTS
    mkdir -p $OASIS_CONFIG_DIR
    cd $OASIS_TEST_PROJECTS
}

function after() {
    rm -rf $OASIS_TEST_BASE/*
}

function assert_directory() {
    DIR="$1"
    if [ ! -d $DIR ]; then
        echo "directory "$DIR" does not exist"
        return 1
    fi
    return 0
}

function assert_file() {
    FILE="$1"
    if [ ! -f $FILE ]; then
        echo "file "$FILE" does not exist"
        return 1
    fi
    return 0
}

function assert_file_contains() {
    FILE="$1"
    CONTENT="$2"

    matches=$(grep "$CONTENT" "$FILE" | wc -l || true)

    if [ $matches == "0" ]; then
        echo "file "$FILE" does not contain "$CONTENT
        return 1
    fi
    return 0
}

function assert_string_contains() {
    STR="$1"
    SUBSTR="$2"

    count=$(echo "$STR" | grep "$SUBSTR" | wc -l || true)
    if [ $count == "0" ]; then
        echo "string "$STR" does not contain "$SUBSTR
        return 1
    fi
    return 0
}

function assert_file_doesnot_contain() {
    STR="$1"
    SUBSTR="$2"

    count=$(echo "$STR" | grep "$SUBSTR" | wc -l || true)
    if [ $count == "1" ]; then
        echo "string "$STR" does contains "$SUBSTR
        return 1
    fi
    return 0
}

function init_project() {
    # echo to stdin the expected input for the setup of the tool
    # reply 'y' to enable telemetry and '12345' to the local private key
    # for testing
    printf 'y\n12345\n' | $OASIS_CLI_BINARY init my_project > /dev/null 2>&1
}

function test_oasis_setup_ok_with_telemetry() {
    init_project

    assert_directory $OASIS_TEST_PROJECTS/my_project
    assert_directory $OASIS_CONFIG_DIR
    assert_directory $OASIS_CONFIG_DIR/oasis
    assert_directory $OASIS_CONFIG_DIR/oasis/log
    assert_file $OASIS_CONFIG_DIR/oasis/config
    assert_directory $OASIS_TEST_PROJECTS
    assert_file_contains $OASIS_CONFIG_DIR/oasis/config "endpoint = 'https://gollum.devnet2.oasiscloud.io/'"
    assert_file_contains $OASIS_CONFIG_DIR/oasis/config "private_key = '12345'"
    assert_file_contains $OASIS_CONFIG_DIR/oasis/config "enabled = true"

    return $SUCCESS
}

function test_oasis_setup_ok_no_telemetry() {
    output=$(printf 'n\n\n' | $OASIS_CLI_BINARY init my_project 2>&1 || true)
    assert_file_contains $OASIS_CONFIG_DIR/oasis/config "enabled = false"
    assert_file_doesnot_contain $OASIS_CONFIG_DIR/oasis/config "enabled = true"

    return $SUCCESS
}

function test_oasis_setup_ok_invalid_answers() {
    output=$(printf 'invalid\n\n' | $OASIS_CLI_BINARY init my_project 2>&1 || true)
    assert_file_contains $OASIS_CONFIG_DIR/oasis/config "enabled = false"
    assert_file_doesnot_contain $OASIS_CONFIG_DIR/oasis/config "enabled = true"

    return $SUCCESS
}

function test_oasis_setup_invalid_oasis_config() {
    mkdir -p $OASIS_CONFIG_DIR/oasis
    echo "INVALID CONTENT" > $OASIS_CONFIG_DIR/oasis/config

    output=$(printf 'y\n\n' | $OASIS_CLI_BINARY init my_project 2>&1 || true)
    assert_string_contains "$output" "panicked"

    return $SUCCESS
}

function test_oasis_init() {
    init_project

    assert_directory $OASIS_TEST_PROJECTS/my_project

    return $SUCCESS
}

function test_oasis_build() {
    init_project

    cd my_project
    $OASIS_CLI_BINARY build > /dev/null 2>&1

    assert_file $OASIS_TEST_PROJECTS/my_project/target/wasm32-wasi/debug/my_project.wasm

    return $SUCCESS
}

function test_all() {
    for test_case in "test_oasis_setup_invalid_oasis_config"   \
                         "test_oasis_setup_ok_with_telemetry"  \
                         "test_oasis_setup_ok_no_telemetry"    \
                         "test_oasis_setup_ok_invalid_answers" \
                         "test_oasis_init"                     \
                         "test_oasis_build"; do
        before
        $test_case
        code=$?
        if [[ "$code" == "0" ]]; then
            echo "TEST $test_case succeeded"
        else
            echo "TEST $test_case failed with code "$code
        fi
        after
    done
}
