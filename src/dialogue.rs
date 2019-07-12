use std::io::{self, Write as _};

pub trait LineRead {
    fn read_line(&mut self, buf: &mut String) -> io::Result<usize>;
}

pub struct StdinRead;

impl LineRead for StdinRead {
    fn read_line(&mut self, buf: &mut String) -> io::Result<usize> {
        io::stdin().read_line(buf)
    }
}

pub struct Dialoguer {
    reader: Box<dyn LineRead>,
    output: bool,
}

impl Dialoguer {
    pub fn new() -> Self {
        Dialoguer {
            reader: box StdinRead {},
            output: true,
        }
    }

    pub fn new_with_reader(reader: Box<dyn LineRead>, output: bool) -> Self {
        Dialoguer { reader, output }
    }

    pub fn confirm(&mut self, question: &str, default: bool) -> Result<bool, failure::Error> {
        if self.output {
            print!("{} ", question);
        }

        let mut s = String::new();
        io::stdout().flush()?;
        let _ = self.reader.read_line(&mut s)?;
        let s = s.trim_end().to_string();

        println!("RECEIVED: {}", s);
        let r = match &*s.to_lowercase() {
            "y" | "yes" => true,
            "n" | "no" => false,
            "" => default,
            _ => false,
        };

        println!();

        Ok(r)
    }

    pub fn ask_string(&mut self, question: &str) -> Result<String, failure::Error> {
        let mut s = String::new();

        if self.output {
            println!("{}", question);
        }
        let _ = self.reader.read_line(&mut s)?;
        Ok(s.trim_end().to_string())
    }

    pub fn introduction(&self) {
        if self.output {
            println!("Welcome to Oasis Development Environment!");
            println!("In next few steps, we will configure your settings.");
            println!();
            println!("We hope to collect telemetry data from the logging generated from your usage. This telemetry data");
            println!("will give us usage insights and be able to guide our engineering team to improve your experience.");
        }
    }
}
