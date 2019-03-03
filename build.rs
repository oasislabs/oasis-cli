fn cargo_install(pkg_name: &'static str, bin_name: &'static str) {
    let output = std::process::Command::new("cargo")
        .args(&["install", "xargo", "--bin", bin_name])
        .output()
        .expect(&format!("Could not `cargo install {} --bin {}`", pkg_name, bin_name));
    match output.status.code() {
        Some(0) | Some(101) => (),
        _ => panic!("{:#?} {:?}", output, output.status.code()),
    };
}

fn main() {
    cargo_install("xargo", "xargo");
    cargo_install("owasm-utils-cli", "wasm-build");
}
