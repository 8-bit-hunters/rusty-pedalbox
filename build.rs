fn main() {
    println!("cargo:rustc-link-arg=--nmagic");
    println!("cargo:rustc-link-arg=-Tlink.x");
    #[cfg(feature = "defmt")]
    println!("cargo:rustc-link-arg=-Tdefmt.x");
}
