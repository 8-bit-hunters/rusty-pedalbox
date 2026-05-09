use std::env;
use std::path::PathBuf;

fn main() {
    let out = &PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    println!("cargo:rustc-link-search={}", out.join("memory").display());

    println!("cargo:rustc-link-arg=-Tdefmt.x");
}
