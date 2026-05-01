mod agent_bypass;
mod config;
mod display;
mod helper;
mod proc_info;

use config::{HeadlessAction, format_message, load};
use helper::{HelperRequest, HelperResult, run as run_helper};
use pam::constants::{PamFlag, PamResultCode};
use pam::module::{PamHandle, PamHooks};
use proc_info::ProcessInfo;
use std::ffi::CStr;
use syslog::{BasicLogger, Facility, Formatter3164};

const MODULE_NAME: &str = "pam_sentinel";

struct PamSentinel;
pam::pam_hooks!(PamSentinel);

impl PamHooks for PamSentinel {
    fn sm_authenticate(
        pamh: &mut PamHandle,
        _args: Vec<&CStr>,
        _flags: PamFlag,
    ) -> PamResultCode {
        init_logger();

        // Recursion-prevention: when invoked from polkit-agent-helper-1
        // under the Sentinel polkit agent, the agent's HMAC bypass token
        // is in the environment. If it verifies, return PAM_SUCCESS
        // immediately and don't spawn another dialog.
        if let Some(rc) = agent_bypass::check_agent_bypass() {
            return rc;
        }

        let service = pamh
            .get_item::<pam::items::Service>()
            .ok()
            .flatten()
            .and_then(|s| s.to_str().ok().map(str::to_owned))
            .unwrap_or_else(|| "unknown".into());

        let cfg = load(&service);

        if !cfg.enabled {
            log::debug!("{MODULE_NAME}: disabled for service {service}");
            return PamResultCode::PAM_IGNORE;
        }

        // Identify the *requesting* user — the human who'll see the dialog.
        // For pkexec/sudo, this is the calling user (uid != 0), not the
        // target user being authenticated to (often root).
        //
        // PAM's get_user() can fail or return the target user depending on
        // the service (polkit, in particular, doesn't always propagate
        // PAM_USER), so we read /proc/<ppid>/loginuid which is set by
        // login/PAM at session start and immune to setuid transitions.
        let requesting_pid = unsafe { libc_getppid() };
        let requesting_uid = caller_uid(requesting_pid);
        let user = match nix::unistd::User::from_uid(nix::unistd::Uid::from_raw(requesting_uid)) {
            Ok(Some(u)) => u.name,
            _ => pamh
                .get_user(None)
                .ok()
                .unwrap_or_else(|| "unknown".into()),
        };

        if !display::detect_for_user(requesting_uid) {
            return match cfg.headless_action {
                HeadlessAction::Allow => {
                    if cfg.log_attempts {
                        log::warn!(
                            "{MODULE_NAME}: no display, allowing (service {service}, user {user})"
                        );
                    }
                    PamResultCode::PAM_SUCCESS
                }
                HeadlessAction::Deny => {
                    if cfg.log_attempts {
                        log::info!(
                            "{MODULE_NAME}: no display, denying (service {service}, user {user})"
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
            };
        }

        let process = ProcessInfo::for_pid(requesting_pid);

        let formatted_message = format_message(&cfg.message, &user, &service, &process.name);
        let formatted_secondary = format_message(&cfg.secondary, &user, &service, &process.name);

        let req = HelperRequest {
            cfg: &cfg,
            user: &user,
            service: &service,
            process: &process,
            formatted_message: &formatted_message,
            formatted_secondary: &formatted_secondary,
            target_uid: requesting_uid,
            requesting_pid,
        };

        let result = run_helper(&req);

        if cfg.log_attempts {
            match &result {
                Ok(HelperResult::Allow) => log::info!(
                    "{MODULE_NAME}: user {user}, service {service}, process {}: ALLOW",
                    process.name
                ),
                Ok(HelperResult::Deny) => log::info!(
                    "{MODULE_NAME}: user {user}, service {service}, process {}: DENY",
                    process.name
                ),
                Err(e) => log::warn!(
                    "{MODULE_NAME}: helper error for user {user}, service {service}: {e}"
                ),
            }
        }

        match result {
            Ok(HelperResult::Allow) => PamResultCode::PAM_SUCCESS,
            _ => PamResultCode::PAM_AUTH_ERR,
        }
    }

    fn sm_setcred(
        _pamh: &mut PamHandle,
        _args: Vec<&CStr>,
        _flags: PamFlag,
    ) -> PamResultCode {
        PamResultCode::PAM_SUCCESS
    }
}

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

unsafe extern "C" {
    #[link_name = "getuid"]
    fn libc_getuid_raw() -> u32;
    #[link_name = "getppid"]
    fn libc_getppid_raw() -> i32;
}

pub(crate) unsafe fn libc_getuid() -> u32 {
    unsafe { libc_getuid_raw() }
}

pub(crate) unsafe fn libc_getppid() -> i32 {
    unsafe { libc_getppid_raw() }
}

/// Identify the calling (human) user, even when the immediate caller is a
/// setuid binary like sudo or pkexec.
///
/// Strategy:
/// 1. `/proc/<ppid>/loginuid` — set by login/PAM at session start, immune to
///    setuid transitions, the canonical Linux audit answer to "who is this
///    really?". Returns `(uint32_t)-1` (i.e. 0xFFFFFFFF) for processes not
///    associated with a login session.
/// 2. `/proc/<ppid>/status` — the `Uid:` line, real-uid (first field).
///    Works even for non-login processes.
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
