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

        let user = match pamh.get_user(None) {
            Ok(u) => u,
            Err(e) => {
                log::error!("{MODULE_NAME}: cannot get username: {e:?}");
                return PamResultCode::PAM_USER_UNKNOWN;
            }
        };

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

        let target_uid = match nix::unistd::User::from_name(&user) {
            Ok(Some(u)) => u.uid.as_raw(),
            _ => unsafe { libc_getuid() },
        };

        if !display::detect_for_user(target_uid) {
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

        let requesting_pid = unsafe { libc_getppid() };
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
            target_uid,
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

unsafe fn libc_getuid() -> u32 {
    unsafe { libc_getuid_raw() }
}

unsafe fn libc_getppid() -> i32 {
    unsafe { libc_getppid_raw() }
}
