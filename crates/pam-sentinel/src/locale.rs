//! Locale variable propagation from the requesting user's process to
//! the spawned `sentinel-helper`.
//!
//! When `pam_sentinel.so` runs inside a privileged binary (sudo,
//! polkit-agent-helper-1, su), the process environment has been
//! scrubbed of `LANG` and `LC_*` — that's why the dialog would
//! otherwise always appear in English regardless of the user's
//! desktop locale. We recover them by reading
//! `/proc/<requesting_pid>/environ` (the user's original env, NUL-
//! separated) and forwarding the locale-relevant ones into the
//! post-fork child env, just before exec.
//!
//! ## Threat model
//!
//! `/proc/<pid>/environ` is **user-controlled** — an attacker can set
//! `LANG=anything` in their shell and we'll read it. Mitigations:
//!
//! 1. Only the locale-relevant variable names are forwarded. We don't
//!    inherit the whole env.
//! 2. Each value is validated against a strict whitelist: at most 32
//!    chars from `[A-Za-z0-9._@-]`. POSIX locale strings fit this
//!    easily; anything sketchy (NUL bytes, path separators, shell
//!    metacharacters) is rejected.
//! 3. The helper itself does *another* round of canonicalization +
//!    negotiation against its embedded bundle list — so even a value
//!    that passes our filter has limited blast radius.
//!
//! ## Why not just set them globally?
//!
//! `std::env::set_var` from PAM-call context is fine post-fork (we're
//! single-threaded by then), but the values must be set BEFORE the
//! child execs the helper so the helper sees them as inherited env.

use std::collections::HashMap;

/// Variable names we forward into the helper child. Order doesn't
/// matter for correctness — fluent-langneg picks the first valid
/// entry.
pub const FORWARDED_VARS: &[&str] = &["LC_ALL", "LC_MESSAGES", "LANG"];

/// Read NUL-separated `KEY=VALUE` pairs from `/proc/<pid>/environ`
/// and return the subset of [`FORWARDED_VARS`] whose values pass
/// [`is_safe_locale_value`].
///
/// Returns an empty map on any error (unreadable, missing, permission
/// denied) — locale propagation is best-effort and must never block
/// auth.
pub fn read_locale_env(pid: i32) -> HashMap<&'static str, String> {
    let mut out = HashMap::new();
    if pid <= 0 {
        return out;
    }
    let bytes = match std::fs::read(format!("/proc/{pid}/environ")) {
        Ok(b) => b,
        Err(_) => return out,
    };
    for entry in bytes.split(|b| *b == 0) {
        if entry.is_empty() {
            continue;
        }
        let Ok(s) = std::str::from_utf8(entry) else {
            continue;
        };
        let Some((key, value)) = s.split_once('=') else {
            continue;
        };
        if let Some(canonical_key) = FORWARDED_VARS.iter().find(|k| **k == key) {
            if is_safe_locale_value(value) {
                // Last-write-wins if the entry appears multiple times,
                // which mirrors what execve does.
                out.insert(*canonical_key, value.to_string());
            }
        }
    }
    out
}

/// Strict whitelist for locale strings. POSIX locales look like
/// `tr_TR.UTF-8@modifier` — letters, digits, dot, underscore, at,
/// hyphen. Anything else (NUL bytes, slashes, shell metas, control
/// chars) is rejected.
pub fn is_safe_locale_value(value: &str) -> bool {
    if value.is_empty() || value.len() > 32 {
        return false;
    }
    value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '@' | '-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_normal_posix_locales() {
        assert!(is_safe_locale_value("en_US.UTF-8"));
        assert!(is_safe_locale_value("tr_TR.UTF-8"));
        assert!(is_safe_locale_value("de_DE@euro"));
        assert!(is_safe_locale_value("sr_RS.UTF-8@latin"));
        assert!(is_safe_locale_value("C"));
        assert!(is_safe_locale_value("POSIX"));
        assert!(is_safe_locale_value("en"));
    }

    #[test]
    fn rejects_empty() {
        assert!(!is_safe_locale_value(""));
    }

    #[test]
    fn rejects_overlong() {
        assert!(!is_safe_locale_value(&"a".repeat(33)));
    }

    #[test]
    fn rejects_nul_bytes() {
        assert!(!is_safe_locale_value("en\0US"));
    }

    #[test]
    fn rejects_path_traversal() {
        assert!(!is_safe_locale_value("../etc/shadow"));
        assert!(!is_safe_locale_value("en/../oops"));
    }

    #[test]
    fn rejects_shell_metas() {
        assert!(!is_safe_locale_value("en$(whoami)"));
        assert!(!is_safe_locale_value("en;rm"));
        assert!(!is_safe_locale_value("en|cat"));
        assert!(!is_safe_locale_value("en US")); // space
    }

    #[test]
    fn rejects_control_chars() {
        assert!(!is_safe_locale_value("en\nUS"));
        assert!(!is_safe_locale_value("en\tUS"));
    }

    #[test]
    fn read_locale_env_handles_missing_pid() {
        // pid -1 / 0 short-circuits before any /proc lookup.
        assert!(read_locale_env(-1).is_empty());
        assert!(read_locale_env(0).is_empty());
    }

    #[test]
    fn read_locale_env_for_self_returns_at_least_lang_when_set() {
        // /proc/<pid>/environ for our own process is always readable.
        // Whether LANG is set depends on the test runner's env, so this
        // test only asserts the function doesn't crash and that any
        // returned key is a known forwardable name.
        let env = read_locale_env(std::process::id() as i32);
        for key in env.keys() {
            assert!(FORWARDED_VARS.contains(key), "unexpected key {key}");
        }
    }
}
