fn main() {
    if cfg!(windows) {
        println!("cargo:rustc-cdylib-link-arg=/DEF:./kdmapi/Ordinals.def")
    }
}
