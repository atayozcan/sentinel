// Bake compile-time install-path env vars into the binary.

use std::path::PathBuf;

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn main() {
    println!("cargo:rerun-if-env-changed=SENTINEL_PREFIX");
    println!("cargo:rerun-if-env-changed=SENTINEL_LIBEXECDIR");
    println!("cargo:rerun-if-env-changed=SENTINEL_HELPER_PATH");
    println!("cargo:rerun-if-env-changed=POLKIT_AGENT_HELPER_CANDIDATES");

    let prefix = env_or("SENTINEL_PREFIX", "/usr");
    let libexecdir = env_or("SENTINEL_LIBEXECDIR", "lib");

    let helper_path = std::env::var("SENTINEL_HELPER_PATH").unwrap_or_else(|_| {
        PathBuf::from(&prefix)
            .join(&libexecdir)
            .join("sentinel-helper")
            .display()
            .to_string()
    });

    // Semicolon-separated list. Probed at runtime in the order given.
    let helper1_candidates = std::env::var("POLKIT_AGENT_HELPER_CANDIDATES").unwrap_or_else(|_| {
        [
            "/usr/lib/polkit-1/polkit-agent-helper-1",
            "/usr/libexec/polkit-1/polkit-agent-helper-1",
            "/usr/lib/policykit-1/polkit-agent-helper-1",
            "/usr/libexec/polkit-agent-helper-1",
        ]
        .join(";")
    });

    println!("cargo:rustc-env=SENTINEL_HELPER_PATH={helper_path}");
    println!("cargo:rustc-env=POLKIT_AGENT_HELPER_CANDIDATES={helper1_candidates}");
}
