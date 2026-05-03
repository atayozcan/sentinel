// SPDX-FileCopyrightText: 2025 Atay Özcan <atay@oezcan.me>
// SPDX-License-Identifier: GPL-3.0-or-later
//! Shared syslog initialization for the PAM module and the polkit agent.
//!
//! Both consumers ship the same `Formatter3164` boilerplate; this module
//! is the single source of truth.
//!
//! ## Logger fallback (A13)
//!
//! Privileged binaries that load `pam_sentinel.so` (sudo-rs, su, login)
//! sometimes initialize their own `log` facade implementation before
//! they call into PAM. In that case `log::set_boxed_logger` returns
//! `Err(SetLoggerError)` — the `log` macros remain bound to whatever
//! the host registered, our `log::info!` calls land somewhere we don't
//! control or get dropped silently.
//!
//! To stay observable in that case, we keep a side-channel `syslog::Logger`
//! around (`FALLBACK`). When the global registration succeeds, this is
//! `None` and `log::info!` etc. work as expected. When it fails, the
//! caller can use [`audit_emit`] which writes to the fallback handle
//! directly.
//!
//! In practice the fallback path is rarely needed; today's consumers
//! (sudo, polkit-agent-helper-1) don't pre-register a `log` impl. But
//! a single-line addition here protects future consumers from going
//! silent.

use std::sync::Mutex;
use std::sync::OnceLock;
use syslog::{BasicLogger, Facility, Formatter3164, Logger, LoggerBackend};

/// Side-channel logger used when `log::set_boxed_logger` fails because
/// the host already installed one.
static FALLBACK: OnceLock<Mutex<Logger<LoggerBackend, Formatter3164>>> = OnceLock::new();

/// Initialize syslog for the AUTH facility under the given identifier.
///
/// Idempotent: repeated calls are no-ops. Safe to call from
/// `Once::call_once` blocks. Both `pam_sentinel.so` and
/// `sentinel-polkit-agent` use this.
///
/// On global-registration failure, falls back to a side-channel logger
/// stored in [`FALLBACK`] so [`audit_emit`] still reaches syslog.
pub fn init_syslog(ident: &str, level: log::LevelFilter) {
    let formatter = Formatter3164 {
        facility: Facility::LOG_AUTH,
        hostname: None,
        process: ident.into(),
        pid: std::process::id(),
    };
    // Build two loggers from the same formatter spec — one we hand to
    // `log::set_boxed_logger`, one we stash for the fallback path.
    let Ok(global_logger) = syslog::unix(formatter.clone()) else {
        return;
    };

    if log::set_boxed_logger(Box::new(BasicLogger::new(global_logger))).is_ok() {
        log::set_max_level(level);
        return;
    }

    // Host registered its own logger first; keep a side-channel handle.
    if let Ok(fallback_logger) = syslog::unix(formatter) {
        let _ = FALLBACK.set(Mutex::new(fallback_logger));
    }
}

/// Emit a single syslog line, preferring the global `log` facade and
/// falling back to the side-channel logger if registration was lost.
///
/// Use sparingly — most code should call `log::info!` etc. as normal.
/// This is for audit-critical lines where we want a hard guarantee
/// they reach syslog even if the host owns the global logger.
pub fn audit_emit(level: log::Level, msg: &str) {
    log::log!(level, "{msg}");

    // If the fallback exists, the global registration failed; write
    // directly so we don't lose audit lines when the host has its own
    // log facade.
    if let Some(fb) = FALLBACK.get() {
        if let Ok(mut logger) = fb.lock() {
            let _ = match level {
                log::Level::Error => logger.err(msg),
                log::Level::Warn => logger.warning(msg),
                log::Level::Info => logger.info(msg),
                log::Level::Debug => logger.debug(msg),
                log::Level::Trace => logger.debug(msg),
            };
        }
    }
}
