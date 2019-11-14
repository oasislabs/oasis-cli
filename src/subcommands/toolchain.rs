use std::{
    collections::BTreeSet, fs, io::Read, os::unix::fs::PermissionsExt as _, path::Path,
    str::FromStr,
};

use crate::{
    errors::{CliError, Error},
    oasis_dir, utils,
};

const OASIS_GENESIS_YEAR: u8 = 19;
const WEEKS_IN_YEAR: u8 = 54;
const INSTALLED_RELEASE_FILE: &str = "installed_release";
const TOOLS_URL: &str = "https://tools.oasis.dev";

cfg_if::cfg_if! {
    if #[cfg(target_os = "linux")] {
        static PLATFORM: &str = "linux";
    } else if #[cfg(target_os = "macos")] {
        static PLATFORM: &str = "darwin";
    } else {
        compile_error!("`oasis-cli` does not support your platform. Thanks for trying!");
    }
}

pub fn installed_release() -> Result<Release, Error> {
    let installed_release_file = oasis_dir!(data)?.join(INSTALLED_RELEASE_FILE);
    Ok(serde_json::from_slice(&fs::read(installed_release_file)?)?)
}

pub fn set(version: &str) -> Result<(), Error> {
    if version == "current" {
        // ^ This is effectively a post-install hook.
        let rustup = std::env::var("CARGO_HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| crate::dirs::home_dir().join(".cargo"))
            .join("bin/rustup");
        crate::cli::gen_completions()?;
        crate::cmd!(
            &rustup.to_str().unwrap(),
            "toolchain",
            "install",
            crate::rust_toolchain!()
        )?;
        crate::cmd!(
            &rustup.to_str().unwrap(),
            "target",
            "add",
            "wasm32-wasi",
            "--toolchain",
            crate::rust_toolchain!()
        )?;
        return Ok(());
    }

    let bin_dir = crate::ensure_dir!(bin)?;
    let cache_dir = oasis_dir!(cache)?;

    let requested_version = ReleaseVersion::from_str(version)?;

    let installed_release = installed_release().unwrap_or_default();

    if requested_version
        .name()
        .map(|n| n == installed_release.name)
        .unwrap_or_default()
    {
        println!("{} is up-to-date", version);
        return Ok(());
    }

    let tools_client = ToolsClient::new()?;

    let release = match Release::for_version(requested_version, tools_client.fetch_manifest()?) {
        Some(release) => release,
        None => return Err(CliError::UnknownToolchain(version.to_string()).into()),
    };

    if release == installed_release {
        println!("{} is up-to-date", version);
        return Ok(());
    }

    for tool in release.tools.iter() {
        utils::print_status_ctx(utils::Status::Downloading, &tool.name, &tool.ver);
        tools_client
            .fetch_tool(&tool, &cache_dir)
            .map_err(|e| anyhow!("could not download {}: {}", tool.name, e))?;
    }
    for tool in release.tools.iter() {
        let dest = bin_dir.join(&tool.name);
        fs::rename(cache_dir.join(&tool.name_ver), &dest)
            .unwrap_or_else(|_| panic!("{:?} {:?} {:?}", tool, cache_dir, bin_dir));
        let mut perms = fs::metadata(&dest)?.permissions();
        perms.set_mode(0o755 /* o+rwd,ag+rx */);
        fs::set_permissions(dest, perms)?;
    }

    fs::write(
        oasis_dir!(data)?.join(INSTALLED_RELEASE_FILE),
        serde_json::to_string_pretty(&release).unwrap(),
    )
    .ok(); // This isn't catastropic. We'll just have to re-download later.

    crate::cmd!(
        std::env::args().nth(0).unwrap(), /* oasis */
        "set-toolchain",
        "current"
    )?;

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
    type Err = Error;

    fn from_str(version: &str) -> Result<Self, Self::Err> {
        Ok(match version {
            "latest" => ReleaseVersion::Latest,
            "unstable" => ReleaseVersion::Unstable,
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
                        return Err(CliError::UnknownToolchain(version.to_string()).into());
                    }
                }
                _ => return Err(CliError::UnknownToolchain(version.to_string()).into()),
            },
        })
    }
}

#[derive(Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Release {
    name: String,
    tools: BTreeSet<Tool>,
}

impl Release {
    pub fn name(&self) -> &str {
        &self.name
    }

    fn for_version(version: ReleaseVersion, tools_manifest: impl Read) -> Option<Release> {
        use xml::reader::{EventReader, XmlEvent};

        let mut tools = BTreeSet::new(); // there aren't many tools

        let tools_parser = EventReader::new(tools_manifest);
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
                Err(_) => return None,
                _ => (),
            }
        }
        if !tools.is_empty() {
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
        }
    }
}

#[derive(Debug, Eq, Ord)]
pub struct Tool {
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

impl FromStr for Tool {
    type Err = Error;

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
            .ok_or_else(|| anyhow!("invalid tool key: `{}`", s3_key))
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

struct ToolsClient(utils::http::Client);

impl ToolsClient {
    fn new() -> Result<Self, reqwest::Error> {
        Ok(Self(utils::http::ClientBuilder::new(TOOLS_URL).build()?))
    }

    #[cfg(not(test))]
    fn fetch_manifest(&self) -> Result<impl Read, Error> {
        Ok(self
            .0
            .get("")
            .send()
            .map_err(|e| anyhow!("could not fetch releases: {}", e))?)
    }

    #[cfg(test)]
    fn fetch_manifest(&self) -> Result<impl Read, Error> {
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

    fn fetch_tool(&self, tool: &Tool, out_dir: &Path) -> Result<(), Error> {
        let out_path = out_dir.join(&tool.name_ver);
        if out_path.exists() {
            return Ok(());
        }
        let mut res = self.0.get(&tool.s3_key).send()?;
        let mut f = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .open(out_dir.join(&tool.name_ver))?;
        res.copy_to(&mut f)?;
        Ok(())
    }
}

#[cfg(not(test))]
fn current_year() -> u8 {
    chrono::Datelike::year(&chrono::Utc::now()) as u8 % 100
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
            ReleaseVersion::from_str("unstable").unwrap(),
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
        assert!(ReleaseVersion::from_str("19.55").is_err());
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
        let tools_xml = ToolsClient::new().unwrap().fetch_manifest().unwrap();
        let r = Release::for_version(ReleaseVersion::Unstable, tools_xml).unwrap();
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
        let tools_xml = ToolsClient::new().unwrap().fetch_manifest().unwrap();
        let r = Release::for_version(ReleaseVersion::Latest, tools_xml).unwrap();
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
        let tools_xml = ToolsClient::new().unwrap().fetch_manifest().unwrap();
        let r = Release::for_version(
            ReleaseVersion::Named {
                name: "19.36".to_string(),
                year: 19,
                week: 36,
            },
            tools_xml,
        )
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
