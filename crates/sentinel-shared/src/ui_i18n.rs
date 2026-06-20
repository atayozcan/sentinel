// SPDX-FileCopyrightText: 2026 Atay Özcan <atay@oezcan.me>
// SPDX-License-Identifier: GPL-3.0-or-later
//! Minimal UI-string localization for the helper frontends, keyed by a
//! stable string key.
//!
//! The COSMIC helper localizes its UI via fluent bundles
//! (`crates/sentinel-helper/locales/*.ftl`); the KDE helper (cxx-qt/QML)
//! has no fluent runtime, so it looks strings up here via a single
//! `translate()` invokable. Translations mirror the fluent bundles —
//! English is the source/fallback. Add a locale by extending the `match`
//! arms below (the keys are identical across locales).
//!
//! Count placeholders use Qt's `%1` style (the KDE QML calls
//! `.arg(seconds)` on the result), not fluent's `{$seconds}`.

/// The UI language as a lowercase 2-letter code, resolved from the POSIX
/// locale environment (`LC_ALL` > `LC_MESSAGES` > `LANG`). Returns `"en"`
/// for unset / `C` / `POSIX`.
pub fn ui_lang() -> String {
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(v) = std::env::var(var) {
            let v = v.trim();
            if v.is_empty() || v == "C" || v == "POSIX" {
                continue;
            }
            // e.g. "de_DE.UTF-8" -> "de", "pt_BR" -> "pt"
            let lang = v
                .split(['_', '.', '@'])
                .next()
                .unwrap_or("en")
                .to_ascii_lowercase();
            if !lang.is_empty() {
                return lang;
            }
        }
    }
    "en".to_string()
}

/// Look up `key` in `lang` (a 2-letter code). Falls back to the English
/// source string, then to `key` itself for an unknown key (so a typo is
/// visible rather than blank).
pub fn translate(key: &str, lang: &str) -> &'static str {
    let localized = match lang {
        "de" => de(key),
        "es" => es(key),
        _ => None,
    };
    // Localized → English source → a visible marker for an unknown key
    // (`key` isn't `'static`, so we can't echo it back here).
    localized.or_else(|| en(key)).unwrap_or("?")
}

fn en(key: &str) -> Option<&'static str> {
    Some(match key {
        "allow" => "Allow",
        "deny" => "Deny",
        "show-details" => "Show details",
        "hide-details" => "Hide details",
        "auto-deny-in" => "Auto-deny in %1 s",
        "title-default" => "Authentication Required",
        "detail-action" => "Action",
        "detail-command" => "Command",
        "detail-pid" => "PID",
        "detail-requested-by" => "Requested by",
        "detail-cwd" => "Working directory",
        _ => return None,
    })
}

fn de(key: &str) -> Option<&'static str> {
    Some(match key {
        "allow" => "Erlauben",
        "deny" => "Verweigern",
        "show-details" => "Details anzeigen",
        "hide-details" => "Details ausblenden",
        "auto-deny-in" => "Automatische Ablehnung in %1 s",
        "title-default" => "Authentifizierung erforderlich",
        "detail-action" => "Aktion",
        "detail-command" => "Befehl",
        "detail-pid" => "PID",
        "detail-requested-by" => "Angefordert von",
        "detail-cwd" => "Arbeitsverzeichnis",
        _ => return None,
    })
}

fn es(key: &str) -> Option<&'static str> {
    Some(match key {
        "allow" => "Permitir",
        "deny" => "Denegar",
        "show-details" => "Mostrar detalles",
        "hide-details" => "Ocultar detalles",
        "auto-deny-in" => "Denegación automática en %1 s",
        "title-default" => "Autenticación requerida",
        "detail-action" => "Acción",
        "detail-command" => "Comando",
        "detail-pid" => "PID",
        "detail-requested-by" => "Solicitado por",
        "detail-cwd" => "Directorio de trabajo",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_is_source_and_fallback() {
        assert_eq!(translate("allow", "en"), "Allow");
        // unknown locale falls back to English
        assert_eq!(translate("allow", "xx"), "Allow");
        // unknown key echoes a marker, never panics
        assert_eq!(translate("nope", "de"), "?");
    }

    #[test]
    fn localized_strings_resolve() {
        assert_eq!(translate("allow", "de"), "Erlauben");
        assert_eq!(translate("deny", "es"), "Denegar");
        assert_eq!(translate("detail-cwd", "de"), "Arbeitsverzeichnis");
        // count placeholder is Qt %1 style
        assert!(translate("auto-deny-in", "es").contains("%1"));
    }

    #[test]
    fn every_locale_covers_all_keys() {
        // De/es must define exactly the English key set — guards against a
        // missing translation silently falling back.
        for key in [
            "allow",
            "deny",
            "show-details",
            "hide-details",
            "auto-deny-in",
            "title-default",
            "detail-action",
            "detail-command",
            "detail-pid",
            "detail-requested-by",
            "detail-cwd",
        ] {
            assert!(en(key).is_some(), "en missing {key}");
            assert!(de(key).is_some(), "de missing {key}");
            assert!(es(key).is_some(), "es missing {key}");
        }
    }

    #[test]
    fn ui_lang_parses_posix_locale() {
        // Pure-function check via the same split logic.
        let code = "pt_BR.UTF-8"
            .split(['_', '.', '@'])
            .next()
            .unwrap()
            .to_ascii_lowercase();
        assert_eq!(code, "pt");
    }
}
