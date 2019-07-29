const TEMPLATE_VER: &str = "0.2";
const TEMPLATE_TAGS_URL: &str = " https://api.github.com/repos/oasislabs/template/tags";

#[derive(serde::Deserialize)]
struct GithubTag {
    name: String,
    tarball_url: String,
}

fn main() -> Result<(), failure::Error> {
    let version_req = semver::VersionReq::parse(TEMPLATE_VER)?;
    let tags = reqwest::get(TEMPLATE_TAGS_URL)?.json::<Vec<GithubTag>>()?;
    let tag = tags
        .iter()
        .find(|tag| {
            semver::Version::parse(&tag.name[1..] /* strip leading 'v' */)
                .map(|v| version_req.matches(&v))
                .unwrap_or_default()
        })
        .unwrap_or_else(|| panic!("No matching template version for `{}`", TEMPLATE_VER));

    let mut template_path = std::path::PathBuf::from(std::env::var("OUT_DIR")?);
    template_path.push(format!("template-{}.tar.gz", tag.name));
    println!("cargo:rustc-env=TEMPLATE_VER={}", TEMPLATE_VER);
    println!(
        "cargo:rustc-env=TEMPLATE_INCLUDE_PATH={}",
        template_path.display()
    );

    if template_path.is_file() {
        return Ok(());
    }

    let mut template_tgz = std::fs::File::create(&template_path)?;
    reqwest::get(&tag.tarball_url)?.copy_to(&mut template_tgz)?;

    Ok(())
}
