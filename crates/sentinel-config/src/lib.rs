//! Shared configuration schema for the Sentinel PAM module
//! (`pam-sentinel`) and the polkit authentication agent
//! (`sentinel-polkit-agent`).
//!
//! The on-disk format is TOML at `/etc/security/sentinel.conf`
//! (`SENTINEL_CONFIG_PATH`, baked at compile time by this crate's
//! `build.rs`). The file is root-owned and intentionally NOT
//! user-editable: a per-user override layer would defeat the whole
//! UAC contract by letting an unprivileged user lower their own
//! `timeout` to zero.
//!
//! # Public API
//!
//! - [`load`] — read the file, return the effective [`ServiceConfig`]
//!   for one PAM service. The hot path for both consumers.
//! - [`Document`] — full parsed view; lets the upcoming settings UI
//!   walk all sections without re-implementing the schema.
//! - [`format_message`] — `%u`/`%s`/`%p`/`%%` substitution for dialog
//!   message templates.
//!
//! # Failure handling
//!
//! `load` is infallible by design: missing-file falls back silently to
//! defaults; malformed-file falls back to defaults *and logs a WARN*.
//! That asymmetry is deliberate — you don't want a typo in the config
//! to silently revert your security settings without a trail in
//! `journalctl -t pam_sentinel` (or the agent's syslog identifier).

use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Compile-time absolute path to the system config file. Set by this
/// crate's `build.rs` from `$SENTINEL_SYSCONFDIR/security/sentinel.conf`.
pub const CONFIG_PATH: &str = env!("SENTINEL_CONFIG_PATH");

/// Filename of the bypass socket the polkit agent binds and the PAM
/// module connects to. Lives in the user's `XDG_RUNTIME_DIR` (defaults
/// to `/run/user/<uid>`). Defined here so both consumers (agent server
/// + PAM client) agree — diverging path == silently broken bypass.
pub const BYPASS_SOCKET_BASENAME: &str = "sentinel-agent.sock";

/// Compute the full path to the bypass socket for a given user.
/// Honors `XDG_RUNTIME_DIR` when set + non-empty, falls back to
/// `/run/user/<uid>/sentinel-agent.sock`.
pub fn bypass_socket_path(uid: u32) -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") {
        if !dir.is_empty() {
            return PathBuf::from(dir).join(BYPASS_SOCKET_BASENAME);
        }
    }
    PathBuf::from(format!("/run/user/{uid}")).join(BYPASS_SOCKET_BASENAME)
}

/// Verdict the helper writes on stdout, parsed back by both the PAM
/// module's pipe reader and the polkit agent's child-process line
/// reader. The Display + FromStr impls are the *only* source of truth
/// for the wire format — keep this enum and those impls in sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Allow,
    Deny,
    Timeout,
}

impl std::fmt::Display for Outcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Allow => "ALLOW",
            Self::Deny => "DENY",
            Self::Timeout => "TIMEOUT",
        })
    }
}

/// `Err(())` for unrecognized input. Callers decide the policy: the
/// PAM module treats anything-not-Allow as `PAM_AUTH_ERR`; the agent
/// surfaces Timeout separately for logging.
impl std::str::FromStr for Outcome {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "ALLOW" => Ok(Self::Allow),
            "DENY" => Ok(Self::Deny),
            "TIMEOUT" => Ok(Self::Timeout),
            _ => Err(()),
        }
    }
}

impl Outcome {
    /// Process exit code matching the verdict: 0 for Allow (auth ok),
    /// 1 for Deny / Timeout (auth refused). The helper exits with this.
    pub fn exit_code(self) -> i32 {
        match self {
            Self::Allow => 0,
            Self::Deny | Self::Timeout => 1,
        }
    }

    pub fn is_allow(self) -> bool {
        matches!(self, Self::Allow)
    }
}

/// What to do when no Wayland display is reachable from the PAM call site.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HeadlessAction {
    /// Silently `PAM_SUCCESS`. Dangerous; only for tightly controlled boxes.
    Allow,
    /// `PAM_AUTH_ERR`. Caller (sudo, polkit) sees a hard fail.
    Deny,
    /// `PAM_IGNORE`. Next module in the stack runs (typically pam_unix
    /// → password prompt). Default.
    #[default]
    Password,
}

/// Top-level parsed config. Public so the settings UI can walk it.
#[derive(Debug, Clone, Deserialize)]
pub struct Document {
    #[serde(default)]
    pub general: General,
    #[serde(default)]
    pub appearance: Appearance,
    #[serde(default)]
    pub services: HashMap<String, ServiceOverride>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct General {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_timeout")]
    pub timeout: u32,
    #[serde(default = "default_true")]
    pub randomize_buttons: bool,
    #[serde(default)]
    pub headless_action: HeadlessAction,
    #[serde(default = "default_true")]
    pub show_process_info: bool,
    #[serde(default = "default_true")]
    pub log_attempts: bool,
    #[serde(default = "default_min_display_time")]
    pub min_display_time_ms: u32,
}

impl Default for General {
    fn default() -> Self {
        Self {
            enabled: true,
            timeout: default_timeout(),
            randomize_buttons: true,
            headless_action: HeadlessAction::default(),
            show_process_info: true,
            log_attempts: true,
            min_display_time_ms: default_min_display_time(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Appearance {
    #[serde(default = "default_title")]
    pub title: String,
    #[serde(default = "default_message")]
    pub message: String,
    #[serde(default = "default_secondary")]
    pub secondary: String,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            title: default_title(),
            message: default_message(),
            secondary: default_secondary(),
        }
    }
}

/// Per-service override block (`[services.<name>]`). Any `None` field
/// inherits from `[general]`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ServiceOverride {
    pub enabled: Option<bool>,
    pub timeout: Option<u32>,
    pub randomize: Option<bool>,
}

/// Effective config for a single PAM service after applying overrides
/// on top of `[general]` + `[appearance]`. This is what consumers
/// actually drive the dialog with.
#[derive(Debug, Clone)]
pub struct ServiceConfig {
    pub enabled: bool,
    pub timeout: u32,
    pub randomize_buttons: bool,
    pub headless_action: HeadlessAction,
    pub show_process_info: bool,
    pub log_attempts: bool,
    pub min_display_time_ms: u32,
    pub title: String,
    pub message: String,
    pub secondary: String,
}

impl Document {
    pub fn defaults() -> Self {
        Self {
            general: General::default(),
            appearance: Appearance::default(),
            services: HashMap::new(),
        }
    }

    /// Compute the effective [`ServiceConfig`] for a PAM service name
    /// (e.g. `"sudo"`, `"polkit-1"`). Unknown service names fall through
    /// to plain `[general]` + `[appearance]` defaults.
    pub fn for_service(&self, service: &str) -> ServiceConfig {
        let mut cfg = ServiceConfig {
            enabled: self.general.enabled,
            timeout: self.general.timeout,
            randomize_buttons: self.general.randomize_buttons,
            headless_action: self.general.headless_action,
            show_process_info: self.general.show_process_info,
            log_attempts: self.general.log_attempts,
            min_display_time_ms: self.general.min_display_time_ms,
            title: self.appearance.title.clone(),
            message: self.appearance.message.clone(),
            secondary: self.appearance.secondary.clone(),
        };
        if let Some(over) = self.services.get(service) {
            if let Some(v) = over.enabled {
                cfg.enabled = v;
            }
            if let Some(v) = over.timeout {
                cfg.timeout = v;
            }
            if let Some(v) = over.randomize {
                cfg.randomize_buttons = v;
            }
        }
        cfg
    }

    /// Read + parse the system config file. Falls back to defaults on
    /// any error; logs a warning on parse failure (silent only on
    /// missing file).
    pub fn load() -> Self {
        Self::load_from(Path::new(CONFIG_PATH))
    }

    /// Read + parse a specific path. Same fail-soft semantics as
    /// [`Document::load`]; intended for the settings app reading from a
    /// staging location, or for tests.
    pub fn load_from(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(contents) => match toml::from_str::<Document>(&contents) {
                Ok(parsed) => parsed,
                Err(e) => {
                    log::warn!(
                        "sentinel-config: failed to parse {}: {e} — falling back to defaults",
                        path.display()
                    );
                    Document::defaults()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                log::debug!(
                    "sentinel-config: {} not present — using defaults",
                    path.display()
                );
                Document::defaults()
            }
            Err(e) => {
                log::warn!(
                    "sentinel-config: cannot read {}: {e} — using defaults",
                    path.display()
                );
                Document::defaults()
            }
        }
    }
}

/// Convenience: parse the system config and return the effective
/// per-service config in one call. The hot path used by both
/// `pam_sentinel.so` and `sentinel-polkit-agent`.
pub fn load(service: &str) -> ServiceConfig {
    Document::load().for_service(service)
}

/// Where the system config file lives at runtime. Useful for the
/// settings UI ("save to PathBuf").
pub fn config_path() -> PathBuf {
    PathBuf::from(CONFIG_PATH)
}

/// Substitute `%u` (user), `%s` (service), `%p` (process), and `%%`
/// (literal `%`) into a template. Unknown `%x` sequences are preserved
/// verbatim so a typo is visible to the admin in the rendered dialog
/// rather than silently dropped.
pub fn format_message(template: &str, user: &str, service: &str, process: &str) -> String {
    let mut out = String::with_capacity(template.len());
    let mut chars = template.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('u') => out.push_str(user),
            Some('s') => out.push_str(service),
            Some('p') => out.push_str(process),
            Some('%') => out.push('%'),
            Some(other) => {
                out.push('%');
                out.push(other);
            }
            None => out.push('%'),
        }
    }
    out
}

// Default appearance strings, exposed as `pub const` so the helper can
// detect "this is still the built-in default, translate it" vs "admin
// customized this, use as-is". If you change a const here, update the
// matching `dialog-{title,message,secondary}-default` keys in every
// `crates/sentinel-helper/locales/<lang>/sentinel-helper.ftl`.
pub const DEFAULT_TITLE: &str = "Authentication Required";
pub const DEFAULT_MESSAGE: &str = "The application \"%p\" is requesting elevated privileges.";
pub const DEFAULT_SECONDARY: &str = "Click \"Allow\" to continue or \"Deny\" to cancel.";

/// Logfmt-style helpers for structured audit log lines.
///
/// We intentionally don't pull in a logfmt crate — the format is
/// trivial and the helper is two functions. The output goes to
/// syslog via the existing `log::info!` etc. calls, lands in the
/// systemd journal, and is queryable with `journalctl -t pam_sentinel
/// -t sentinel-polkit-agent --output=json` (the line ends up in the
/// `MESSAGE` field; downstream tooling can split on whitespace +
/// `key=value`).
///
/// # Convention
///
/// Auth-outcome events use `event=auth.{allow,deny,timeout,error}`
/// plus a `source=` discriminator (`dialog` / `bypass` / `headless` /
/// `agent` / `agent.bypass`). Diagnostic messages stay unstructured.
pub mod log_kv {
    /// Quote a value for logfmt: bare token if it contains no
    /// whitespace / `"` / `=`, otherwise wrapped in double quotes
    /// with internal `"` and `\` escaped. Empty values become `""`
    /// so they're visually distinguishable from missing keys.
    pub fn quote(value: &str) -> String {
        if value.is_empty() {
            return "\"\"".into();
        }
        let needs_quoting = value
            .chars()
            .any(|c| c.is_whitespace() || c == '"' || c == '=');
        if !needs_quoting {
            return value.to_string();
        }
        let mut out = String::with_capacity(value.len() + 2);
        out.push('"');
        for c in value.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                _ => out.push(c),
            }
        }
        out.push('"');
        out
    }
}

fn default_true() -> bool {
    true
}
fn default_timeout() -> u32 {
    30
}
fn default_min_display_time() -> u32 {
    500
}
fn default_title() -> String {
    DEFAULT_TITLE.into()
}
fn default_message() -> String {
    DEFAULT_MESSAGE.into()
}
fn default_secondary() -> String {
    DEFAULT_SECONDARY.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- format_message ---------------------------------------------------

    #[test]
    fn format_message_substitutes_all_tokens() {
        let out = format_message("%u %s %p", "alice", "sudo", "/usr/bin/cat");
        assert_eq!(out, "alice sudo /usr/bin/cat");
    }

    #[test]
    fn format_message_handles_literal_percent() {
        let out = format_message("100%% done by %u", "alice", "sudo", "cat");
        assert_eq!(out, "100% done by alice");
    }

    #[test]
    fn format_message_preserves_unknown_tokens() {
        // Unknown %x — neither a known token nor an escape — stays as-is so
        // admins see their typo rather than silently losing characters.
        let out = format_message("%x %u", "alice", "sudo", "cat");
        assert_eq!(out, "%x alice");
    }

    #[test]
    fn format_message_trailing_percent_is_kept() {
        let out = format_message("hello %", "u", "s", "p");
        assert_eq!(out, "hello %");
    }

    #[test]
    fn format_message_empty_template() {
        assert_eq!(format_message("", "u", "s", "p"), "");
    }

    #[test]
    fn format_message_no_substitutions() {
        assert_eq!(format_message("plain text", "u", "s", "p"), "plain text");
    }

    // ---- Document::for_service -------------------------------------------

    fn doc_with_services(services: HashMap<String, ServiceOverride>) -> Document {
        Document {
            general: General::default(),
            appearance: Appearance::default(),
            services,
        }
    }

    #[test]
    fn service_config_uses_general_defaults_for_unknown_service() {
        let doc = doc_with_services(HashMap::new());
        let cfg = doc.for_service("anything");
        assert!(cfg.enabled);
        assert_eq!(cfg.timeout, 30);
        assert!(cfg.randomize_buttons);
    }

    #[test]
    fn service_config_per_service_override_wins() {
        let mut services = HashMap::new();
        services.insert(
            "sudo".to_string(),
            ServiceOverride {
                enabled: Some(false),
                timeout: Some(99),
                randomize: Some(false),
            },
        );
        let doc = doc_with_services(services);
        let cfg = doc.for_service("sudo");
        assert!(!cfg.enabled);
        assert_eq!(cfg.timeout, 99);
        assert!(!cfg.randomize_buttons);
    }

    #[test]
    fn service_config_partial_override_inherits_rest() {
        let mut services = HashMap::new();
        services.insert(
            "su".to_string(),
            ServiceOverride {
                enabled: Some(false),
                timeout: None,
                randomize: None,
            },
        );
        let doc = doc_with_services(services);
        let cfg = doc.for_service("su");
        assert!(!cfg.enabled);
        assert_eq!(cfg.timeout, 30);
        assert!(cfg.randomize_buttons);
    }

    #[test]
    fn service_config_other_services_unaffected() {
        let mut services = HashMap::new();
        services.insert(
            "sudo".to_string(),
            ServiceOverride {
                enabled: Some(false),
                timeout: Some(1),
                randomize: Some(false),
            },
        );
        let doc = doc_with_services(services);
        let polkit_cfg = doc.for_service("polkit-1");
        assert!(polkit_cfg.enabled);
        assert_eq!(polkit_cfg.timeout, 30);
        assert!(polkit_cfg.randomize_buttons);
    }

    // ---- TOML round-trip -------------------------------------------------

    #[test]
    fn parses_full_config_toml() {
        let src = r#"
            [general]
            enabled = true
            timeout = 45
            randomize_buttons = false
            headless_action = "deny"
            min_display_time_ms = 1000

            [appearance]
            title = "Custom"
            message = "msg %u"

            [services.sudo]
            timeout = 5
        "#;
        let doc: Document = toml::from_str(src).expect("parse");
        let cfg = doc.for_service("sudo");
        assert_eq!(cfg.timeout, 5);
        assert_eq!(cfg.headless_action, HeadlessAction::Deny);
        assert!(!cfg.randomize_buttons);
        assert_eq!(cfg.min_display_time_ms, 1000);
        assert_eq!(cfg.title, "Custom");
    }

    #[test]
    fn malformed_toml_is_a_parse_error_not_a_panic() {
        let result: Result<Document, _> = toml::from_str("this is not [valid toml");
        assert!(result.is_err());
    }

    #[test]
    fn headless_action_default_is_password() {
        assert_eq!(HeadlessAction::default(), HeadlessAction::Password);
    }

    // ---- load_from --------------------------------------------------------

    #[test]
    fn load_from_missing_file_returns_defaults() {
        let doc = Document::load_from(Path::new("/nonexistent/sentinel.conf"));
        assert_eq!(doc.general.timeout, 30);
        assert!(doc.services.is_empty());
    }

    #[test]
    fn load_from_real_file_round_trips() {
        // Write a minimal config to a tempfile and load it back.
        let dir = std::env::temp_dir();
        let path = dir.join(format!("sentinel-config-test-{}.toml", std::process::id()));
        std::fs::write(&path, "[general]\ntimeout = 12\n").unwrap();
        let doc = Document::load_from(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(doc.general.timeout, 12);
    }

    // ---- log_kv::quote ---------------------------------------------------

    #[test]
    fn log_kv_bare_token_unquoted() {
        assert_eq!(log_kv::quote("alice"), "alice");
        assert_eq!(log_kv::quote("/usr/bin/sudo"), "/usr/bin/sudo");
        assert_eq!(log_kv::quote("polkit-1"), "polkit-1");
    }

    #[test]
    fn log_kv_whitespace_gets_quoted() {
        assert_eq!(log_kv::quote("hello world"), "\"hello world\"");
        assert_eq!(log_kv::quote("a\tb"), "\"a\tb\"");
    }

    #[test]
    fn log_kv_internal_quotes_escaped() {
        assert_eq!(log_kv::quote("a\"b"), "\"a\\\"b\"");
    }

    #[test]
    fn log_kv_empty_becomes_quoted_empty() {
        // Distinguishes "key=" from "key" — the latter would parse
        // ambiguously in some logfmt implementations.
        assert_eq!(log_kv::quote(""), "\"\"");
    }

    #[test]
    fn log_kv_equals_sign_gets_quoted() {
        // = inside a value would otherwise look like a key boundary.
        assert_eq!(log_kv::quote("a=b"), "\"a=b\"");
    }

    #[test]
    fn log_kv_backslash_escaped() {
        assert_eq!(log_kv::quote("a\\b"), "a\\b"); // bare backslash without quoting trigger stays
        assert_eq!(log_kv::quote("a b\\c"), "\"a b\\\\c\""); // gets escaped when wrapping
    }

    // ---- Outcome ----------------------------------------------------------

    #[test]
    fn outcome_display_strings_are_stable_protocol() {
        // The wire format the helper writes and consumers parse. Bumping
        // these is a wire-protocol break.
        assert_eq!(Outcome::Allow.to_string(), "ALLOW");
        assert_eq!(Outcome::Deny.to_string(), "DENY");
        assert_eq!(Outcome::Timeout.to_string(), "TIMEOUT");
    }

    #[test]
    fn outcome_round_trips_through_str() {
        for v in [Outcome::Allow, Outcome::Deny, Outcome::Timeout] {
            assert_eq!(v.to_string().parse::<Outcome>().unwrap(), v);
        }
    }

    #[test]
    fn outcome_from_str_strips_whitespace() {
        assert_eq!("ALLOW\n".parse::<Outcome>(), Ok(Outcome::Allow));
        assert_eq!(" DENY ".parse::<Outcome>(), Ok(Outcome::Deny));
    }

    #[test]
    fn outcome_unknown_is_error() {
        assert!("MAYBE".parse::<Outcome>().is_err());
        assert!("".parse::<Outcome>().is_err());
        assert!("allow".parse::<Outcome>().is_err()); // case-sensitive on purpose
    }

    #[test]
    fn outcome_exit_code_allow_is_zero() {
        assert_eq!(Outcome::Allow.exit_code(), 0);
        assert_eq!(Outcome::Deny.exit_code(), 1);
        assert_eq!(Outcome::Timeout.exit_code(), 1);
    }

    #[test]
    fn outcome_is_allow_helper() {
        assert!(Outcome::Allow.is_allow());
        assert!(!Outcome::Deny.is_allow());
        assert!(!Outcome::Timeout.is_allow());
    }
}
