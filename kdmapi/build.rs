fn main() {
    if cfg!(windows) {
        println!("cargo:rustc-cdylib-link-arg=/DEF:Ordinals.def")
    }
}
