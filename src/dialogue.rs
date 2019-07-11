use std::io::{self, Write as _};

pub fn confirm(question: &str, default: bool) -> Result<bool, failure::Error> {
    print!("{} ", question);
    let mut s = String::new();
    io::stdout().flush()?;
    let _ = io::stdin().read_line(&mut s)?;
    let s = s.trim_end().to_string();

    let r = match &*s.to_lowercase() {
        "y" | "yes" => true,
        "n" | "no" => false,
        "" => default,
        _ => false,
    };

    println!();

    Ok(r)
}

pub fn ask_string(question: &str) -> Result<String, failure::Error> {
    let mut s = String::new();

    println!("{}", question);
    let _ = io::stdin().read_line(&mut s)?;
    Ok(s.trim_end().to_string())
}

pub fn introduction() {
    println!("Welcome to Oasis Development Environment!");
    println!("In next few steps, we will configure your settings.");
    println!();
    println!("We hope to collect telemetry data from the logging generated from your usage. This telemetry data");
    println!("will give us usage insights and be able to guide our engineering team to improve your experience.");
}
