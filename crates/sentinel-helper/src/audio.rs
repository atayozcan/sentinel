// SPDX-FileCopyrightText: 2025 Atay Özcan <atay@oezcan.me>
// SPDX-License-Identifier: GPL-3.0-or-later
//! UAC-style audio cue when the dialog appears.
//!
//! Fires a freedesktop-named sound through `canberra-gtk-play`
//! (part of `libcanberra`, installed by default on most Linux
//! desktops). No Rust audio dependency, no PulseAudio/PipeWire
//! linkage from the helper itself.
//!
//! Failure to spawn the player is intentionally silent — an
//! installation without `canberra-gtk-play` should not prevent the
//! dialog from rendering.

/// Spawn the sound player asynchronously. Returns immediately; the
/// player process is detached. Empty `name` short-circuits as a
/// no-op so the caller doesn't need to gate the call.
pub fn play_named(name: &str) {
    if name.is_empty() {
        return;
    }
    // canberra-gtk-play handles the freedesktop sound naming spec
    // lookup: theme cascade, parent-theme fallback, format selection.
    // We pass just the configured name and let libcanberra walk the
    // theme cascade — passing multiple `-i` flags would NOT chain
    // (GLib's option parser keeps the last value), it'd override.
    let child = std::process::Command::new("canberra-gtk-play")
        .args(["-i", name])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    // Reap the child in a fire-and-forget thread so it doesn't linger
    // as a zombie for the helper's lifetime. The helper itself is
    // short-lived (one auth attempt) so this is purely cosmetic, but
    // it keeps `ps` clean during long-running test sessions.
    if let Ok(mut child) = child {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}
