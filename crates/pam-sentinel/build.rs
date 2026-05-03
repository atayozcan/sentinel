// SENTINEL_CONFIG_PATH lives in `sentinel-shared`'s build.rs (shared
// with the agent). This crate only needs the helper path baked in.

fn main() {
    let prefix = std::env::var("SENTINEL_PREFIX").unwrap_or_else(|_| "/usr".into());
    let libexecdir = std::env::var("SENTINEL_LIBEXECDIR").unwrap_or_else(|_| "lib".into());

    let helper_path = format!("{prefix}/{libexecdir}/sentinel-helper");

    println!("cargo:rustc-env=SENTINEL_HELPER_PATH={helper_path}");
    println!("cargo:rerun-if-env-changed=SENTINEL_PREFIX");
    println!("cargo:rerun-if-env-changed=SENTINEL_LIBEXECDIR");
}
