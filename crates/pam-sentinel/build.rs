fn main() {
    let prefix = std::env::var("SENTINEL_PREFIX").unwrap_or_else(|_| "/usr".into());
    let sysconfdir = std::env::var("SENTINEL_SYSCONFDIR").unwrap_or_else(|_| "/etc".into());
    let libexecdir = std::env::var("SENTINEL_LIBEXECDIR").unwrap_or_else(|_| "lib".into());

    let helper_path = format!("{prefix}/{libexecdir}/sentinel-helper");
    let config_path = format!("{sysconfdir}/security/sentinel.conf");

    println!("cargo:rustc-env=SENTINEL_HELPER_PATH={helper_path}");
    println!("cargo:rustc-env=SENTINEL_CONFIG_PATH={config_path}");
    println!("cargo:rerun-if-env-changed=SENTINEL_PREFIX");
    println!("cargo:rerun-if-env-changed=SENTINEL_SYSCONFDIR");
    println!("cargo:rerun-if-env-changed=SENTINEL_LIBEXECDIR");
}
