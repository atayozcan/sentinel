//! Localization for the helper's own UI chrome.
//!
//! Bundles are **embedded** in the binary at compile time
//! (`include_str!`). This keeps the helper self-contained — no
//! `/usr/share/sentinel/locales/` runtime dependency, no risk that
//! a path-based locale lookup gets influenced by user-controlled
//! `LANG` (we only use `LANG` to *select* a baked-in bundle).
//!
//! Locale resolution (POSIX):
//!   1. `LC_ALL`
//!   2. `LC_MESSAGES`
//!   3. `LANG`
//!   4. fallback to `DEFAULT_LOCALE` (`en-US`)
//!
//! The raw value is sanitized aggressively: strip `.charset` and
//! `@modifier` suffixes, normalize `_` → `-`, validate against
//! `[A-Za-z0-9-]{1,16}`, then negotiate against the embedded set.
//! Anything malformed or unrecognized falls back silently to the
//! default — translation is best-effort and must never block auth.
//!
//! # Adding a locale
//!
//! 1. `mkdir crates/sentinel-helper/locales/<bcp47>/`
//! 2. Copy `en-US/sentinel-helper.ftl` and translate the values
//!    (keep the keys + `{$variable}` placeholders identical).
//! 3. Add a row to `BUNDLES` below.
//! 4. Rebuild — the new locale is auto-discoverable via `LANG`.

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource, FluentValue};
use fluent_langneg::{NegotiationStrategy, negotiate_languages};
use std::sync::OnceLock;
// `unic_langid::LanguageIdentifier` is what `fluent_bundle` accepts;
// `fluent_langneg::LanguageIdentifier` is a separate type from
// `icu_locid` used only inside `negotiate_languages`. They aren't
// interchangeable, so we keep them isolated by name.
use fluent_langneg::LanguageIdentifier as NegLangId;
use unic_langid::LanguageIdentifier as BundleLangId;

/// Always available; covers the case where the user's `LANG` doesn't
/// match anything we ship.
const DEFAULT_LOCALE: &str = "en-US";

/// Embedded bundles. Order is irrelevant — `negotiate_languages` does
/// the matching. Add entries here when shipping new translations and
/// the `every_bundle_has_required_keys` test will catch missing keys.
static BUNDLES: &[(&str, &str)] = &[
    (
        "en-US",
        include_str!("../locales/en-US/sentinel-helper.ftl"),
    ),
    (
        "de-DE",
        include_str!("../locales/de-DE/sentinel-helper.ftl"),
    ),
    (
        "es-ES",
        include_str!("../locales/es-ES/sentinel-helper.ftl"),
    ),
    (
        "fr-FR",
        include_str!("../locales/fr-FR/sentinel-helper.ftl"),
    ),
    (
        "it-IT",
        include_str!("../locales/it-IT/sentinel-helper.ftl"),
    ),
    (
        "ja-JP",
        include_str!("../locales/ja-JP/sentinel-helper.ftl"),
    ),
    (
        "nl-NL",
        include_str!("../locales/nl-NL/sentinel-helper.ftl"),
    ),
    (
        "pl-PL",
        include_str!("../locales/pl-PL/sentinel-helper.ftl"),
    ),
    (
        "pt-BR",
        include_str!("../locales/pt-BR/sentinel-helper.ftl"),
    ),
    (
        "ru-RU",
        include_str!("../locales/ru-RU/sentinel-helper.ftl"),
    ),
    (
        "tr-TR",
        include_str!("../locales/tr-TR/sentinel-helper.ftl"),
    ),
    (
        "zh-CN",
        include_str!("../locales/zh-CN/sentinel-helper.ftl"),
    ),
];

static BUNDLE: OnceLock<FluentBundle<FluentResource>> = OnceLock::new();

/// Initialize the global bundle from the current process environment.
/// Idempotent; subsequent calls are no-ops. Must be called before any
/// [`t`] / [`t_args`].
pub fn init() {
    BUNDLE.get_or_init(build_bundle);
}

fn build_bundle() -> FluentBundle<FluentResource> {
    let chosen = negotiate_locale(env_locale().as_deref());
    let ftl_src = ftl_for(&chosen).unwrap_or_else(|| ftl_for(DEFAULT_LOCALE).expect("default ftl"));
    let res =
        FluentResource::try_new(ftl_src.to_string()).expect("embedded FTL must parse — bug if not");

    let langid: BundleLangId = chosen
        .parse()
        .unwrap_or_else(|_| DEFAULT_LOCALE.parse().expect("valid default locale"));
    let mut bundle = FluentBundle::new_concurrent(vec![langid]);
    // We render plain text in dialog widgets — no need for the
    // bidi-isolation marks fluent inserts around `{$arg}` by default.
    bundle.set_use_isolating(false);
    bundle
        .add_resource(res)
        .expect("embedded FTL must add cleanly");
    bundle
}

/// Look up `key` with no arguments. Falls back to the key itself if
/// missing — the user sees something diagnostic rather than nothing.
pub fn t(key: &str) -> String {
    let bundle = bundle();
    match bundle.get_message(key).and_then(|m| m.value()) {
        Some(pat) => {
            let mut errors = Vec::new();
            bundle.format_pattern(pat, None, &mut errors).into_owned()
        }
        None => key.to_string(),
    }
}

/// Look up `key` with a single integer arg. Used for the auto-deny
/// countdown; covers our only int-templated string today.
pub fn t_int(key: &str, arg_name: &str, value: i64) -> String {
    let mut args = FluentArgs::new();
    args.set(arg_name, FluentValue::from(value));
    format_with(key, Some(&args))
}

/// Look up `key` with a single string arg. Used by the
/// `dialog-message-default` template, which needs the requesting
/// process's name as `{$process}`.
pub fn t_str(key: &str, arg_name: &str, value: &str) -> String {
    let mut args = FluentArgs::new();
    args.set(arg_name, FluentValue::from(value.to_string()));
    format_with(key, Some(&args))
}

fn format_with(key: &str, args: Option<&FluentArgs<'_>>) -> String {
    let bundle = bundle();
    match bundle.get_message(key).and_then(|m| m.value()) {
        Some(pat) => {
            let mut errors = Vec::new();
            bundle.format_pattern(pat, args, &mut errors).into_owned()
        }
        None => key.to_string(),
    }
}

fn bundle() -> &'static FluentBundle<FluentResource> {
    BUNDLE.get().unwrap_or_else(|| {
        // init() should have been called from main, but be defensive
        // — a missing init would otherwise panic-crash the dialog.
        init();
        BUNDLE.get().expect("bundle initialized")
    })
}

fn env_locale() -> Option<String> {
    std::env::var("LC_ALL")
        .ok()
        .or_else(|| std::env::var("LC_MESSAGES").ok())
        .or_else(|| std::env::var("LANG").ok())
        .filter(|s| !s.is_empty())
}

fn negotiate_locale(raw: Option<&str>) -> String {
    let Some(raw) = raw else {
        return DEFAULT_LOCALE.to_string();
    };
    let Some(canon) = canonicalize_locale(raw) else {
        return DEFAULT_LOCALE.to_string();
    };

    let requested: Vec<NegLangId> = canon.parse().ok().into_iter().collect();
    let available: Vec<NegLangId> = BUNDLES
        .iter()
        .filter_map(|(tag, _)| tag.parse().ok())
        .collect();
    let default: NegLangId = DEFAULT_LOCALE.parse().expect("valid default");

    // fluent_langneg wants slices whose element type implements
    // AsRef<LanguageIdentifier>; the owned type itself doesn't, so we
    // collect refs.
    let req_refs: Vec<&NegLangId> = requested.iter().collect();
    let avail_refs: Vec<&NegLangId> = available.iter().collect();
    let default_ref: &NegLangId = &default;
    let negotiated = negotiate_languages(
        &req_refs,
        &avail_refs,
        Some(&default_ref),
        NegotiationStrategy::Filtering,
    );
    negotiated
        .first()
        .map(|l| l.to_string())
        .unwrap_or_else(|| DEFAULT_LOCALE.to_string())
}

/// POSIX locale → BCP-47, with strict validation.
///
/// Examples: `tr_TR.UTF-8@modifier` → `tr-TR`, `en` → `en`,
/// `C` / `POSIX` / malformed → `None` (caller falls back to default).
fn canonicalize_locale(raw: &str) -> Option<String> {
    // Strip `.charset` and `@modifier` suffixes.
    let stem: String = raw.chars().take_while(|c| *c != '.' && *c != '@').collect();
    if stem.is_empty() {
        return None;
    }
    // POSIX `C` and `POSIX` are not real locales.
    if stem == "C" || stem == "POSIX" {
        return None;
    }
    // POSIX uses `_` between language and region; BCP-47 uses `-`.
    let bcp47 = stem.replace('_', "-");
    // Defensive bound + character whitelist. fluent's parser would
    // reject malformed input anyway, but we don't want a 4 KB
    // attacker-controlled string flowing into FluentBundle.
    if bcp47.len() > 16 || !bcp47.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return None;
    }
    Some(bcp47)
}

fn ftl_for(locale: &str) -> Option<&'static str> {
    BUNDLES
        .iter()
        .find(|(tag, _)| *tag == locale)
        .map(|(_, ftl)| *ftl)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- canonicalize_locale ---------------------------------------------

    #[test]
    fn canonicalize_strips_charset() {
        assert_eq!(
            canonicalize_locale("tr_TR.UTF-8"),
            Some("tr-TR".to_string())
        );
    }

    #[test]
    fn canonicalize_strips_modifier() {
        assert_eq!(canonicalize_locale("de_DE@euro"), Some("de-DE".to_string()));
    }

    #[test]
    fn canonicalize_handles_combined_suffixes() {
        assert_eq!(
            canonicalize_locale("sr_RS.UTF-8@latin"),
            Some("sr-RS".to_string())
        );
    }

    #[test]
    fn canonicalize_passes_through_simple() {
        assert_eq!(canonicalize_locale("en"), Some("en".to_string()));
    }

    #[test]
    fn canonicalize_rejects_c_locale() {
        assert_eq!(canonicalize_locale("C"), None);
        assert_eq!(canonicalize_locale("POSIX"), None);
    }

    #[test]
    fn canonicalize_rejects_overlong() {
        // 17 chars after stripping is too long.
        let s = "a".repeat(17);
        assert_eq!(canonicalize_locale(&s), None);
    }

    #[test]
    fn canonicalize_rejects_path_traversal() {
        // The validator must reject anything that's not [A-Za-z0-9-].
        // We never USE the locale as a path component, but defense in
        // depth — this means even a malicious LANG can't sneak past.
        assert_eq!(canonicalize_locale("../etc/shadow"), None);
        assert_eq!(canonicalize_locale("tr_TR/../oops"), None);
        assert_eq!(canonicalize_locale("en\u{0000}injected"), None);
    }

    #[test]
    fn canonicalize_rejects_empty() {
        assert_eq!(canonicalize_locale(""), None);
        assert_eq!(canonicalize_locale(".UTF-8"), None);
    }

    // ---- negotiation -----------------------------------------------------

    #[test]
    fn negotiates_exact_match() {
        assert_eq!(negotiate_locale(Some("tr-TR")), "tr-TR");
    }

    #[test]
    fn negotiates_posix_format() {
        assert_eq!(negotiate_locale(Some("tr_TR.UTF-8")), "tr-TR");
    }

    #[test]
    fn unknown_locale_falls_back_to_default() {
        assert_eq!(negotiate_locale(Some("xx-YY")), DEFAULT_LOCALE);
    }

    #[test]
    fn missing_lang_falls_back_to_default() {
        assert_eq!(negotiate_locale(None), DEFAULT_LOCALE);
    }

    #[test]
    fn malicious_lang_falls_back_to_default() {
        assert_eq!(negotiate_locale(Some("../../etc/shadow")), DEFAULT_LOCALE);
    }

    // ---- bundle lookups --------------------------------------------------
    //
    // These touch the global BUNDLE; first call wins. For test
    // determinism we only assert lookup behavior in the bundle that
    // initialized first (whatever LANG was in the test runner's env),
    // by going through the public API.

    #[test]
    fn missing_key_returns_key_as_fallback() {
        init();
        assert_eq!(
            t("definitely-not-a-real-key-1234"),
            "definitely-not-a-real-key-1234"
        );
    }

    #[test]
    fn t_int_substitutes_argument() {
        init();
        // Whatever bundle is active, `auto-deny-in` MUST contain `{$seconds}`
        // and the rendered output must contain the number we passed.
        let out = t_int("auto-deny-in", "seconds", 7);
        assert!(out.contains('7'), "expected '7' in {out:?}");
    }

    // ---- FTL parses cleanly ----------------------------------------------

    #[test]
    fn every_embedded_bundle_parses() {
        // Catches typos in any locale's .ftl file at test time rather
        // than waiting for a user to set that LANG and crash.
        for (tag, src) in BUNDLES {
            FluentResource::try_new(src.to_string())
                .unwrap_or_else(|_| panic!("bundle {tag} fails to parse"));
        }
    }

    #[test]
    fn every_bundle_has_required_keys() {
        // The set of keys the helper expects to look up. Adding a new
        // string in app.rs without translating it should fail this test.
        const REQUIRED_KEYS: &[&str] = &[
            "dialog-title-default",
            "dialog-message-default",
            // `dialog-secondary-default` was removed in v0.6.0 — the
            // built-in default secondary line is empty (the helper
            // renders an admin-set secondary verbatim if non-empty).
            "button-allow",
            "button-deny",
            "toggle-show-details",
            "toggle-hide-details",
            "auto-deny-in",
            "detail-command",
            "detail-pid",
            "detail-cwd",
            "detail-requested-by",
            "detail-action",
            "truncated-suffix",
        ];
        for (tag, src) in BUNDLES {
            let res = FluentResource::try_new(src.to_string()).expect("parses");
            let langid: BundleLangId = tag.parse().expect("valid tag");
            let mut bundle = FluentBundle::new_concurrent(vec![langid]);
            bundle.add_resource(res).expect("adds");
            for key in REQUIRED_KEYS {
                assert!(
                    bundle.get_message(key).is_some(),
                    "bundle {tag} is missing key {key}"
                );
            }
        }
    }

    /// Pull the raw value (right-hand side) for a key out of an FTL
    /// source. Our FTLs have one line per key, no continuation —
    /// trivial linear scan.
    fn ftl_value_for<'a>(src: &'a str, key: &str) -> Option<&'a str> {
        for line in src.lines() {
            let line = line.trim_end();
            // Match "key " or "key=" — keys can have trailing whitespace
            // before the `=` per FTL syntax.
            let Some(rest) = line.strip_prefix(key) else {
                continue;
            };
            let rest = rest.trim_start();
            if let Some(value) = rest.strip_prefix('=') {
                return Some(value.trim_start());
            }
        }
        None
    }

    /// Extract the set of `{$name}` placeholders from a fluent value.
    /// Doesn't try to handle nested expressions or selectors — our
    /// values are single-placeholder simple substitutions.
    fn placeholders(value: &str) -> std::collections::BTreeSet<String> {
        let mut out = std::collections::BTreeSet::new();
        let bytes = value.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'{' && i + 1 < bytes.len() && bytes[i + 1] == b'$' {
                let start = i + 2;
                let mut end = start;
                while end < bytes.len() && bytes[end] != b'}' {
                    end += 1;
                }
                if end < bytes.len() {
                    if let Ok(name) = std::str::from_utf8(&bytes[start..end]) {
                        out.insert(name.trim().to_string());
                    }
                }
                i = end + 1;
            } else {
                i += 1;
            }
        }
        out
    }

    #[test]
    fn every_bundle_has_matching_placeholders() {
        // For every key, every locale's value must contain the same
        // `{$arg}` set as en-US. Catches a translator who renamed
        // `{$seconds}` to `{$secondes}` etc. — without this, runtime
        // substitution fails silently.
        let en_src = ftl_for("en-US").expect("en-US present");
        let mut keys: Vec<&str> = en_src
            .lines()
            .filter_map(|l| {
                let l = l.trim_start();
                if l.starts_with('#') || l.is_empty() {
                    return None;
                }
                l.split_once('=').map(|(k, _)| k.trim())
            })
            .collect();
        keys.sort();
        keys.dedup();

        for key in keys {
            let en_value = ftl_value_for(en_src, key).expect("en-US value");
            let en_phs = placeholders(en_value);
            for (tag, src) in BUNDLES {
                if *tag == "en-US" {
                    continue;
                }
                let Some(value) = ftl_value_for(src, key) else {
                    continue;
                };
                let phs = placeholders(value);
                assert_eq!(
                    en_phs, phs,
                    "bundle {tag}: placeholder mismatch on key {key} \
                     (en-US has {en_phs:?}, {tag} has {phs:?})"
                );
            }
        }
    }
}
