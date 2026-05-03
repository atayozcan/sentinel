# Configuration

Sentinel reads `/etc/security/sentinel.conf` (TOML) on every PAM
auth attempt — no daemon to reload. The file is **root-owned and
not user-writable on purpose**: a per-user override layer would
defeat the UAC contract by letting an unprivileged user lower their
own `timeout` to zero.

## Sections

### `[general]`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `enabled` | bool | `true` | Master switch. When `false`, the module returns `PAM_IGNORE` and the rest of the stack runs unchanged. |
| `timeout` | uint | `30` | Auto-deny timeout in seconds. `0` disables the timeout (the dialog stays open until the user clicks). |
| `randomize_buttons` | bool | `true` | Swap Allow/Deny positions randomly to deter scripted clickers. |
| `headless_action` | enum | `"password"` | What to do when no Wayland display is available. `"allow"` silently grants (DANGEROUS), `"deny"` silently rejects, `"password"` falls through to the next PAM module (typically `pam_unix`). |
| `show_process_info` | bool | `true` | Display the requesting process's exe/cmdline in the dialog. |
| `log_attempts` | bool | `true` | Log every allow/deny/timeout to syslog (`auth.info`). |
| `min_display_time_ms` | uint | `500` | Disable the Allow button for this many ms after the dialog appears, blocking instant scripted clicks. |

### `[appearance]`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `title` | string | `"Authentication Required"` | Dialog title. No substitutions. |
| `message` | string | `'The application "%p" is requesting elevated privileges.'` | Primary message. Tokens: see below. |
| `secondary` | string | `""` | Optional hint line below the message. Empty by default — naming the buttons in the secondary text broke under `randomize_buttons` in 0.5.x. |

### `[audio]`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `sound_name` | string | `"dialog-warning"` | Freedesktop sound name (NOT a file path). Empty string disables. Resolved via `canberra-gtk-play` if installed. |

### `[services.<name>]`

Per-PAM-service overrides. Any `[general]` key can be overridden under
`[services.sudo]`, `[services."polkit-1"]`, `[services.su]`, etc.
Omitted keys inherit from `[general]`.

```toml
[services.polkit-1]
timeout = 60          # more lenient for GUI auth

[services.su]
enabled = false       # never confirm `su`, fall through to password
```

## Tokens

Inside `message` and `secondary` the following expand at runtime:

| Token | Expands to |
|-------|------------|
| `%u` | Username being authenticated |
| `%s` | PAM service name (`sudo`, `polkit-1`, …) |
| `%p` | Requesting process's executable path basename |
| `%%` | Literal `%` |

Unknown `%x` sequences are preserved verbatim so a typo is visible to
the admin in the rendered dialog.

## Example

```toml
# /etc/security/sentinel.conf

[general]
enabled = true
timeout = 30
randomize_buttons = true
headless_action = "password"
min_display_time_ms = 500

[appearance]
title = "Authentication Required"
message = 'The application "%p" is requesting elevated privileges.'
secondary = ""

[audio]
sound_name = "dialog-warning"

[services.sudo]
timeout = 30

[services."polkit-1"]
timeout = 60

[services.su]
enabled = false

[services.gdm-password]
enabled = false

[services.lightdm]
enabled = false

[services.sddm]
enabled = false
```

## Localization

The helper's UI chrome (button labels, "Show details" toggle, "Auto-deny
in Ns") is translated into 12 locales: en-US, de-DE, es-ES, fr-FR,
it-IT, ja-JP, nl-NL, pl-PL, pt-BR, ru-RU, tr-TR, zh-CN.

The dialog message/title/secondary are admin-supplied via this config
file — they're rendered verbatim as you write them. If you leave the
defaults (`title`, `message`), the helper substitutes the locale's
own translation; once you customise them, your text wins.

Locale resolution: `LC_ALL` → `LC_MESSAGES` → `LANG`, with the helper
canonicalising to BCP-47 (`tr_TR.UTF-8` → `tr-TR`) and falling back to
`en-US` for unknown values.
