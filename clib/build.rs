use pkg_version::*;

fn main() {
    let major = pkg_version_major!();
    let minor = pkg_version_minor!();
    let patch = pkg_version_patch!();
    let ver: u32 = patch | minor << 8 | major << 16;

    let crate_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();

    let mut config = cbindgen::Config::from_file("cbindgen.toml")
        .expect("Unable to find cbindgen.toml configuration file");
    config.after_includes = Some(format!("\n#define XSYNTH_VERSION {:#x}", ver));

    cbindgen::Builder::new()
        .with_config(config)
        .with_crate(crate_dir)
        .generate()
        .expect("Unable to generate bindings")
        .write_to_file("xsynth.h");
}
