const QUICKSTART_TGZ_URL: &'static str =
    "https://codeload.github.com/oasislabs/quickstart/tar.gz/master";

fn main() {
    let client = reqwest::Client::new();
    let head = client
        .head(QUICKSTART_TGZ_URL)
        .send()
        .expect("Could not HEAD quickstart.tar.gz");
    let etag = head
        .headers()
        .get(reqwest::header::ETAG)
        .expect("Missing ETag");

    let mut quickstart_path = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    quickstart_path.push(format!(
        "quickstart-{}.tar.gz",
        etag.to_str().expect("Invalid ETag").replace('"', "")
    ));
    println!(
        "cargo:rustc-env=QUICKSTART_INCLUDE_PATH={}",
        quickstart_path.display()
    );

    if quickstart_path.is_file() {
        return;
    }

    let mut quickstart_zip = std::fs::File::create(&quickstart_path).unwrap();
    reqwest::get(QUICKSTART_TGZ_URL)
        .expect("Could not GET quickstart.tar.gz")
        .copy_to(&mut quickstart_zip)
        .expect("Could not write quickstart.tar.gz");
}
