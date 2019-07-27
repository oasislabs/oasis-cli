const QUICKSTART_VER: &str = "0.1";
const QUICKSTART_TAGS_URL: &str = " https://api.github.com/repos/oasislabs/quickstart/tags";

#[derive(serde::Deserialize)]
struct GithubTag {
    name: String,
    tarball_url: String,
}

fn main() -> Result<(), failure::Error> {
    let version_req = semver::VersionReq::parse(QUICKSTART_VER)?;
    let tags = reqwest::get(QUICKSTART_TAGS_URL)?.json::<Vec<GithubTag>>()?;
    let tag = tags
        .iter()
        .find(|tag| {
            semver::Version::parse(&tag.name[1..] /* strip leading 'v' */)
                .map(|v| version_req.matches(&v))
                .unwrap_or_default()
        })
        .unwrap_or_else(|| panic!("No matching quickstart version for `{}`", QUICKSTART_VER));

    let mut quickstart_path = std::path::PathBuf::from(std::env::var("OUT_DIR")?);
    quickstart_path.push(format!("quickstart-{}.tar.gz", tag.name));
    println!("cargo:rustc-env=QUICKSTART_VER={}", QUICKSTART_VER);
    println!(
        "cargo:rustc-env=QUICKSTART_INCLUDE_PATH={}",
        quickstart_path.display()
    );

    if quickstart_path.is_file() {
        return Ok(());
    }

    let mut quickstart_tgz = std::fs::File::create(&quickstart_path)?;
    reqwest::get(&tag.tarball_url)?.copy_to(&mut quickstart_tgz)?;

    Ok(())
}
