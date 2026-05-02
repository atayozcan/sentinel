use crate::proc_info::ProcessInfo;
use nix::errno::Errno;
use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
use nix::sys::signal::{Signal, kill};
use nix::sys::wait::waitpid;
use nix::unistd::{
    ForkResult, Pid, User, dup2_stdout, execv, fork, initgroups, pipe, setgid, setuid,
};
use sentinel_config::ServiceConfig;
use std::ffi::CString;
use std::os::fd::{AsFd, OwnedFd};

pub const HELPER_PATH: &str = env!("SENTINEL_HELPER_PATH");

#[derive(Debug, Clone, Copy)]
pub enum HelperResult {
    Allow,
    Deny,
}

pub struct HelperRequest<'a> {
    pub cfg: &'a ServiceConfig,
    pub user: &'a str,
    pub service: &'a str,
    pub process: &'a ProcessInfo,
    pub formatted_message: &'a str,
    pub formatted_secondary: &'a str,
    pub target_uid: u32,
    pub requesting_pid: i32,
}

pub fn run(req: &HelperRequest<'_>) -> Result<HelperResult, String> {
    let (read_fd, write_fd) = pipe().map_err(|e| format!("pipe: {e}"))?;

    // SAFETY: fork in a PAM module called from a process not yet using threads
    // for this auth attempt. The child uses only async-signal-safe operations.
    match unsafe { fork() }.map_err(|e| format!("fork: {e}"))? {
        ForkResult::Child => {
            drop(read_fd);
            child_exec(req, write_fd);
        }
        ForkResult::Parent { child } => {
            drop(write_fd);
            parent_wait(child, read_fd, req)
        }
    }
}

fn child_exec(req: &HelperRequest<'_>, write_fd: OwnedFd) -> ! {
    let user = match User::from_uid(nix::unistd::Uid::from_raw(req.target_uid)) {
        Ok(Some(u)) => u,
        _ => std::process::exit(1),
    };

    if initgroups(
        &CString::new(user.name.clone()).unwrap_or_else(|_| CString::new("").unwrap()),
        user.gid,
    )
    .is_err()
    {
        std::process::exit(1);
    }
    if setgid(user.gid).is_err() {
        std::process::exit(1);
    }
    if setuid(user.uid).is_err() {
        std::process::exit(1);
    }

    // SAFETY: setting env in the post-fork child before exec; no other threads.
    unsafe {
        std::env::set_var("HOME", &user.dir);
        std::env::set_var("USER", &user.name);
        std::env::set_var("LOGNAME", &user.name);

        // Forward locale-relevant env vars from the requesting user's
        // own process so the helper picks the right translation. This
        // env was scrubbed by sudo / polkit-agent-helper-1, so we have
        // to recover it from /proc/<requesting_pid>/environ. Values
        // are validated against a strict whitelist before use — see
        // `crate::locale` for the threat model.
        for (key, value) in crate::locale::read_locale_env(req.requesting_pid) {
            std::env::set_var(key, value);
        }
    }

    if dup2_stdout(write_fd.as_fd()).is_err() {
        std::process::exit(1);
    }
    drop(write_fd);

    let mut argv: Vec<CString> = Vec::with_capacity(20);
    let push = |argv: &mut Vec<CString>, s: &str| {
        if let Ok(c) = CString::new(s) {
            argv.push(c);
        }
    };
    push(&mut argv, HELPER_PATH);
    push(&mut argv, "--title");
    push(&mut argv, &req.cfg.title);
    push(&mut argv, "--message");
    push(&mut argv, req.formatted_message);
    push(&mut argv, "--secondary");
    push(&mut argv, req.formatted_secondary);
    push(&mut argv, "--timeout");
    push(&mut argv, &req.cfg.timeout.to_string());
    push(&mut argv, "--min-time");
    push(&mut argv, &req.cfg.min_display_time_ms.to_string());
    if req.cfg.randomize_buttons {
        push(&mut argv, "--randomize");
    }
    if req.cfg.show_process_info {
        push(&mut argv, "--process-exe");
        push(&mut argv, &req.process.exe);
        if !req.process.cmdline.is_empty() {
            push(&mut argv, "--process-cmdline");
            push(&mut argv, &req.process.cmdline);
        }
        push(&mut argv, "--process-pid");
        push(&mut argv, &req.requesting_pid.to_string());
        if !req.process.cwd.is_empty() {
            push(&mut argv, "--process-cwd");
            push(&mut argv, &req.process.cwd);
        }
        push(&mut argv, "--requesting-user");
        push(&mut argv, req.user);
        push(&mut argv, "--action");
        push(&mut argv, req.service);
    }

    let prog = match CString::new(HELPER_PATH) {
        Ok(c) => c,
        Err(_) => std::process::exit(1),
    };
    let _ = execv(&prog, &argv);
    // exec failed; signal DENY via the dup'd stdout pipe.
    let _ = nix::unistd::write(std::io::stdout(), b"DENY\n");
    std::process::exit(1);
}

fn read_pipe(fd: &OwnedFd, buf: &mut [u8]) -> nix::Result<usize> {
    nix::unistd::read(fd, buf)
}

fn parent_wait(
    child: Pid,
    read_fd: OwnedFd,
    req: &HelperRequest<'_>,
) -> Result<HelperResult, String> {
    // Helper has its own auto-deny; give it a small grace period.
    let timeout_ms = (i32::try_from(req.cfg.timeout).unwrap_or(30) + 5) * 1000;
    let mut fds = [PollFd::new(read_fd.as_fd(), PollFlags::POLLIN)];

    let timeout = PollTimeout::try_from(timeout_ms).unwrap_or(PollTimeout::MAX);
    let n = match poll(&mut fds, timeout) {
        Ok(n) => n,
        Err(e) => {
            let _ = kill(child, Signal::SIGKILL);
            let _ = waitpid(child, None);
            return Err(format!("poll: {e}"));
        }
    };

    if n == 0 {
        let _ = kill(child, Signal::SIGKILL);
        let _ = waitpid(child, None);
        return Err("helper timeout".into());
    }

    let mut buf = [0u8; 32];
    let read_n = match read_pipe(&read_fd, &mut buf) {
        Ok(n) => n,
        Err(Errno::EINTR) => 0,
        Err(e) => {
            let _ = waitpid(child, None);
            return Err(format!("read: {e}"));
        }
    };

    let _ = waitpid(child, None);

    if read_n == 0 {
        return Err("helper produced no output".into());
    }

    let s = std::str::from_utf8(&buf[..read_n])
        .unwrap_or("")
        .split(['\n', '\r'])
        .next()
        .unwrap_or("");

    match s {
        "ALLOW" => Ok(HelperResult::Allow),
        _ => Ok(HelperResult::Deny),
    }
}
