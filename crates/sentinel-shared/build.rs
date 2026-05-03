// SPDX-FileCopyrightText: 2025 Atay Özcan <atay@oezcan.me>
// SPDX-License-Identifier: GPL-3.0-or-later
// Bake the absolute path of /etc/security/sentinel.conf at compile
// time so both `pam-sentinel` (running inside privileged binaries
// where env-based path resolution would be a security concern) and
// `sentinel-polkit-agent` (running as the user) reach the same file.

fn main() {
    let sysconfdir = std::env::var("SENTINEL_SYSCONFDIR").unwrap_or_else(|_| "/etc".into());
    let config_path = format!("{sysconfdir}/security/sentinel.conf");
    println!("cargo:rustc-env=SENTINEL_CONFIG_PATH={config_path}");
    println!("cargo:rerun-if-env-changed=SENTINEL_SYSCONFDIR");
}
