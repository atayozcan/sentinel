//! Eagerly-populated snapshot of a process's `/proc/<pid>/*` data.
//!
//! Just a typed bundle around `sentinel_shared::procfs::*` lookups
//! with the unknown / empty defaults the dialog renderer expects.
//! New /proc readers go in `sentinel_shared::procfs`, not here.

use sentinel_shared::procfs;

pub struct ProcessInfo {
    pub name: String,
    pub exe: String,
    pub cmdline: String,
    pub cwd: String,
}

impl ProcessInfo {
    pub fn for_pid(pid: i32) -> Self {
        Self {
            name: procfs::read_comm(pid).unwrap_or_else(|| "unknown".into()),
            exe: procfs::read_exe(pid).unwrap_or_else(|| "unknown".into()),
            cmdline: procfs::read_cmdline(pid).unwrap_or_default(),
            cwd: procfs::read_cwd(pid).unwrap_or_default(),
        }
    }
}
