// SPDX-FileCopyrightText: 2025 Atay Özcan <atay@oezcan.me>
// SPDX-License-Identifier: GPL-3.0-or-later
//! Eagerly-populated snapshot of a process's `/proc/<pid>/*` data.
//!
//! Just a typed bundle around `sentinel_shared::procfs::*` lookups
//! with the unknown / empty defaults the dialog renderer expects.
//! New /proc readers go in `sentinel_shared::procfs`, not here.

use sentinel_shared::{procfs, strip_elevation_prefix};

pub struct ProcessInfo {
    pub name: String,
    pub exe: String,
    pub cmdline: String,
    pub cwd: String,
}

impl ProcessInfo {
    pub fn for_pid(pid: i32) -> Self {
        let raw_exe = procfs::read_exe(pid).unwrap_or_else(|| "unknown".into());
        let raw_cmdline = procfs::read_cmdline(pid).unwrap_or_default();

        // Resolve what to display. Three paths, in order:
        //
        // 1. The cmdline is `sudo X args…` (or pkexec/su/doas). Strip
        //    the elevation prefix and use the elevated program — the
        //    dialog says "pacman", not "sudo-rs".
        //
        // 2. The cmdline is just `sudo` with flags but NO target
        //    (e.g. `sudo -v` for credential caching, common in
        //    `topgrade` and `paru`). `strip_elevation_prefix` returns
        //    an empty string. Walk up to `PPid` and use the parent's
        //    exe/cmdline — the dialog shows the user-facing
        //    originator (`paru`, `topgrade`, the user's shell) rather
        //    than just `sudo-rs` which is uninformative.
        //
        // 3. Not an elevation tool at all (the PAM module loaded into
        //    something else). Use the binary's own /proc info as-is.
        let stripped = strip_elevation_prefix(&raw_cmdline);
        let was_elevation = !raw_cmdline.is_empty() && stripped != raw_cmdline;
        let (exe, cmdline) = if was_elevation && !stripped.is_empty() {
            // Path 1: elevation wrapper with a target.
            let target_exe = stripped
                .split_whitespace()
                .next()
                .unwrap_or("unknown")
                .to_string();
            (target_exe, stripped)
        } else if was_elevation {
            // Path 2: elevation wrapper with NO target — walk up.
            let parent = procfs::read_ppid(pid).and_then(|ppid| {
                let pexe = procfs::read_exe(ppid)?;
                let pcmdline = procfs::read_cmdline(ppid).unwrap_or_default();
                Some((pexe, pcmdline))
            });
            match parent {
                Some((pexe, pcmdline)) => (pexe, pcmdline),
                None => (raw_exe, raw_cmdline),
            }
        } else {
            // Path 3: not an elevation tool.
            (raw_exe, raw_cmdline)
        };

        Self {
            name: sentinel_shared::process_basename(&exe)
                .map(str::to_owned)
                .or_else(|| procfs::read_comm(pid))
                .unwrap_or_else(|| "unknown".into()),
            exe,
            cmdline,
            cwd: procfs::read_cwd(pid).unwrap_or_default(),
        }
    }
}
