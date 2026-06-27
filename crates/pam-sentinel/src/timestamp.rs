// SPDX-FileCopyrightText: 2026 Atay Özcan <atay@oezcan.me>
// SPDX-License-Identifier: GPL-3.0-or-later
//! Root-owned "remember" timestamp store — a `sudo`-ts analogue backing
//! the optional auto-allow window (`[general].remember_seconds`).
//!
//! # Security model
//!
//! Records live on tmpfs at `/run/sentinel/ts`, a **root-owned `0700`**
//! directory this module creates and re-validates. Since `pam_sentinel`
//! runs as root inside the privileged binary, only root can create,
//! read, list, or forge a record — a non-root process is locked out by
//! directory permissions.
//!
//! Each record is **bound** to:
//! - the human user's `loginuid`, and
//! - the kernel audit `sessionid`,
//!
//! so a grant cannot be replayed in another login session or by another
//! user; and it is **scoped** to the `(service, command)` it was granted
//! for — the *full* elevated command, not just the program name, so a
//! grant for `sudo pacman -Syu` can never auto-allow
//! `sudo pacman -U /tmp/evil`. A process with no audit session
//! (`loginuid`/`sessionid == (u32)-1`) is treated as un-rememberable.
//!
//! Freshness uses `CLOCK_BOOTTIME` (monotonic since boot) stored *in* the
//! record — not the wall clock or file mtime — so the window cannot be
//! extended by moving the system clock backwards. tmpfs is wiped on
//! reboot, so no record survives a reboot.
//!
//! Every operation is fail-closed: any doubt (wrong owner/mode, parse
//! error, missing clock) yields "not fresh" / "don't record".

use nix::time::{ClockId, clock_gettime};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

const TS_DIR: &str = "/run/sentinel/ts";

/// Hard ceiling on the remember window regardless of config — bounds the
/// blast radius of an over-generous `remember_seconds`.
pub const MAX_REMEMBER_SECS: u64 = 900;

/// Identity a record is bound to.
pub struct Binding<'a> {
    pub loginuid: u32,
    pub sessionid: u32,
    pub service: &'a str,
    /// The **full** command this grant authorizes (e.g. `pacman -Syu`),
    /// not just the program name. Comes from
    /// [`crate::proc_info::ProcessInfo::remember_command`], which is
    /// `None` for un-rememberable requests (bare-elevation root shells,
    /// arbitrary-code gateways) — so a present, non-empty `command` is
    /// itself an eligibility signal. Binding to the whole command is what
    /// stops a grant for one invocation authorizing a different one.
    pub command: &'a str,
}

impl Binding<'_> {
    /// A record can only be created/trusted when the request is tied to a
    /// real login session and a concrete command.
    fn is_bindable(&self) -> bool {
        self.loginuid != u32::MAX && self.sessionid != u32::MAX && !self.command.is_empty()
    }

    fn key(&self) -> String {
        format!(
            "{}:{}:{}:{}",
            self.loginuid, self.sessionid, self.service, self.command
        )
    }
}

fn boottime_secs() -> Option<u64> {
    clock_gettime(ClockId::CLOCK_BOOTTIME)
        .ok()
        .map(|t| t.tv_sec() as u64)
}

fn record_path(dir: &Path, key: &str) -> PathBuf {
    // Filename = 16 hex chars of a stable hash of the full key. The full
    // key is stored in the file and re-verified on read, so a hash
    // collision can never promote the wrong record to a match.
    let mut h = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut h);
    dir.join(format!("{:016x}", h.finish()))
}

/// True iff a fresh, validly-owned record exists for `b` within
/// `ttl_secs` (capped at [`MAX_REMEMBER_SECS`]).
pub fn is_fresh(b: &Binding, ttl_secs: u64) -> bool {
    is_fresh_in(Path::new(TS_DIR), b, ttl_secs, true)
}

/// Create or refresh the record for `b`. Best-effort; failures are
/// logged and swallowed (a missing record just means "ask again").
pub fn record(b: &Binding) {
    record_in(Path::new(TS_DIR), b, true);
}

// ---- inner, dependency-injected forms (the `strict` flag is false only
// ---- in unit tests, where the temp dir is owned by the test user) -----

fn is_fresh_in(dir: &Path, b: &Binding, ttl_secs: u64, strict: bool) -> bool {
    if ttl_secs == 0 || !b.is_bindable() {
        return false;
    }
    let ttl = ttl_secs.min(MAX_REMEMBER_SECS);
    let key = b.key();
    let path = record_path(dir, &key);

    let Ok(mut f) = fs::File::open(&path) else {
        return false;
    };
    let Ok(meta) = f.metadata() else {
        return false;
    };
    // Defence in depth: trust only a regular, root-owned, group/other-zero
    // file (the dir perms already exclude non-root, but verify anyway).
    if !meta.is_file() || (strict && (meta.uid() != 0 || meta.mode() & 0o077 != 0)) {
        return false;
    }
    let mut buf = String::new();
    if f.read_to_string(&mut buf).is_err() {
        return false;
    }
    let mut lines = buf.lines();
    let stored_key = lines.next().unwrap_or_default();
    let stored_ts = lines.next().and_then(|s| s.parse::<u64>().ok());
    if stored_key != key {
        return false; // hash collision or tampering
    }
    let (Some(ts), Some(now)) = (stored_ts, boottime_secs()) else {
        return false;
    };
    // `now >= ts` rejects a record stamped in the future (clock skew /
    // tamper); the window is [ts, ts + ttl).
    now >= ts && now - ts < ttl
}

fn record_in(dir: &Path, b: &Binding, strict: bool) {
    if !b.is_bindable() {
        return;
    }
    if let Err(e) = ensure_dir(dir, strict) {
        log::warn!("sentinel: remember store unavailable: {e}");
        return;
    }
    let Some(now) = boottime_secs() else {
        return;
    };
    let key = b.key();
    let path = record_path(dir, &key);
    let tmp = path.with_extension("tmp");
    let content = format!("{key}\n{now}\n");

    let write = || -> std::io::Result<()> {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(content.as_bytes())?;
        f.sync_all()?;
        fs::rename(&tmp, &path) // atomic replace
    };
    if let Err(e) = write() {
        let _ = fs::remove_file(&tmp);
        log::warn!("sentinel: failed to write remember record: {e}");
    }
}

/// Ensure `dir` and its parent exist as root-owned `0700` directories.
/// Refuses (Err) if a component exists but is NOT a root-owned `0700`
/// directory — catching a pre-planted symlink or a user-owned dir. Uses
/// `symlink_metadata` so a symlink is rejected rather than followed.
fn ensure_dir(dir: &Path, strict: bool) -> std::io::Result<()> {
    let mut chain = Vec::new();
    if let Some(parent) = dir.parent() {
        chain.push(parent);
    }
    chain.push(dir);
    for p in chain {
        match fs::symlink_metadata(p) {
            Ok(meta) => {
                let bad =
                    !meta.is_dir() || (strict && (meta.uid() != 0 || meta.mode() & 0o077 != 0));
                if bad {
                    return Err(std::io::Error::other(format!(
                        "{} is not a root-owned 0700 directory",
                        p.display()
                    )));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(p)?;
                fs::set_permissions(p, fs::Permissions::from_mode(0o700))?;
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b<'a>(service: &'a str, command: &'a str) -> Binding<'a> {
        Binding {
            loginuid: 1000,
            sessionid: 3,
            service,
            command,
        }
    }

    #[test]
    fn record_then_fresh_roundtrip() {
        let dir = std::env::temp_dir().join(format!("sentinel-ts-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let sub = dir.join("ts");
        let bind = b("sudo", "/usr/bin/pacman");

        assert!(!is_fresh_in(&sub, &bind, 60, false), "no record yet");
        record_in(&sub, &bind, false);
        assert!(is_fresh_in(&sub, &bind, 60, false), "fresh after record");
        // ttl=0 always misses
        assert!(!is_fresh_in(&sub, &bind, 0, false));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn binding_must_differ_per_session_user_and_target() {
        let dir = std::env::temp_dir().join(format!("sentinel-ts-test2-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let sub = dir.join("ts");
        record_in(&sub, &b("sudo", "pacman -Syu"), false);

        // different service / command / session / user must NOT match
        assert!(is_fresh_in(&sub, &b("sudo", "pacman -Syu"), 60, false));
        assert!(!is_fresh_in(&sub, &b("su", "pacman -Syu"), 60, false));
        // SAME program, DIFFERENT arguments must NOT match — this is the
        // argv-binding guarantee (a grant for `pacman -Syu` cannot
        // authorize `pacman -U /tmp/evil`).
        assert!(!is_fresh_in(
            &sub,
            &b("sudo", "pacman -U /tmp/evil"),
            60,
            false
        ));
        assert!(!is_fresh_in(&sub, &b("sudo", "topgrade"), 60, false));
        let mut other_session = b("sudo", "pacman -Syu");
        other_session.sessionid = 99;
        assert!(!is_fresh_in(&sub, &other_session, 60, false));
        let mut other_user = b("sudo", "pacman -Syu");
        other_user.loginuid = 1001;
        assert!(!is_fresh_in(&sub, &other_user, 60, false));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn unbindable_never_remembers() {
        let dir = std::env::temp_dir().join(format!("sentinel-ts-test3-{}", std::process::id()));
        let sub = dir.join("ts");
        let no_session = Binding {
            loginuid: u32::MAX,
            sessionid: u32::MAX,
            service: "sudo",
            command: "pacman -Syu",
        };
        record_in(&sub, &no_session, false); // no-op
        assert!(!is_fresh_in(&sub, &no_session, 60, false));
        assert!(!sub.exists(), "nothing written for an unbindable request");
    }

    #[test]
    fn corrupt_record_is_not_fresh() {
        let dir = std::env::temp_dir().join(format!("sentinel-ts-test4-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let sub = dir.join("ts");
        fs::create_dir_all(&sub).unwrap();
        let bind = b("sudo", "/usr/bin/pacman");
        // wrong key in the file body → reject (simulates a hash collision)
        let path = record_path(&sub, &bind.key());
        fs::write(&path, "someone:else:su:/bin/x\n123\n").unwrap();
        assert!(!is_fresh_in(&sub, &bind, 600, false));
        let _ = fs::remove_dir_all(&dir);
    }
}
