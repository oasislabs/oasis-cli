use std::str::FromStr;

use chrono::Datelike as _;

use crate::error::Error;

static TOOLS_URL: &str = "https://tools.oasis.dev";

cfg_if::cfg_if! {
    if #[cfg(target_os = "linux")] {
        static PLATFORM: &str = "linux";
    } else if #[cfg(target_os = "macos")] {
        static PLATFORM: &str = "darwin";
    } else {
        compile_error!(
            "`oasis-cli` does not support your platform. How did you even get this error?");
    }
}

pub fn set(version: &str) -> Result<(), failure::Error> {
    let release = fetch_release(ReleaseVersion::from_str(version)?)?;
    Ok(())
}

fn fetch_release(version: ReleaseVersion) -> Result<Release, failure::Error> {
    use xml::reader::{EventReader, XmlEvent};
    let mut tools = Vec::with_capacity(4); // there aren't many tools
    let tools_parser = EventReader::new(reqwest::get(TOOLS_URL)?);
    let mut in_key = false;
    for e in tools_parser {
        match e {
            Ok(XmlEvent::StartElement { name, .. }) => {
                in_key = name.local_name == "Key";
            }
            Ok(XmlEvent::Characters(s3_key)) => {
                if !in_key || !s3_key.starts_with(PLATFORM) {
                    continue;
                }
                let mut spec = s3_key.split('/').skip(1); // skip <platform>
                let release_state = spec.next().unwrap();
                if (version == ReleaseVersion::Unstable && release_state != "current")
                    || (version != ReleaseVersion::Unstable && release_state != "release")
                {
                    continue;
                }
                println!("{}", s3_key);
            }
            Ok(XmlEvent::EndElement { name }) => {
                in_key = false;
            }
            Err(e) => {
                return Err(failure::format_err!(
                    "unable to fetch tool versions. \
                     Try checking https://toolstate.oasis.dev for system status."
                ))
            }
            _ => (),
        }
    }
    Ok(Release {
        name: "current".to_string(),
        tools: Vec::new(),
    })
}

#[derive(PartialEq, Eq)]
enum ReleaseVersion {
    Latest,
    Unstable,
    Named(String),
}

impl FromStr for ReleaseVersion {
    type Err = Error;

    fn from_str(version: &str) -> Result<Self, Self::Err> {
        Ok(match version {
            "latest" => Self::Latest,
            "latest-unstable" => Self::Unstable,
            _ => match version.split('.').collect::<Vec<_>>().as_slice() {
                [year, week]
                    if (19..=(chrono::Utc::now().year() % 100) as u64)
                        .contains(&u64::from_str(year).unwrap_or(0))
                        && u64::from_str(week).unwrap_or(0) <= 54 =>
                {
                    Self::Named(version.to_string())
                }
                _ => return Err(Error::UnknownToolchain(version.to_string())),
            },
        })
    }
}

struct Release {
    name: String,
    tools: Vec<Tool>,
}

struct Tool {
    name: String,
    s3_key: String,
}
