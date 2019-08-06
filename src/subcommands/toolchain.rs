use std::{collections::BTreeSet, fs, io::Read, path::Path, str::FromStr};

use crate::{error::Error, oasis_dir, utils};

static TOOLS_URL: &str = "https://tools.oasis.dev";
const OASIS_GENESIS_YEAR: u8 = 19;
const WEEKS_IN_YEAR: u8 = 54;
const INSTALLED_RELEASE_FILE: &str = "installed_release";

cfg_if::cfg_if! {
    if #[cfg(target_os = "linux")] {
        static PLATFORM: &str = "linux";
    } else if #[cfg(target_os = "macos")] {
        static PLATFORM: &str = "darwin";
    } else {
        compile_error!("`oasis-cli` does not support your platform. Thanks for trying!");
    }
}

pub fn set(version: &str) -> Result<(), failure::Error> {
    let bin_dir = crate::dirs::bin_dir();
    let cache_dir = oasis_dir!(cache)?;
    let installed_release_file = oasis_dir!(data)?.join(INSTALLED_RELEASE_FILE);

    let requested_version = ReleaseVersion::from_str(version)?;

    let installed_release: Release = fs::read(&installed_release_file)
        .ok()
        .and_then(|f| serde_json::from_slice(&f).ok())
        .unwrap_or_default();

    if requested_version
        .name()
        .map(|n| n == installed_release.name)
        .unwrap_or_default()
    {
        println!("{} is up-to-date", version);
        return Ok(());
    }

    let release = match Release::for_version(requested_version) {
        Ok(Some(release)) => release,
        Ok(None) => return Err(Error::UnknownToolchain(version.to_string()).into()),
        Err(e) => return Err(failure::format_err!("could not fetch releases: {}", e)),
    };

    if release == installed_release {
        println!("{} is up-to-date", version);
        return Ok(());
    }

    for tool in release.tools.iter() {
        utils::print_status_ctx(utils::Status::Downloading, &tool.name, &tool.ver);
        tool.fetch(&cache_dir)
            .map_err(|e| failure::format_err!("could not download {}: {}", tool.name, e))?;
    }
    for tool in release.tools.iter() {
        fs::rename(cache_dir.join(&tool.name_ver), bin_dir.join(&tool.name))?;
    }

    fs::write(
        installed_release_file,
        serde_json::to_string_pretty(&release).unwrap(),
    )
    .ok(); // This isn't catastropic. We'll just have to re-download later.

    Ok(())
}

#[derive(Clone, Debug)]
enum ReleaseVersion {
    Latest,
    Unstable,
    Named { name: String, year: u8, week: u8 },
}

impl ReleaseVersion {
    fn name(&self) -> Option<&str> {
        match self {
            ReleaseVersion::Named { name, .. } => Some(name),
            _ => None,
        }
    }
}

impl Default for ReleaseVersion {
    fn default() -> Self {
        Self::Named {
            name: "".to_string(),
            year: 0,
            week: 0,
        }
    }
}

impl PartialEq for ReleaseVersion {
    fn eq(&self, other: &ReleaseVersion) -> bool {
        use ReleaseVersion::*;
        match (self, other) {
            (Unstable, Unstable) | (Latest, Latest) => true,
            (Named { name: sn, .. }, Named { name: on, .. }) => sn == on,
            _ => false, // `Latest` and `Named` are incomparable without first fetching versions
        }
    }
}

impl PartialOrd for ReleaseVersion {
    fn partial_cmp(&self, other: &ReleaseVersion) -> Option<std::cmp::Ordering> {
        use std::cmp::Ordering;
        use ReleaseVersion::*;
        if self == other {
            return Some(Ordering::Equal);
        }
        match (self, other) {
            (_, Unstable) => Some(Ordering::Less),
            (Unstable, _) => Some(Ordering::Greater),
            (
                Named {
                    year: sy, week: sw, ..
                },
                Named {
                    year: oy, week: ow, ..
                },
            ) => (sy, sw).partial_cmp(&(oy, ow)),
            _ => None, // `Latest` and `Named` are incomparable without first fetching versions
        }
    }
}

impl FromStr for ReleaseVersion {
    type Err = failure::Error;

    fn from_str(version: &str) -> Result<Self, Self::Err> {
        Ok(match version {
            "latest" => ReleaseVersion::Latest,
            "latest-unstable" => ReleaseVersion::Unstable,
            _ => match version.split('.').collect::<Vec<_>>().as_slice() {
                [year, week] => {
                    let year = u8::from_str(year).unwrap_or(0);
                    let week = u8::from_str(week).unwrap_or(0);
                    if (OASIS_GENESIS_YEAR..=current_year()).contains(&year)
                        && week <= WEEKS_IN_YEAR
                    {
                        ReleaseVersion::Named {
                            name: version.to_string(),
                            week,
                            year,
                        }
                    } else {
                        return Err(Error::UnknownToolchain(version.to_string()).into());
                    }
                }
                _ => return Err(Error::UnknownToolchain(version.to_string()).into()),
            },
        })
    }
}

#[derive(Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Release {
    name: String,
    tools: BTreeSet<Tool>,
}

impl Release {
    fn for_version(version: ReleaseVersion) -> Result<Option<Release>, failure::Error> {
        use xml::reader::{EventReader, XmlEvent};

        let mut tools = BTreeSet::new(); // there aren't many tools

        let tools_parser = EventReader::new(fetch_tools_xml()?);
        let mut in_key_tag = false;
        let mut target_version = if version == ReleaseVersion::Latest {
            ReleaseVersion::default()
        } else {
            version.clone()
        };
        for e in tools_parser {
            match e {
                Ok(XmlEvent::StartElement { name, .. }) => {
                    in_key_tag = name.local_name == "Key";
                }
                Ok(XmlEvent::Characters(s3_key)) => {
                    if !in_key_tag || !s3_key.starts_with(PLATFORM) {
                        continue;
                    }
                    let mut spec = s3_key.split('/').skip(1); // skip <platform>
                    let tool_stage = spec.next().unwrap();

                    if target_version == ReleaseVersion::Unstable {
                        if tool_stage == "current" {
                            tools.insert(Tool::from_str(&s3_key).unwrap());
                        }
                        continue;
                    }
                    if tool_stage != "release" {
                        continue;
                    }
                    let tool_ver = ReleaseVersion::from_str(spec.next().unwrap()).unwrap();
                    if version == ReleaseVersion::Latest && target_version < tool_ver {
                        tools.clear();
                        target_version = tool_ver.clone();
                    }
                    if tool_ver == target_version {
                        tools.insert(Tool::from_str(&s3_key).unwrap());
                    }
                }
                Ok(XmlEvent::EndElement { .. }) => {
                    in_key_tag = false;
                }
                Err(_) => {
                    return Err(failure::format_err!(
                        "unable to fetch tool versions. \
                         Try checking https://status.oasis.dev for system status."
                    ))
                }
                _ => (),
            }
        }
        Ok(if !tools.is_empty() {
            Some(Release {
                name: if version == ReleaseVersion::Unstable {
                    "unstable".to_string()
                } else {
                    target_version.name().unwrap().to_string()
                },
                tools,
            })
        } else {
            None
        })
    }
}

#[derive(Debug, Eq, Ord)]
struct Tool {
    name: String,
    ver: String,
    name_ver: String,
    s3_key: String,
}

impl PartialEq for Tool {
    fn eq(&self, other: &Tool) -> bool {
        self.name_ver == other.name_ver
    }
}

impl PartialOrd for Tool {
    fn partial_cmp(&self, other: &Tool) -> Option<std::cmp::Ordering> {
        self.name_ver.partial_cmp(&other.name_ver)
    }
}

impl Tool {
    fn fetch(&self, out_dir: &Path) -> Result<(), failure::Error> {
        let out_path = out_dir.join(&self.name_ver);
        if out_path.exists() {
            return Ok(());
        }
        let mut res = reqwest::get(TOOLS_URL)?;
        let mut f = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .open(out_dir.join(&self.name_ver))?;
        res.copy_to(&mut f)?;
        Ok(())
    }
}

impl FromStr for Tool {
    type Err = failure::Error;

    fn from_str(s3_key: &str) -> Result<Self, Self::Err> {
        s3_key
            .rsplitn(2, '/')
            .nth(0) // `rsplitn` reverses, so this is actually the last component
            .and_then(
                |tool_hash| match tool_hash.rsplitn(2, '-').collect::<Vec<_>>().as_slice() {
                    [hash, tool] => Some(Tool {
                        // This is way too much copying, but doing otherwise would impair
                        // maintainability. Try speeding up the network connection instead.
                        name: tool.to_string(),
                        ver: hash.to_string(),
                        name_ver: tool_hash.to_string(),
                        s3_key: s3_key.to_string(),
                    }),
                    _ => None,
                },
            )
            .ok_or_else(|| failure::format_err!("invalid tool key: `{}`", s3_key))
    }
}

// If we're going to do the easy thing and store duplicated strings in `Tool`, at least
// don't store it for the user to think that we're the struct's namesake.
impl serde::Serialize for Tool {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.s3_key)
    }
}

impl<'de> serde::Deserialize<'de> for Tool {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s3_key = String::deserialize(deserializer)?;
        let name_ver = s3_key.rsplitn(2, '/').nth(0).unwrap();
        let ver_name = name_ver.rsplitn(2, '-').collect::<Vec<_>>();
        Ok(Tool {
            name_ver: name_ver.to_string(),
            name: ver_name[1].to_string(),
            ver: ver_name[0].to_string(),
            s3_key,
        })
    }
}

#[cfg(not(test))]
fn fetch_tools_xml() -> Result<impl Read, failure::Error> {
    Ok(reqwest::get(TOOLS_URL)?)
}

#[cfg(test)]
fn fetch_tools_xml() -> Result<impl Read, failure::Error> {
    Ok(std::io::Cursor::new(format!(
        r#"<Test>
            <Key>{0}/cache/oasis-abcdef</Key>
            <Key>{0}/current/oasis-0a515</Key>
            <Key>{0}/release/19.36/oasis-tool-ae5b4f</Key>
            <Key>{0}/release/20.34/oasis-chain-7777777</Key>
            <Key>{0}/release/19.36/oasis-tool2-ae5b4f</Key>
            <Key>{0}/release/20.34/oasis-build-build-123456</Key>
            <Key>{0}/current/oasis-build-c0deaf</Key>
        </Test>"#,
        PLATFORM
    )))
}

#[cfg(not(test))]
fn current_year() -> u8 {
    use chrono::Datelike as _;
    (chrono::Utc::now().year() % 100) as u8
}

#[cfg(test)]
fn current_year() -> u8 {
    99 // Let's be real: none of us are going to be around when this causes the tests to fail.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_parse() {
        assert_eq!(
            ReleaseVersion::from_str("latest").unwrap(),
            ReleaseVersion::Latest
        );
        assert_eq!(
            ReleaseVersion::from_str("latest-unstable").unwrap(),
            ReleaseVersion::Unstable
        );
        assert_eq!(
            ReleaseVersion::from_str("19.36").unwrap(),
            ReleaseVersion::Named {
                name: "19.36".to_string(),
                year: 19,
                week: 36
            }
        );
        assert!(ReleaseVersion::from_str("19.55").is_err(),);
    }

    #[test]
    fn test_version_ord() {
        let named_early = ReleaseVersion::from_str("19.36").unwrap();
        let named_late = ReleaseVersion::from_str("20.10").unwrap();
        let latest = ReleaseVersion::Latest;
        let unstable = ReleaseVersion::Unstable;

        assert!(named_early < unstable);
        assert!(named_late < unstable);
        assert!(latest < unstable);

        assert!(named_early < named_late);
        assert!(named_early.partial_cmp(&latest).is_none());
    }

    #[test]
    fn test_release_for_version_unstable() {
        let r = Release::for_version(ReleaseVersion::Unstable)
            .unwrap()
            .unwrap();
        assert_eq!(r.name, "unstable");
        assert_eq!(r.tools.len(), 2);
        assert!(r
            .tools
            .iter()
            .any(|t| t.name == "oasis" && t.s3_key.ends_with("0a515")));
        assert!(r
            .tools
            .iter()
            .any(|t| t.name == "oasis-build" && t.s3_key.ends_with("c0deaf")));
    }

    #[test]
    fn test_release_for_version_latest() {
        let r = Release::for_version(ReleaseVersion::Latest)
            .unwrap()
            .unwrap();
        assert_eq!(r.name, "20.34");
        assert_eq!(r.tools.len(), 2);
        assert!(r
            .tools
            .iter()
            .any(|t| t.name == "oasis-chain" && t.s3_key.ends_with("7777777")));
        assert!(r
            .tools
            .iter()
            .any(|t| t.name == "oasis-build-build" && t.s3_key.ends_with("123456")));
    }

    #[test]
    fn test_release_for_version_named() {
        let r = Release::for_version(ReleaseVersion::Named {
            name: "19.36".to_string(),
            year: 19,
            week: 36,
        })
        .unwrap()
        .unwrap();
        assert_eq!(r.name, "19.36");
        assert_eq!(r.tools.len(), 2);
        assert!(r
            .tools
            .iter()
            .any(|t| t.name == "oasis-tool" && t.s3_key.ends_with("ae5b4f")));
        assert!(r
            .tools
            .iter()
            .any(|t| t.name == "oasis-tool2" && t.s3_key.ends_with("ae5b4f")));
    }
}
