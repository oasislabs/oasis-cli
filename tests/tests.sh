#!/bin/bash

SUCCESS=0

function before() {
    export OASIS_PROJECT_DIR="$OASIS_TEST_BASE/project"
    cd "$OASIS_TEST_BASE"

    rm -rf "$OASIS_CONFIG_DIR"/*
    rm -rf "$OASIS_DATA_DIR"/*
    rm -rf "$OASIS_PROJECT_DIR"/*

    mkdir -p "$OASIS_PROJECT_DIR"
    mkdir -p "$OASIS_CONFIG_DIR"
    mkdir -p "$OASIS_DATA_DIR"

    cd "$OASIS_PROJECT_DIR"
}

function after() {
    rm -rf "$OASIS_CONFIG_DIR"/*
    rm -rf "$OASIS_DATA_DIR"/*
    rm -rf "$OASIS_PROJECT_DIR"/*
}

function assert_directory() {
    DIR="$1"
    if [ ! -d "$DIR" ]; then
        echo "directory "$DIR" does not exist"
        return 1
    fi
    return 0
}

function assert_file() {
    FILE="$1"
    if [ ! -f "$FILE" ]; then
        echo "file "$FILE" does not exist"
        return 1
    fi
    return 0
}

function assert_no_file() {
    FILE="$1"
    if [ -f "$FILE" ]; then
        echo "file "$FILE" exists"
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
    printf 'y\n' | $OASIS_CLI_BINARY init my_project > /dev/null
}

function test_oasis_setup_ok_with_telemetry() {
    init_project

    # assert project initialized
    assert_directory "$OASIS_PROJECT_DIR/my_project"

    # assert data directory
    assert_directory "$OASIS_DATA_DIR/oasis"
    assert_file "$OASIS_DATA_DIR/oasis/metrics.jsonl"

    # assert configuration files
    assert_directory "$OASIS_CONFIG_DIR/oasis"
    assert_file "$OASIS_CONFIG_DIR/oasis/config.toml"
    assert_file_contains "$OASIS_CONFIG_DIR/oasis/config.toml" "enabled = true"

    return $SUCCESS
}

function test_oasis_setup_ok_no_telemetry() {
    output=$(printf 'n\n' | $OASIS_CLI_BINARY init my_project 2>&1 || true)

    # assert project initialized
    assert_directory "$OASIS_PROJECT_DIR/my_project"

    # assert data directory
    assert_directory "$OASIS_DATA_DIR/oasis"
    assert_no_file "$OASIS_DATA_DIR/oasis/metrics.jsonl"

    # assert configuration files
    assert_directory "$OASIS_CONFIG_DIR/oasis"
    assert_file "$OASIS_CONFIG_DIR/oasis/config.toml"
    assert_file_contains "$OASIS_CONFIG_DIR/oasis/config.toml" "enabled = false"
    assert_file_doesnot_contain "$OASIS_CONFIG_DIR/config.toml" "enabled = true"

    return $SUCCESS
}

function test_oasis_setup_ok_invalid_answers() {
    output=$(printf 'invalid\n' | $OASIS_CLI_BINARY init my_project 2>&1 || true)

    # assert project initialized
    assert_directory "$OASIS_PROJECT_DIR/my_project"

    # assert data directory
    assert_directory "$OASIS_DATA_DIR/oasis"
    assert_no_file "$OASIS_DATA_DIR/oasis/metrics.jsonl"

    # assert configuration files
    assert_directory "$OASIS_CONFIG_DIR/oasis"
    assert_file "$OASIS_CONFIG_DIR/oasis/config.toml"
    assert_file_contains "$OASIS_CONFIG_DIR/oasis/config.toml" "enabled = false"
    assert_file_doesnot_contain "$OASIS_CONFIG_DIR/oasis/config.toml" "enabled = true"

    return $SUCCESS
}

function test_oasis_init() {
    init_project
    assert_directory "$OASIS_PROJECT_DIR/my_project"
    return $SUCCESS
}

# test can be enabled when the build for the
# quickstart project succeeds
# function test_oasis_build() {
#     init_project

#     cd my_project
#     echo 0
#     $OASIS_CLI_BINARY build #> /dev/null 2>&1
#     echo 1

#     assert_file "$OASIS_PROJECT_DIR/my_project/target/wasm32-wasi/debug/my_project.wasm"
#     echo 2

#     return $SUCCESS
# }

function test_all() {
    for test_case in "test_oasis_setup_ok_with_telemetry"  \
                         "test_oasis_setup_ok_no_telemetry"    \
                         "test_oasis_setup_ok_invalid_answers" \
                         "test_oasis_init"; do
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
