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
    // lookup: theme cascade, fallback names, format selection. We
    // don't link libcanberra ourselves to keep the helper's C
    // dependency surface small.
    let _ = std::process::Command::new("canberra-gtk-play")
        .args(["-i", name])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}
