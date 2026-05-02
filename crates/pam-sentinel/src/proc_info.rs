use std::fs;
use std::path::PathBuf;

pub struct ProcessInfo {
    pub name: String,
    pub exe: String,
    pub cmdline: String,
    pub cwd: String,
}

impl ProcessInfo {
    pub fn for_pid(pid: i32) -> Self {
        Self {
            name: read_comm(pid).unwrap_or_else(|| "unknown".into()),
            exe: read_exe(pid).unwrap_or_else(|| "unknown".into()),
            cmdline: read_cmdline(pid).unwrap_or_default(),
            cwd: read_cwd(pid).unwrap_or_default(),
        }
    }
}

fn read_comm(pid: i32) -> Option<String> {
    let s = fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
    Some(s.trim().to_owned())
}

fn read_exe(pid: i32) -> Option<String> {
    let p = PathBuf::from(format!("/proc/{pid}/exe"));
    fs::read_link(p).ok()?.into_os_string().into_string().ok()
}

fn read_cwd(pid: i32) -> Option<String> {
    let p = PathBuf::from(format!("/proc/{pid}/cwd"));
    fs::read_link(p).ok()?.into_os_string().into_string().ok()
}

fn read_cmdline(pid: i32) -> Option<String> {
    let bytes = fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    let parts: Vec<String> = bytes
        .split(|&b| b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}
