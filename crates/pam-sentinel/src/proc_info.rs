use std::fs;
use std::path::PathBuf;

pub struct ProcessInfo {
    pub name: String,
    pub exe: String,
}

impl ProcessInfo {
    pub fn for_pid(pid: i32) -> Self {
        Self {
            name: read_comm(pid).unwrap_or_else(|| "unknown".into()),
            exe: read_exe(pid).unwrap_or_else(|| "unknown".into()),
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
