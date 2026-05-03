// Bake compile-time install paths into the binary.

use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=SENTINEL_PREFIX");
    println!("cargo:rerun-if-env-changed=SENTINEL_LIBEXECDIR");
    println!("cargo:rerun-if-env-changed=SENTINEL_HELPER_PATH");

    let prefix = std::env::var("SENTINEL_PREFIX").unwrap_or_else(|_| "/usr".into());
    let libexecdir = std::env::var("SENTINEL_LIBEXECDIR").unwrap_or_else(|_| "lib".into());

    let helper_path = std::env::var("SENTINEL_HELPER_PATH").unwrap_or_else(|_| {
        PathBuf::from(&prefix)
            .join(&libexecdir)
            .join("sentinel-helper")
            .display()
            .to_string()
    });

    println!("cargo:rustc-env=SENTINEL_HELPER_PATH={helper_path}");
}
