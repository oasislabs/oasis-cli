use colored::*;
use std::string::ToString;

pub fn error<E: ToString>(msg: &str, err: E) {
    log("ERROR", msg, err)
}

pub fn warn<E: ToString>(msg: &str, err: E) {
    log("WARN", msg, err)
}

pub fn info<E: ToString>(msg: &str, err: E) {
    log("INFO", msg, err)
}

pub fn log<E: ToString>(level: &str, msg: &str, err: E) {
    eprintln!("{}: {} `{}`", level.red(), msg, err.to_string())
}
