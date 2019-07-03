use crate::utils::run_cmd_capture_output;

#[derive(Debug, PartialEq)]
pub enum Release {
    Stable,
    Nightly,
}

impl Release {
    fn parse(s: &str) -> Result<Release, failure::Error> {
        match s {
            "stable" => Ok(Release::Stable),
            "nightly" => Ok(Release::Nightly),
            _ => Err(failure::format_err!("Uknown release name {}", s)),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Toolchain {
    pub release: Release,
    pub target: String,
}

impl Toolchain {
    fn parse(s: &str) -> Result<Toolchain, failure::Error> {
        let mut it = s.splitn(2, "-");
        let release = match it.next() {
            None => return Err(failure::format_err!("Toolchain must have a release")),
            Some(s) => match Release::parse(s) {
                Ok(release) => release,
                Err(err) => return Err(err),
            },
        };

        let target = match it.next() {
            None => return Err(failure::format_err!("Toolchain must have a target")),
            Some(s) => s.trim_end_matches(" (default)"),
        };

        Ok(Toolchain {
            release: release,
            target: target.to_owned(),
        })
    }
}

#[derive(Debug)]
pub struct Tag {
    pub major: u8,
    pub minor: u8,
    pub patch: u8,
    pub release: Release,
}

impl Tag {
    fn is_at_least(&self, other: &Tag) -> bool {
        return self.release == other.release
            && (self.major > other.major
                || self.major == other.major && self.minor > other.minor
                || self.major == other.major
                    && self.minor == other.minor
                    && self.patch >= other.patch);
    }

    fn parse(s: &str) -> Result<Tag, failure::Error> {
        let mut it = s.split_terminator(|c| c == '.' || c == '-');

        let major: u8 = match it.next() {
            None => {
                return Err(failure::format_err!(
                    "Version tag must have a major version"
                ))
            }
            Some(s) => match s.parse() {
                Ok(patch) => patch,
                Err(err) => {
                    return Err(failure::format_err!(
                        "Failed to parse major version from version tag {}",
                        err
                    ))
                }
            },
        };
        let minor: u8 = match it.next() {
            None => {
                return Err(failure::format_err!(
                    "Version tag must have a minor version"
                ))
            }
            Some(s) => match s.parse() {
                Ok(patch) => patch,
                Err(err) => {
                    return Err(failure::format_err!(
                        "Failed to parse minor version from version tag with error {}",
                        err
                    ))
                }
            },
        };
        let patch: u8 = match it.next() {
            None => {
                return Err(failure::format_err!(
                    "Version tag must have a patch version"
                ))
            }
            Some(s) => match s.parse() {
                Ok(patch) => patch,
                Err(err) => {
                    return Err(failure::format_err!(
                        "Failed to parse patch version from version tag with error {}",
                        err
                    ))
                }
            },
        };
        let release = match it.next() {
            None => Release::Stable,
            Some(s) => match Release::parse(s) {
                Ok(release) => release,
                Err(err) => return Err(err),
            },
        };

        Ok(Tag {
            major: major,
            minor: minor,
            patch: patch,
            release: release,
        })
    }
}

#[derive(Debug)]
pub struct Version {
    pub exec: String,
    pub tag: Tag,
    pub hash: String,
    pub date: String,
}

impl Version {
    fn parse(s: &str) -> Result<Version, failure::Error> {
        let mut it = s.split_whitespace();
        let exec = match it.next() {
            None => return Err(failure::format_err!("Version must have an executable name")),
            Some(s) => s,
        };
        let tag = match it.next() {
            None => return Err(failure::format_err!("Version must have a tag")),
            Some(s) => match Tag::parse(s) {
                Ok(tag) => tag,
                Err(err) => return Err(err),
            },
        };
        let hash = match it.next() {
            None => return Err(failure::format_err!("Version must have a hash")),
            Some(s) => s.trim_start_matches("("),
        };
        let date = match it.next() {
            None => return Err(failure::format_err!("Version must have a date")),
            Some(s) => s.trim_end_matches(")"),
        };
        let end = it.next();
        if let Some(_) = end {
            return Err(failure::format_err!("Version string has invalid format"));
        }

        Ok(Version {
            exec: exec.to_owned(),
            tag: tag,
            hash: hash.to_owned(),
            date: date.to_owned(),
        })
    }
}

fn get_rust_version() -> Result<Version, failure::Error> {
    let output = run_cmd_capture_output("rustc", vec!["--version"])?;
    let s = String::from_utf8(output.stdout)?;
    Version::parse(&s)
}

fn get_cargo_version() -> Result<Version, failure::Error> {
    let output = run_cmd_capture_output("cargo", vec!["--version"])?;
    let s = String::from_utf8(output.stdout)?;
    Version::parse(&s)
}

fn get_rustup_toolchains() -> Result<Vec<Toolchain>, failure::Error> {
    let output = run_cmd_capture_output("rustup", vec!["toolchain", "list"])?;
    let s = String::from_utf8(output.stdout)?;
    let mut toolchains: Vec<Toolchain> = Vec::new();
    let mut it = s.split('\n');

    loop {
        if let Some(el) = it.next() {
            if el.len() > 0 {
                toolchains.push(Toolchain::parse(&el)?);
            }
        } else {
            break;
        }
    }

    Ok(toolchains)
}

pub fn setup() -> Result<String, failure::Error> {
    let minimum_rustc_version = "rustc 1.35.0-nightly (53f2165c5 2019-04-04)";
    let minimum_cargo_version = "cargo 1.35.0-nightly (63231f438 2019-03-27)";
    let required_toolchain = "nightly-x86_64-apple-darwin";

    let rustc_version = get_rust_version()?;
    if !rustc_version
        .tag
        .is_at_least(&Version::parse(minimum_rustc_version).unwrap().tag)
    {
        return Err(failure::format_err!(
            "Expected rustc version at least {}",
            minimum_rustc_version
        ));
    }

    let cargo_version = get_cargo_version()?;
    if !cargo_version
        .tag
        .is_at_least(&Version::parse(minimum_cargo_version).unwrap().tag)
    {
        return Err(failure::format_err!(
            "Expected cargo version at least {}",
            minimum_cargo_version
        ));
    }

    let toolchains = get_rustup_toolchains()?;
    if !toolchains.contains(&Toolchain::parse(required_toolchain).unwrap()) {
        return Err(failure::format_err!(
            "Required toolchain missing {}",
            required_toolchain
        ));
    }

    Ok("All dependencies are met".to_owned())
}
