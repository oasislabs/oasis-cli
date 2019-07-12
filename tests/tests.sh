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
    DIR=$1
    if [ ! -d $DIR ]; then
        echo "directory "$DIR" does not exist"
        exit
    fi
}

function assert_file() {
    FILE=$1
    if [ ! -f $FILE ]; then
        echo "file "$FILE" does not exist"
        exit
    fi
}

function test_oasis_first_run() {
    echo 'y\n12345\n' | $OASIS_CLI_BINARY init my_project > /dev/null 2>&1

    assert_directory $OASIS_CONFIG_DIR
    assert_directory $OASIS_CONFIG_DIR/oasis
    assert_directory $OASIS_CONFIG_DIR/oasis/log
    assert_file $OASIS_CONFIG_DIR/oasis/config
    assert_directory $OASIS_TEST_PROJECTS

    echo $SUCCESS
}

function test_oasis_init() {
    echo 'y\n12345\n' | $OASIS_CLI_BINARY init my_project > /dev/null 2>&1

    assert_directory $OASIS_TEST_PROJECTS/my_project

    echo $SUCCESS
}

function test_oasis_build() {
    echo 'y\n12345\n' | $OASIS_CLI_BINARY init my_project > /dev/null 2>&1
    cd my_project
    $OASIS_CLI_BINARY build > /dev/null 2>&1

    assert_file $OASIS_TEST_PROJECTS/my_project/target/wasm32-wasi/debug/my_project.wasm

    echo $SUCCESS
}

function test_all() {
    for test_case in "test_oasis_first_run" "test_oasis_init" "test_oasis_build"; do
        before
        code=$($test_case)
        if [[ "$code" == "0" ]]; then
            echo "TEST $test_case succeeded"
        else
            echo "TEST $test_case failed with code "$code
        fi
        after
    done
}
