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

        // If we're inside an elevation tool (sudo, sudo-rs, su, doas)
        // — which is the common case for the PAM module's sudo path
        // — the `cmdline` is something like `sudo-rs true` and the
        // user wants to see "true" in the dialog, not "sudo-rs".
        // `strip_elevation_prefix` recognises the elevation tools by
        // basename and strips them + their flags, leaving the
        // elevated argv. When that yields a non-empty result, derive
        // exe + cmdline from it.
        let stripped = strip_elevation_prefix(&raw_cmdline);
        let (exe, cmdline) = if !stripped.is_empty() && stripped != raw_cmdline {
            // First whitespace-separated token is the elevated
            // program path (often relative, e.g. "true"; the dialog
            // basename's it for display either way).
            let target_exe = stripped
                .split_whitespace()
                .next()
                .unwrap_or("unknown")
                .to_string();
            (target_exe, stripped)
        } else {
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
