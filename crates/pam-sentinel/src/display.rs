use std::fs;
use std::path::PathBuf;

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
        if !s.starts_with("wayland-") || s.contains(".lock") {
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
