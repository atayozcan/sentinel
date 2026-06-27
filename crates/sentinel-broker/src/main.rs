// SPDX-FileCopyrightText: 2026 Atay Özcan <atay@oezcan.me>
// SPDX-License-Identifier: GPL-3.0-or-later
//! `sentinel-broker` — the privilege-separation remember-decision daemon.
//!
//! A long-lived, **unprivileged** daemon (run it as a dedicated user via
//! systemd `DynamicUser=`/`User=`) that owns the remember decision and an
//! in-memory grant store, behind a Unix socket. The root PAM shim relays
//! requests to it (see `sentinel-broker-proto`); the broker holds no root
//! privilege and writes nothing to disk, so a compromise of it yields far
//! less than the current in-`sudo`-process model.
//!
//! Trust: only **root** peers (the shim inside a privileged binary) are
//! served — enforced per-connection via `SO_PEERCRED`. The daemon is
//! sandboxed by its systemd unit (seccomp `@system-service`, no caps,
//! `ProtectSystem=strict`, `AF_UNIX`-only, …).
//!
//! This is increment 2 of the broker (daemon + in-memory store). The PAM
//! shim rewire that makes `pam_sentinel` relay here is increment 3.

#![forbid(unsafe_code)]

mod server;
mod store;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;

/// Default socket. Kept separate from the legacy `/run/sentinel/ts` store
/// so the two can coexist during the shim-rewire transition. systemd
/// provides the dir via `RuntimeDirectory=sentinel-broker` (0700).
///
/// Overridable with `SENTINEL_BROKER_SOCK` (matches the client's override)
/// so the broker can run for dev/test without root or systemd.
const DEFAULT_SOCK_PATH: &str = "/run/sentinel-broker/broker.sock";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sock_path =
        std::env::var("SENTINEL_BROKER_SOCK").unwrap_or_else(|_| DEFAULT_SOCK_PATH.to_string());
    let sock_dir = Path::new(&sock_path)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("/run/sentinel-broker"));

    // Defensive: systemd's RuntimeDirectory= normally creates this 0700,
    // owned by our user. Create it if we're run standalone.
    if !sock_dir.exists() {
        fs::create_dir_all(&sock_dir)?;
        fs::set_permissions(&sock_dir, fs::Permissions::from_mode(0o700))?;
    }
    // Clear a socket left by an unclean shutdown, then bind 0600.
    let _ = fs::remove_file(&sock_path);
    let listener = UnixListener::bind(&sock_path)?;
    fs::set_permissions(&sock_path, fs::Permissions::from_mode(0o600))?;
    eprintln!("sentinel-broker: listening on {sock_path}");

    let store = Arc::new(store::RememberStore::new());
    for conn in listener.incoming() {
        match conn {
            Ok(stream) => {
                let store = Arc::clone(&store);
                // One thread per connection, each bounded by an I/O
                // timeout (see `server::handle`), so a stuck client can't
                // wedge the accept loop.
                thread::spawn(move || server::handle(stream, &store, true));
            }
            Err(e) => eprintln!("sentinel-broker: accept failed: {e}"),
        }
    }
    Ok(())
}
