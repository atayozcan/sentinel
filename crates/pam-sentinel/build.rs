// SPDX-FileCopyrightText: 2025 Atay Özcan <atay@oezcan.me>
// SPDX-License-Identifier: GPL-3.0-or-later
// SENTINEL_CONFIG_PATH lives in `sentinel-shared`'s build.rs (shared
// with the agent). This crate only needs the helper path baked in.

fn main() {
    println!("cargo:rerun-if-env-changed=SENTINEL_PREFIX");
    println!("cargo:rerun-if-env-changed=SENTINEL_LIBEXECDIR");
    println!("cargo:rerun-if-env-changed=SENTINEL_HELPER_PATH");

    let prefix = std::env::var("SENTINEL_PREFIX").unwrap_or_else(|_| "/usr".into());
    let libexecdir = std::env::var("SENTINEL_LIBEXECDIR").unwrap_or_else(|_| "lib".into());

    // Honor an explicit SENTINEL_HELPER_PATH override (matching
    // sentinel-polkit-agent's build.rs) so a packager can point the PAM
    // module at a non-default helper — e.g. the Plasma helper at
    // /usr/bin/sentinel-helper-kde — without symlinking it into the
    // default location. Falls back to the computed default otherwise.
    let helper_path = std::env::var("SENTINEL_HELPER_PATH")
        .unwrap_or_else(|_| format!("{prefix}/{libexecdir}/sentinel-helper"));

    println!("cargo:rustc-env=SENTINEL_HELPER_PATH={helper_path}");
}
