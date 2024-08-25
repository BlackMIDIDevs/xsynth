fn main() {
    if cfg!(windows) {
        // Building on Windows
        println!("cargo:rustc-cdylib-link-arg=/DEF:Ordinals.def")
    } else if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        // Cross building for Windows
        println!("cargo:rustc-cdylib-link-arg=Ordinals.def")
    }
}
