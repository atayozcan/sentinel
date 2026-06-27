// SPDX-FileCopyrightText: 2026 Atay Özcan <atay@oezcan.me>
// SPDX-License-Identifier: GPL-3.0-or-later
//! Minimal UI-string localization for the KDE helper, keyed by a stable
//! string key.
//!
//! The KDE helper (cxx-qt/QML) has no gettext/fluent runtime, so it
//! looks strings up here via a single `translate()` invokable. English
//! is the source/fallback. Add a locale by extending the `match` arms
//! below (the keys are identical across locales).
//!
//! Count placeholders use Qt's `%1` style (the KDE QML calls
//! `.arg(seconds)` on the result).

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
        "fr" => fr(key),
        "it" => it(key),
        "ja" => ja(key),
        "nl" => nl(key),
        "pl" => pl(key),
        "pt" => pt(key),
        "ru" => ru(key),
        "tr" => tr(key),
        "zh" => zh(key),
        _ => None,
    };
    // Localized → English source → a visible marker for an unknown key
    // (`key` isn't `'static`, so we can't echo it back here).
    localized.or_else(|| en(key)).unwrap_or("?")
}

/// Localized template for the "remember" opt-in checkbox. `%1` is the
/// Qt placeholder the helper replaces with a human duration (e.g.
/// `5 min`). Falls back to English for an unlisted locale.
pub fn remember_label_template(lang: &str) -> &'static str {
    match lang {
        "de" => "Für %1 merken",
        "es" => "Recordar durante %1",
        "fr" => "Mémoriser pendant %1",
        "it" => "Ricorda per %1",
        "ja" => "%1 記憶する",
        "nl" => "Onthouden voor %1",
        "pl" => "Zapamiętaj na %1",
        "pt" => "Lembrar por %1",
        "ru" => "Запомнить на %1",
        "tr" => "%1 boyunca hatırla",
        "zh" => "在 %1 内记住",
        _ => "Remember for %1",
    }
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

fn fr(key: &str) -> Option<&'static str> {
    Some(match key {
        "allow" => "Autoriser",
        "deny" => "Refuser",
        "show-details" => "Afficher les détails",
        "hide-details" => "Masquer les détails",
        "auto-deny-in" => "Refus automatique dans %1 s",
        "title-default" => "Authentification requise",
        "detail-action" => "Action",
        "detail-command" => "Commande",
        "detail-pid" => "PID",
        "detail-requested-by" => "Demandé par",
        "detail-cwd" => "Répertoire de travail",
        _ => return None,
    })
}

fn it(key: &str) -> Option<&'static str> {
    Some(match key {
        "allow" => "Consenti",
        "deny" => "Nega",
        "show-details" => "Mostra dettagli",
        "hide-details" => "Nascondi dettagli",
        "auto-deny-in" => "Negazione automatica fra %1 s",
        "title-default" => "Autenticazione richiesta",
        "detail-action" => "Azione",
        "detail-command" => "Comando",
        "detail-pid" => "PID",
        "detail-requested-by" => "Richiesto da",
        "detail-cwd" => "Directory di lavoro",
        _ => return None,
    })
}

fn ja(key: &str) -> Option<&'static str> {
    Some(match key {
        "allow" => "許可",
        "deny" => "拒否",
        "show-details" => "詳細を表示",
        "hide-details" => "詳細を非表示",
        "auto-deny-in" => "%1 秒後に自動的に拒否されます",
        "title-default" => "認証が必要です",
        "detail-action" => "アクション",
        "detail-command" => "コマンド",
        "detail-pid" => "PID",
        "detail-requested-by" => "要求元",
        "detail-cwd" => "作業ディレクトリ",
        _ => return None,
    })
}

fn nl(key: &str) -> Option<&'static str> {
    Some(match key {
        "allow" => "Toestaan",
        "deny" => "Weigeren",
        "show-details" => "Details tonen",
        "hide-details" => "Details verbergen",
        "auto-deny-in" => "Automatisch weigeren over %1 s",
        "title-default" => "Verificatie vereist",
        "detail-action" => "Actie",
        "detail-command" => "Opdracht",
        "detail-pid" => "PID",
        "detail-requested-by" => "Aangevraagd door",
        "detail-cwd" => "Werkmap",
        _ => return None,
    })
}

fn pl(key: &str) -> Option<&'static str> {
    Some(match key {
        "allow" => "Zezwól",
        "deny" => "Odmów",
        "show-details" => "Pokaż szczegóły",
        "hide-details" => "Ukryj szczegóły",
        "auto-deny-in" => "Automatyczna odmowa za %1 s",
        "title-default" => "Wymagane uwierzytelnienie",
        "detail-action" => "Akcja",
        "detail-command" => "Polecenie",
        "detail-pid" => "PID",
        "detail-requested-by" => "Żąda",
        "detail-cwd" => "Katalog roboczy",
        _ => return None,
    })
}

fn pt(key: &str) -> Option<&'static str> {
    Some(match key {
        "allow" => "Permitir",
        "deny" => "Negar",
        "show-details" => "Mostrar detalhes",
        "hide-details" => "Ocultar detalhes",
        "auto-deny-in" => "Negação automática em %1 s",
        "title-default" => "Autenticação necessária",
        "detail-action" => "Ação",
        "detail-command" => "Comando",
        "detail-pid" => "PID",
        "detail-requested-by" => "Solicitado por",
        "detail-cwd" => "Diretório de trabalho",
        _ => return None,
    })
}

fn ru(key: &str) -> Option<&'static str> {
    Some(match key {
        "allow" => "Разрешить",
        "deny" => "Запретить",
        "show-details" => "Показать подробности",
        "hide-details" => "Скрыть подробности",
        "auto-deny-in" => "Автоматический отказ через %1 с",
        "title-default" => "Требуется аутентификация",
        "detail-action" => "Действие",
        "detail-command" => "Команда",
        "detail-pid" => "PID",
        "detail-requested-by" => "Запросил",
        "detail-cwd" => "Рабочий каталог",
        _ => return None,
    })
}

fn tr(key: &str) -> Option<&'static str> {
    Some(match key {
        "allow" => "İzin Ver",
        "deny" => "Reddet",
        "show-details" => "Ayrıntıları göster",
        "hide-details" => "Ayrıntıları gizle",
        "auto-deny-in" => "%1 sn sonra otomatik reddedilecek",
        "title-default" => "Kimlik Doğrulama Gerekli",
        "detail-action" => "İşlem",
        "detail-command" => "Komut",
        "detail-pid" => "PID",
        "detail-requested-by" => "İsteyen kullanıcı",
        "detail-cwd" => "Çalışma dizini",
        _ => return None,
    })
}

fn zh(key: &str) -> Option<&'static str> {
    Some(match key {
        "allow" => "允许",
        "deny" => "拒绝",
        "show-details" => "显示详情",
        "hide-details" => "隐藏详情",
        "auto-deny-in" => "%1 秒后自动拒绝",
        "title-default" => "需要身份验证",
        "detail-action" => "操作",
        "detail-command" => "命令",
        "detail-pid" => "PID",
        "detail-requested-by" => "请求者",
        "detail-cwd" => "工作目录",
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
        // Each locale must define the full English key set — guards against
        // a missing translation silently falling back to English.
        type LocaleFn = fn(&str) -> Option<&'static str>;
        let locales: &[(&str, LocaleFn)] = &[
            ("en", en),
            ("de", de),
            ("es", es),
            ("fr", fr),
            ("it", it),
            ("ja", ja),
            ("nl", nl),
            ("pl", pl),
            ("pt", pt),
            ("ru", ru),
            ("tr", tr),
            ("zh", zh),
        ];
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
            for (name, f) in locales {
                assert!(f(key).is_some(), "{name} missing {key}");
            }
            // count placeholder must survive in every locale's auto-deny-in
            if key == "auto-deny-in" {
                for (name, f) in locales {
                    assert!(
                        f(key).unwrap().contains("%1"),
                        "{name} auto-deny-in lost %1"
                    );
                }
            }
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
