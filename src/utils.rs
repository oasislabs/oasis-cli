use std::{fmt, path::Path};

use colored::*;

pub enum Status {
    Fresh,
    Building,
    Preparing,
    Testing,
    Deploying,
    Downloading,
    Created,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{: >12}",
            match self {
                Self::Fresh => "Fresh".green(),
                Self::Building => "Building".cyan(),
                Self::Preparing => "Preparing".cyan(),
                Self::Testing => "Testing".cyan(),
                Self::Deploying => "Deploying".cyan(),
                Self::Downloading => "Downloading".cyan(),
                Self::Created => "Created".green(),
            }
        )
    }
}

pub fn print_status(status: Status, what: impl fmt::Display) {
    print_status_ctx(status, what, "");
}

pub fn print_status_in(status: Status, what: impl fmt::Display, whence: &Path) {
    let cwd = std::env::current_dir().unwrap();
    print_status_ctx(
        status,
        what,
        whence
            .strip_prefix(cwd)
            .unwrap_or_else(|_| Path::new(""))
            .display(),
    );
}

pub fn print_status_ctx(status: Status, what: impl fmt::Display, ctx: impl fmt::Display) {
    eprint!("{} {}", status, what.to_string());
    let ctx_str = ctx.to_string();
    if !ctx_str.is_empty() {
        eprintln!(" ({})", ctx_str);
    } else {
        eprintln!();
    }
}

pub mod http {
    use reqwest::{blocking::RequestBuilder, header::HeaderMap, Error, IntoUrl, Url};

    pub struct ClientBuilder {
        url: Result<Url, Error>,
        inner: reqwest::blocking::ClientBuilder,
    }

    pub struct Client {
        url: Url,
        inner: reqwest::blocking::Client,
    }

    impl ClientBuilder {
        pub fn new(url: impl IntoUrl) -> Self {
            Self {
                url: url.into_url().map(|url| {
                    if cfg!(debug_assertions) {
                        let mut url = url;
                        url.set_scheme("http").unwrap();
                        url
                    } else {
                        assert!(url.scheme() == "https");
                        url
                    }
                }),
                inner: reqwest::blocking::Client::builder(),
            }
        }

        pub fn build(self) -> Result<Client, Error> {
            let client = self.inner.build()?;
            Ok(Client {
                url: self.url?,
                inner: client,
            })
        }

        pub fn default_headers(mut self, headers: HeaderMap) -> Self {
            self.inner = self.inner.default_headers(headers);
            self
        }
    }

    impl Client {
        pub fn get(&self, extension: &str) -> RequestBuilder {
            self.inner.get(self.url.join(extension).unwrap())
        }

        pub fn post(&self, extension: &str) -> RequestBuilder {
            self.inner.post(self.url.join(extension).unwrap())
        }
    }
}
