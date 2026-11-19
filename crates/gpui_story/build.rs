use std::env;

const WINDOWS_MAIN_STACK_BYTES: usize = 8 * 1024 * 1024;

fn main() {
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bins=/STACK:{WINDOWS_MAIN_STACK_BYTES}");
    }
}
