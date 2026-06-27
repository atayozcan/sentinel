// SPDX-FileCopyrightText: 2025 Atay Özcan <atay@oezcan.me>
// SPDX-License-Identifier: GPL-3.0-or-later
//! The cxx-qt bridge: a single `DialogController` QObject that backs the
//! QML confirmation dialog.
//!
//! All the state QML binds to lives here as Q_PROPERTYs. The countdown /
//! min-display gate is driven by a QML `Timer` calling [`tick`](qobject::DialogController::tick)
//! every 100 ms — the thresholds stay in Rust so the two helpers behave
//! identically. The terminal actions print the verdict to stdout and
//! exit the process directly (the PAM module / polkit agent read that
//! single `ALLOW`/`DENY`/`TIMEOUT` line), so there's no need to thread a
//! return value back out of the Qt event loop.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// An alias to the QString type from cxx-qt-lib.
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        // Admin/PAM-supplied strings (rendered verbatim).
        #[qproperty(QString, title)]
        #[qproperty(QString, message)]
        #[qproperty(QString, secondary)]
        // Requesting-process identification.
        #[qproperty(QString, process_exe, cxx_name = "processExe")]
        #[qproperty(QString, process_cmdline, cxx_name = "processCmdline")]
        #[qproperty(i32, process_pid, cxx_name = "processPid")]
        #[qproperty(QString, process_cwd, cxx_name = "processCwd")]
        #[qproperty(QString, requesting_user, cxx_name = "requestingUser")]
        #[qproperty(QString, action)]
        #[qproperty(QString, icon_name, cxx_name = "iconName")]
        // Layout / behavior flags.
        #[qproperty(bool, has_details, cxx_name = "hasDetails")]
        #[qproperty(bool, allow_first, cxx_name = "allowFirst")]
        // Timing.
        #[qproperty(i32, timeout_secs, cxx_name = "timeoutSecs")]
        #[qproperty(i32, min_time_ms, cxx_name = "minTimeMs")]
        #[qproperty(i32, remaining_secs, cxx_name = "remainingSecs")]
        #[qproperty(i32, elapsed_ms, cxx_name = "elapsedMs")]
        #[qproperty(f64, progress_fraction, cxx_name = "progressFraction")]
        // Live UI state.
        #[qproperty(bool, allow_enabled, cxx_name = "allowEnabled")]
        #[qproperty(bool, show_details, cxx_name = "showDetails")]
        // "Remember" opt-in checkbox (shown when remember_secs > 0).
        #[qproperty(bool, remember_offered, cxx_name = "rememberOffered")]
        #[qproperty(QString, remember_label, cxx_name = "rememberLabel")]
        #[qproperty(bool, remember_checked, cxx_name = "rememberChecked")]
        type DialogController = super::DialogControllerRust;

        /// 100 ms clock tick from the QML `Timer`: advances elapsed time,
        /// enables Allow once `min_time_ms` has passed, updates the
        /// countdown, and auto-denies on timeout.
        #[qinvokable]
        fn tick(self: Pin<&mut Self>);

        /// User pressed Allow. No-op until `allow_enabled` is true.
        #[qinvokable]
        fn allow(self: Pin<&mut Self>);

        /// User pressed Deny (or Escape). Always denies.
        #[qinvokable]
        fn deny(self: Pin<&mut Self>);

        /// Expand/collapse the process details section.
        #[qinvokable]
        #[cxx_name = "toggleDetails"]
        fn toggle_details(self: Pin<&mut Self>);

        /// Localized UI string lookup, replacing `qsTr()` in QML. Keys
        /// like `"allow"`, `"deny"`, `"auto-deny-in"`; see
        /// `sentinel_shared::ui_i18n`.
        #[qinvokable]
        fn translate(&self, key: &QString) -> QString;
    }
}

use core::pin::Pin;
use cxx_qt_lib::QString;
use sentinel_shared::{Outcome, Verdict};

/// QML `Timer` interval, milliseconds. Counting ticks (rather than
/// reading a wall clock) keeps `tick` to plain property getters/setters.
const TICK_MS: i32 = 100;

/// Backing data for the `DialogController` QObject. Field values become
/// the initial Q_PROPERTY values via [`Default`], which pulls from the
/// process-wide parsed CLI args.
pub struct DialogControllerRust {
    title: QString,
    message: QString,
    secondary: QString,
    process_exe: QString,
    process_cmdline: QString,
    process_pid: i32,
    process_cwd: QString,
    requesting_user: QString,
    action: QString,
    icon_name: QString,
    has_details: bool,
    allow_first: bool,
    timeout_secs: i32,
    min_time_ms: i32,
    remaining_secs: i32,
    elapsed_ms: i32,
    progress_fraction: f64,
    allow_enabled: bool,
    show_details: bool,
    remember_offered: bool,
    remember_label: QString,
    remember_checked: bool,
}

impl Default for DialogControllerRust {
    fn default() -> Self {
        let a = crate::args();

        let has_details = a.process_cmdline.is_some()
            || a.process_pid.is_some()
            || a.process_cwd.is_some()
            || a.requesting_user.is_some()
            || a.action.is_some();

        // Randomized button order pushes against muscle-memory click-through.
        // When not randomizing, keep the conventional Allow-first order.
        let allow_first = if a.randomize {
            rand::random_bool(0.5)
        } else {
            true
        };

        let icon_name =
            sentinel_shared::resolve_icon_name(a.process_exe.as_deref()).unwrap_or_default();

        let timeout_secs = i32::try_from(a.timeout).unwrap_or(i32::MAX);
        let min_time_ms = i32::try_from(a.min_time).unwrap_or(i32::MAX);

        // "Remember" opt-in checkbox: offered only when the backend passes
        // a non-zero window. Label is localized "Remember for <duration>".
        let remember_offered = a.remember_secs > 0;
        let remember_label = if remember_offered {
            let secs = a.remember_secs;
            let dur = if secs >= 60 {
                format!("{} min", secs / 60)
            } else {
                format!("{secs} s")
            };
            let tmpl = sentinel_shared::ui_i18n::remember_label_template(ui_lang());
            QString::from(tmpl.replace("%1", &dur).as_str())
        } else {
            QString::default()
        };

        // Clamp every helper-supplied string. The process exe/cmdline/cwd
        // come from /proc of the requesting process and are attacker-
        // influenceable; an unbounded value would stall QML layout.
        Self {
            title: QString::from(clip(a.title.as_str(), 1024).as_str()),
            message: QString::from(clip(a.message.as_str(), 2048).as_str()),
            secondary: QString::from(clip(a.secondary.as_str(), 1024).as_str()),
            process_exe: QString::from(clip(a.process_exe.as_deref().unwrap_or(""), 512).as_str()),
            process_cmdline: QString::from(
                clip(a.process_cmdline.as_deref().unwrap_or(""), 4096).as_str(),
            ),
            process_pid: a.process_pid.unwrap_or(0),
            process_cwd: QString::from(clip(a.process_cwd.as_deref().unwrap_or(""), 4096).as_str()),
            requesting_user: QString::from(
                clip(a.requesting_user.as_deref().unwrap_or(""), 256).as_str(),
            ),
            action: QString::from(clip(a.action.as_deref().unwrap_or(""), 1024).as_str()),
            icon_name: QString::from(icon_name.as_str()),
            has_details,
            allow_first,
            timeout_secs,
            min_time_ms,
            remaining_secs: timeout_secs,
            elapsed_ms: 0,
            progress_fraction: 0.0,
            // min_time == 0 → Allow usable immediately.
            allow_enabled: a.min_time == 0,
            show_details: false,
            remember_offered,
            remember_label,
            remember_checked: false,
        }
    }
}

impl qobject::DialogController {
    /// See the bridge declaration. Drives min-time gating, the progress
    /// bar / countdown, and the auto-deny timeout.
    pub fn tick(mut self: Pin<&mut Self>) {
        let elapsed = *self.elapsed_ms() + TICK_MS;
        let min_time = *self.min_time_ms();
        let timeout = *self.timeout_secs();
        let already_enabled = *self.allow_enabled();

        self.as_mut().set_elapsed_ms(elapsed);

        if !already_enabled && elapsed >= min_time {
            self.as_mut().set_allow_enabled(true);
        }

        if timeout > 0 {
            let total = timeout.saturating_mul(1000);
            let frac = (f64::from(elapsed) / f64::from(total)).clamp(0.0, 1.0);
            self.as_mut().set_progress_fraction(frac);

            let remaining = ((total - elapsed).max(0) as f64 / 1000.0).ceil() as i32;
            self.as_mut().set_remaining_secs(remaining);

            if elapsed >= total {
                finish_outcome(Outcome::Timeout);
            }
        }
    }

    /// User pressed Allow. If the "remember" checkbox was offered and
    /// ticked, the verdict carries the opt-in so the backend records it.
    pub fn allow(self: Pin<&mut Self>) {
        if *self.allow_enabled() {
            let remember = *self.remember_offered() && *self.remember_checked();
            finish(Verdict {
                outcome: Outcome::Allow,
                remember,
            });
        }
    }

    /// User pressed Deny or Escape.
    pub fn deny(self: Pin<&mut Self>) {
        finish_outcome(Outcome::Deny);
    }

    /// Toggle the expandable details panel.
    pub fn toggle_details(mut self: Pin<&mut Self>) {
        let shown = *self.show_details();
        self.as_mut().set_show_details(!shown);
    }

    /// Localized UI string for `key` in the process UI language. Called
    /// from QML in place of `qsTr()`.
    pub fn translate(&self, key: &QString) -> QString {
        QString::from(sentinel_shared::ui_i18n::translate(
            &key.to_string(),
            ui_lang(),
        ))
    }
}

/// Process UI language (2-letter code), resolved once from the locale
/// env. Cached so the per-string `translate` calls don't re-read env.
fn ui_lang() -> &'static str {
    use std::sync::OnceLock;
    static LANG: OnceLock<String> = OnceLock::new();
    LANG.get_or_init(sentinel_shared::ui_i18n::ui_lang).as_str()
}

/// Write the verdict the PAM module / polkit agent read, then exit with
/// the matching code. We flush explicitly because `process::exit` does
/// not flush Rust's (block-buffered when piped) stdout, and the parent
/// reads exactly this one line.
fn finish(verdict: Verdict) -> ! {
    use std::io::Write;
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{verdict}");
    let _ = out.flush();
    std::process::exit(verdict.outcome.exit_code());
}

/// Write a bare outcome (no "remember" opt-in) — used by Deny / Timeout.
fn finish_outcome(outcome: Outcome) -> ! {
    finish(Verdict {
        outcome,
        remember: false,
    })
}

/// Fail-safe used by `main` if the event loop ever returns without a
/// verdict (e.g. the surface was closed by the compositor).
pub fn finish_deny() -> ! {
    finish_outcome(Outcome::Deny)
}

/// Clamp untrusted text to `max` characters. The requesting process's
/// cmdline/exe/cwd come from `/proc` and can be arbitrarily long; rendering
/// a multi-megabyte string would stall the QML layout engine. Appends an
/// ellipsis when the value is cut.
fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max).collect();
        out.push('…');
        out
    }
}
