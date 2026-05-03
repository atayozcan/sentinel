use std::fs;
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};

/// Locate the user's Wayland display, populating WAYLAND_DISPLAY and
/// XDG_RUNTIME_DIR if not already set. Returns true on success.
pub fn detect_for_user(uid: u32) -> bool {
    if let Ok(v) = std::env::var("WAYLAND_DISPLAY") {
        if !v.is_empty() {
            return true;
        }
    }

    let runtime_dir = PathBuf::from(format!("/run/user/{uid}"));
    let Ok(entries) = fs::read_dir(&runtime_dir) else {
        return false;
    };

    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(s) = name.to_str() else { continue };
        if !s.starts_with("wayland-") {
            continue;
        }
        // Skip `wayland-N.lock` lockfiles. Using `Path::extension()`
        // rather than `s.contains(".lock")` so a hypothetical socket
        // named `wayland-locked` doesn't get rejected by accident.
        if Path::new(s).extension().and_then(|e| e.to_str()) == Some("lock") {
            continue;
        }
        // Verify it's actually a Unix socket — the lock file isn't,
        // and a stale regular file with a wayland-* name shouldn't
        // win here either.
        if !entry
            .file_type()
            .ok()
            .map(|t| t.is_socket())
            .unwrap_or(false)
        {
            continue;
        }
        // SAFETY: this PAM module runs single-threaded inside the auth call.
        unsafe {
            std::env::set_var("WAYLAND_DISPLAY", s);
            if std::env::var_os("XDG_RUNTIME_DIR").is_none() {
                std::env::set_var("XDG_RUNTIME_DIR", &runtime_dir);
            }
        }
        return true;
    }
    false
}
