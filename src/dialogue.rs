use std::io::Write as _;

use colored::*;

use crate::errors::Error;

pub fn introduction() {
    println!("Welcome to the Oasis Development Environment!");
    println!();
    println!("You don't seem to have a config file yet. Let's set that up now.");
    println!();
}

pub fn prompt_telemetry(telemetry_path: &std::path::Path) -> Result<bool, Error> {
    println!("{}", "1. Telemetry\n".bold().white());
    println!(
        "Would you like to help build a better developer experience by enabling telemetry?\n\
         This tool will collect anonymous usage stats that won't be shared with third parties.\n\
         You can find your data in `{}` and can change your opt-in\n\
         status by running `{} config telemetry enable/disable`.\n",
        telemetry_path.display(),
        std::env::args().next().unwrap()
    );
    confirm("Enable telemetry?", false)
}

fn confirm(question: &str, default: bool) -> Result<bool, Error> {
    let yn = if default { " (Y/n)" } else { " (y/N)" };

    let mut prompt = String::with_capacity(question.len() + yn.len());
    prompt += question;
    prompt += yn;

    let response = prompt_yn(&prompt, default)?;

    println!();

    Ok(response)
}

pub fn ask_string(question: &str) -> Result<String, Error> {
    let mut s = String::new();
    print!("{} ", question);
    std::io::stdout().flush()?;
    std::io::stdin().read_line(&mut s)?;
    Ok(s.trim_end().to_string())
}

fn prompt_yn(prompt: &str, default: bool) -> Result<bool, Error> {
    Ok(loop {
        break match ask_string(prompt)?.to_lowercase().as_str() {
            "y" | "yes" | "true" => true,
            "n" | "no" | "false" => false,
            "" => default,
            _ => continue,
        };
    })
}
