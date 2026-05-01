use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

pub const CONFIG_PATH: &str = env!("SENTINEL_CONFIG_PATH");

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HeadlessAction {
    Allow,
    Deny,
    #[default]
    Password,
}

#[derive(Debug, Deserialize)]
struct RawConfig {
    #[serde(default)]
    general: General,
    #[serde(default)]
    appearance: Appearance,
    #[serde(default)]
    services: HashMap<String, ServiceOverride>,
}

#[derive(Debug, Deserialize)]
struct General {
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default = "default_timeout")]
    timeout: u32,
    #[serde(default = "default_true")]
    randomize_buttons: bool,
    #[serde(default)]
    headless_action: HeadlessAction,
    #[serde(default = "default_true")]
    show_process_info: bool,
    #[serde(default = "default_true")]
    log_attempts: bool,
    #[serde(default = "default_min_display_time")]
    min_display_time_ms: u32,
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

#[derive(Debug, Deserialize)]
struct Appearance {
    #[serde(default = "default_title")]
    title: String,
    #[serde(default = "default_message")]
    message: String,
    #[serde(default = "default_secondary")]
    secondary: String,
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

#[derive(Debug, Default, Deserialize)]
struct ServiceOverride {
    enabled: Option<bool>,
    timeout: Option<u32>,
    randomize: Option<bool>,
}

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

impl ServiceConfig {
    fn from_raw(raw: &RawConfig, service: &str) -> Self {
        let mut cfg = Self {
            enabled: raw.general.enabled,
            timeout: raw.general.timeout,
            randomize_buttons: raw.general.randomize_buttons,
            headless_action: raw.general.headless_action,
            show_process_info: raw.general.show_process_info,
            log_attempts: raw.general.log_attempts,
            min_display_time_ms: raw.general.min_display_time_ms,
            title: raw.appearance.title.clone(),
            message: raw.appearance.message.clone(),
            secondary: raw.appearance.secondary.clone(),
        };
        if let Some(over) = raw.services.get(service) {
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
}

pub fn load(service: &str) -> ServiceConfig {
    let path = Path::new(CONFIG_PATH);
    let raw = std::fs::read_to_string(path)
        .ok()
        .and_then(|s| toml::from_str::<RawConfig>(&s).ok())
        .unwrap_or_else(|| RawConfig {
            general: General::default(),
            appearance: Appearance::default(),
            services: HashMap::new(),
        });
    ServiceConfig::from_raw(&raw, service)
}

/// Substitute %u (user), %s (service), %p (process) into a template.
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
    "Authentication Required".into()
}
fn default_message() -> String {
    "The application \"%p\" is requesting elevated privileges.".into()
}
fn default_secondary() -> String {
    "Click \"Allow\" to continue or \"Deny\" to cancel.".into()
}
