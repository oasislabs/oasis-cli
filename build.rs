const TEMPLATE_REPO_URL: &str = "https://github.com/oasislabs/template";
const TEMPLATE_VER: &str = "0.3";

fn main() -> anyhow::Result<()> {
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR")?);
    let template_repo_path = out_dir.join("template");
    let template_path = out_dir.join("template.tar.gz");

    println!("cargo:rustc-env=TEMPLATE_VER={}", TEMPLATE_VER);
    println!(
        "cargo:rustc-env=TEMPLATE_INCLUDE_PATH={}",
        template_path.display()
    );

    macro_rules! git {
        ($main:expr, $( $arg:expr ),+) => {{
            let mut cmd = std::process::Command::new("git");
            cmd.arg($main);
            $( cmd.arg($arg); )+
            if $main != "clone" {
                cmd.current_dir(&template_repo_path);
            }
            cmd.output()
        }}
    }

    if !template_repo_path.is_dir() {
        git!("clone", TEMPLATE_REPO_URL, &template_repo_path)?;
    }
    git!("fetch", "origin", "--tags")?;

    let version_req = semver::VersionReq::parse(TEMPLATE_VER)?;
    let tags_str = String::from_utf8(git!("tag", "-l", "v*.*.*")?.stdout)?;
    let best_tag = tags_str
        .trim()
        .split('\n')
        .filter_map(|t| {
            let ver = semver::Version::parse(&t[1..]).expect(t);
            if version_req.matches(&ver) {
                Some((ver, t))
            } else {
                None
            }
        })
        .max()
        .unwrap()
        .1;

    git!("archive", best_tag, "-o", template_path)?;

    Ok(())
}
