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
| `remember_seconds` | uint | `0` | "Remember" window. After an Allow, repeat requests from the **same login session** for the **same service + binary** auto-allow without a dialog for this many seconds. `0` (default) disables it; hard-capped at `900`. See [`[general].remember_seconds`](#remember-window) below. |

<a id="remember-window"></a>
**The remember window** is a `sudo`-timestamp analogue. A grant is bound
to your `loginuid` **and** kernel audit `sessionid`, so it can't be
replayed in another session or by another user, and is scoped to the
exact `(service, exe)` it was granted for — never a blanket allow. It is
enforced by two trust-appropriate backends:

- **sudo / su** (PAM module, root): a record in `/run/sentinel/ts`, a
  root-owned `0700` tmpfs dir. Freshness uses `CLOCK_BOOTTIME` stored in
  the record (so moving the wall clock can't extend it), and tmpfs is
  wiped on reboot, so no grant survives a reboot.
- **polkit / GUI** (agent, per-user): an in-memory cache that evaporates
  on logout (the agent restarts with the session).

A request with no audit session is never remembered. `sudo`'s own
timestamp still covers terminal `sudo` independently of this setting.

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

### `[policy]`

Static allow/deny lists evaluated **before** the dialog. Disabled by
default (empty lists never match), so behaviour is unchanged until you
opt in.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `allow` | list of string | `[]` | Auto-allow (no dialog) when an entry matches. |
| `deny` | list of string | `[]` | Auto-deny (no dialog). **Takes precedence over `allow`.** |

Each entry matches the requesting program's **resolved executable path**
(`/proc/<pid>/exe`, e.g. `/usr/bin/pacman` — never the spoofable
`argv[0]`), that path's **basename** when the entry contains no `/`, or
the **polkit action id** (agent path).

> ⚠️ An `allow` entry is **passwordless elevation** for that target — as
> load-bearing as a `sudoers` `NOPASSWD` line. Prefer absolute paths,
> keep the list short.

```toml
[policy]
allow = [
    "/usr/bin/topgrade",                         # full path (recommended)
    "pacman",                                    # basename: any path named 'pacman'
]
deny = [
    "org.freedesktop.systemd1.manage-units",     # polkit action id
]
```

### `[notifications]`

Desktop notifications (via `notify-send` / libnotify) on the polkit/GUI
auth path. Terminal `sudo`/`su` denials are already visible in the
terminal, so they're not covered. Both default off.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `on_deny` | bool | `false` | Notify when a request is denied (including silent `[policy]` denials, where no dialog appeared). |
| `on_timeout` | bool | `false` | Notify when a request auto-denies on timeout. |

```toml
[notifications]
on_deny = true
on_timeout = true
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
remember_seconds = 0          # 0 = ask every time; e.g. 300 = remember for 5 min

[appearance]
title = "Authentication Required"
message = 'The application "%p" is requesting elevated privileges.'
secondary = ""

[audio]
sound_name = "dialog-warning"

# Auto-allow/deny before the dialog (off by default — empty lists).
[policy]
allow = []
deny = []

# Desktop notification on deny/timeout (polkit/GUI path).
[notifications]
on_deny = false
on_timeout = false

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
in Ns") is localized from the system locale (`LC_ALL`/`LC_MESSAGES`/`LANG`).

- The **COSMIC** helper (`sentinel-helper`) ships 12 locales via fluent:
  en-US, de-DE, es-ES, fr-FR, it-IT, ja-JP, nl-NL, pl-PL, pt-BR, ru-RU,
  tr-TR, zh-CN.
- The **KDE** helper (`sentinel-helper-kde`) localizes the same chrome via
  `sentinel_shared::ui_i18n` — currently en, de, es, extensible to the
  rest of the set.

The dialog message/title/secondary are admin-supplied via this config
file — they're rendered verbatim as you write them. If you leave the
defaults (`title`, `message`), the helper substitutes the locale's
own translation; once you customise them, your text wins.

Locale resolution: `LC_ALL` → `LC_MESSAGES` → `LANG`, with the helper
canonicalising to BCP-47 (`tr_TR.UTF-8` → `tr-TR`) and falling back to
`en-US` for unknown values.
