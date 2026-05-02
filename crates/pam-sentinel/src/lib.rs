//! `pam_sentinel.so` — the PAM module half of Sentinel.
//!
//! Loaded by libpam on every authentication attempt for whatever
//! services have it wired in (`/etc/pam.d/polkit-1`, `/etc/pam.d/sudo`,
//! …). For each call we either:
//!
//! * **bypass**: the Sentinel polkit agent already pre-approved this
//!   auth (we connect to its Unix socket and read "OK"). Return
//!   `PAM_SUCCESS` immediately. See [`agent_bypass`].
//! * **dialog**: spawn `sentinel-helper` to render the confirmation UI;
//!   return `PAM_SUCCESS` on Allow, `PAM_AUTH_ERR` on Deny / timeout.
//! * **headless**: no Wayland display; return whatever
//!   `headless_action` says (default `PAM_IGNORE` so the next module
//!   can prompt for a password).
//! * **disabled**: `enabled = false` in config; `PAM_IGNORE`.

mod agent_bypass;
mod display;
mod helper;
mod locale;
mod proc_info;

use helper::{HelperRequest, run as run_helper};
use pam::constants::{PamFlag, PamResultCode};
use pam::module::{PamHandle, PamHooks};
use proc_info::ProcessInfo;
use sentinel_config::log_kv::quote as q;
use sentinel_config::{HeadlessAction, Outcome, ServiceConfig, format_message, load};
use std::ffi::CStr;
use std::time::Instant;
use syslog::{BasicLogger, Facility, Formatter3164};

const MODULE_NAME: &str = "pam_sentinel";

struct PamSentinel;
pam::pam_hooks!(PamSentinel);

impl PamHooks for PamSentinel {
    fn sm_authenticate(pamh: &mut PamHandle, _args: Vec<&CStr>, _flags: PamFlag) -> PamResultCode {
        init_logger();

        if let Some(rc) = agent_bypass::check_agent_bypass(pamh) {
            return rc;
        }

        let service = pam_service(pamh);
        let cfg = load(&service);
        if !cfg.enabled {
            log::debug!("{MODULE_NAME}: disabled for service {service}");
            return PamResultCode::PAM_IGNORE;
        }

        // The PAM module is dlopen'd inside the privileged binary
        // (`sudo`, `polkit-agent-helper-1`, `su`). `getpid()` therefore
        // yields *that* process — which is what we want to display:
        // `/proc/<sudo-pid>/cmdline` is the full command sudo is about
        // to run, while `getppid()` would point at sudo's parent shell.
        // For loginuid lookup we still walk via the parent because the
        // loginuid is inherited from login, not set on the privileged
        // binary itself.
        let process_pid = unsafe { libc_getpid() };
        let requesting_uid = caller_uid(unsafe { libc_getppid() });
        let user = resolve_user(pamh, requesting_uid);

        if !display::detect_for_user(requesting_uid) {
            return handle_headless(&cfg, &service, &user);
        }

        let process = ProcessInfo::for_pid(process_pid);
        spawn_dialog(&cfg, &service, &user, &process, process_pid, requesting_uid)
    }

    fn sm_setcred(_pamh: &mut PamHandle, _args: Vec<&CStr>, _flags: PamFlag) -> PamResultCode {
        // We're an auth-only module — we don't issue or revoke
        // credentials. Returning PAM_SUCCESS would be a lie that says
        // "yes I established/destroyed credentials"; PAM_IGNORE tells
        // the stack to skip us, which is correct.
        PamResultCode::PAM_IGNORE
    }
}

// -------------- per-stage helpers ------------------------------------------

fn pam_service(pamh: &PamHandle) -> String {
    pamh.get_item::<pam::items::Service>()
        .ok()
        .flatten()
        .and_then(|s| s.to_str().ok().map(str::to_owned))
        .unwrap_or_else(|| "unknown".into())
}

/// Resolve the requesting user's name. Prefers the uid we derived from
/// `/proc/<ppid>/loginuid`; falls back to whatever PAM has if that uid
/// has no passwd entry.
fn resolve_user(pamh: &PamHandle, uid: u32) -> String {
    if let Ok(Some(u)) = nix::unistd::User::from_uid(nix::unistd::Uid::from_raw(uid)) {
        return u.name;
    }
    pamh.get_user(None).ok().unwrap_or_else(|| "unknown".into())
}

fn handle_headless(cfg: &ServiceConfig, service: &str, user: &str) -> PamResultCode {
    match cfg.headless_action {
        HeadlessAction::Allow => {
            if cfg.log_attempts {
                log::warn!(
                    "event=auth.allow source=headless user={} service={}",
                    q(user),
                    q(service)
                );
            }
            PamResultCode::PAM_SUCCESS
        }
        HeadlessAction::Deny => {
            if cfg.log_attempts {
                log::info!(
                    "event=auth.deny source=headless user={} service={}",
                    q(user),
                    q(service)
                );
            }
            PamResultCode::PAM_AUTH_ERR
        }
        HeadlessAction::Password => {
            log::debug!(
                "{MODULE_NAME}: no display, falling through to password (service {service})"
            );
            PamResultCode::PAM_IGNORE
        }
    }
}

fn spawn_dialog(
    cfg: &ServiceConfig,
    service: &str,
    user: &str,
    process: &ProcessInfo,
    requesting_pid: i32,
    requesting_uid: u32,
) -> PamResultCode {
    let formatted_title = format_message(&cfg.title, user, service, &process.name);
    let formatted_message = format_message(&cfg.message, user, service, &process.name);
    let formatted_secondary = format_message(&cfg.secondary, user, service, &process.name);

    let req = HelperRequest {
        cfg,
        user,
        service,
        process,
        formatted_title: &formatted_title,
        formatted_message: &formatted_message,
        formatted_secondary: &formatted_secondary,
        target_uid: requesting_uid,
        requesting_pid,
    };

    let dialog_started = Instant::now();
    let result = run_helper(&req);
    let latency_ms = dialog_started.elapsed().as_millis();

    if cfg.log_attempts {
        match &result {
            Ok(Outcome::Allow) => log::info!(
                "event=auth.allow source=dialog user={} service={} process={} uid={} latency_ms={}",
                q(user),
                q(service),
                q(&process.name),
                requesting_uid,
                latency_ms
            ),
            Ok(Outcome::Deny) => log::info!(
                "event=auth.deny source=dialog user={} service={} process={} uid={} latency_ms={}",
                q(user),
                q(service),
                q(&process.name),
                requesting_uid,
                latency_ms
            ),
            Ok(Outcome::Timeout) => log::info!(
                "event=auth.timeout source=dialog user={} service={} process={} uid={} latency_ms={}",
                q(user),
                q(service),
                q(&process.name),
                requesting_uid,
                latency_ms
            ),
            Err(e) => log::warn!(
                "event=auth.error source=dialog user={} service={} error={} latency_ms={}",
                q(user),
                q(service),
                q(&e.to_string()),
                latency_ms
            ),
        }
    }

    match result {
        Ok(o) if o.is_allow() => PamResultCode::PAM_SUCCESS,
        _ => PamResultCode::PAM_AUTH_ERR,
    }
}

// -------------- module init -----------------------------------------------

fn init_logger() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let formatter = Formatter3164 {
            facility: Facility::LOG_AUTH,
            hostname: None,
            process: MODULE_NAME.into(),
            pid: std::process::id(),
        };
        if let Ok(logger) = syslog::unix(formatter) {
            let _ = log::set_boxed_logger(Box::new(BasicLogger::new(logger)));
            log::set_max_level(log::LevelFilter::Info);
        }
    });
}

// -------------- libc shims + caller-uid lookup ----------------------------

unsafe extern "C" {
    #[link_name = "getuid"]
    fn libc_getuid_raw() -> u32;
    #[link_name = "getppid"]
    fn libc_getppid_raw() -> i32;
    #[link_name = "getpid"]
    fn libc_getpid_raw() -> i32;
}

pub(crate) unsafe fn libc_getuid() -> u32 {
    unsafe { libc_getuid_raw() }
}

pub(crate) unsafe fn libc_getppid() -> i32 {
    unsafe { libc_getppid_raw() }
}

pub(crate) unsafe fn libc_getpid() -> i32 {
    unsafe { libc_getpid_raw() }
}

/// Identify the calling (human) user, even when the immediate PAM
/// caller is a setuid binary or socket-activated systemd service.
///
/// Strategy, in order:
/// 1. `/proc/<ppid>/loginuid` — set by login/PAM at session start,
///    inherited through forks, immune to setuid transitions. Returns
///    `(uint32_t)-1` for processes not in a login session.
/// 2. `/proc/<ppid>/status` — `Uid:` line, real-uid (first field).
///    Works for non-login processes (e.g. systemd services).
/// 3. Fall back to our own real uid.
pub(crate) fn caller_uid(ppid: i32) -> u32 {
    if ppid > 0
        && let Ok(s) = std::fs::read_to_string(format!("/proc/{ppid}/loginuid"))
        && let Ok(uid) = s.trim().parse::<u32>()
        && uid != u32::MAX
    {
        return uid;
    }
    if ppid > 0
        && let Ok(s) = std::fs::read_to_string(format!("/proc/{ppid}/status"))
    {
        for line in s.lines() {
            if let Some(rest) = line.strip_prefix("Uid:")
                && let Some(real) = rest.split_whitespace().next()
                && let Ok(uid) = real.parse::<u32>()
            {
                return uid;
            }
        }
    }
    unsafe { libc_getuid() }
}
