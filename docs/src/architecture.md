# Architecture

Sentinel is three binaries plus one shared crate, in a single Cargo
workspace.

```
crates/
├── sentinel-shared/        # config schema, /proc + logind readers,
│                           # Outcome wire enum, log_kv helpers,
│                           # POLKIT_PAM_SERVICE const, audit::init_syslog
├── pam-sentinel/           # cdylib → /usr/lib/security/pam_sentinel.so
├── sentinel-helper/        # bin → /usr/lib/sentinel-helper (libcosmic dialog)
└── sentinel-polkit-agent/  # bin → /usr/lib/sentinel-polkit-agent
```

## The PAM module — `pam_sentinel.so`

Loaded by libpam on every authentication attempt for whatever
services have it wired in. For each call it picks one of:

- **Bypass:** the polkit agent has already pre-approved this auth
  via the local socket. Return `PAM_SUCCESS` immediately.
- **Dialog:** spawn `sentinel-helper`, wait for Allow / Deny / timeout.
  Return `PAM_SUCCESS` on Allow, `PAM_AUTH_ERR` otherwise.
- **Headless:** no Wayland display detected. Return whatever
  `headless_action` says (default `PAM_IGNORE` → password prompt).
- **Disabled:** `enabled = false` in config → `PAM_IGNORE`.

Identifying the requesting user uses `/proc/<ppid>/loginuid` (set by
PAM at login, inherited through forks, immune to setuid). Falls back
to `/proc/<ppid>/status` `Uid:` line, then `getuid()`.

The displayed process name uses `/proc/<pid>/cmdline` of the
privileged binary (sudo, pkexec, helper-1) and strips the elevation
wrapper via `sentinel_shared::strip_elevation_prefix`. For wrappers
with no target argv (`sudo -v` for cred-cache), it walks `PPid` to
the calling process so the dialog shows the user-facing originator
(`paru`, `topgrade`) rather than `sudo-rs`.

## The polkit agent — `sentinel-polkit-agent`

A per-user agent that registers with polkitd as the session's
`org.freedesktop.PolicyKit1.AuthenticationAgent`. Forks
`sentinel-helper` for the dialog, then satisfies polkit's cookie
validation via `polkit-agent-helper-1` over its socket.

### Bypass socket

`$XDG_RUNTIME_DIR/sentinel-agent.sock` (mode `0600`, owned by the
user). When the agent's own helper-1 invocation runs, the
`pam_sentinel.so` inside it connects here, gets a one-shot
"OK" / "NO" response, and short-circuits to `PAM_SUCCESS` without
spawning a second dialog.

Per-connection check:
1. `SO_PEERCRED` — peer uid must be 0 (helper-1 runs as root).
2. `/proc/<peer-pid>/comm` must equal `polkit-agent-helper-1` or
   its kernel-truncated form `polkit-agent-he` (TASK_COMM_LEN = 16).

Approvals are one-shot, expire after 1 second, and `cancel-authentication`
drains the queue so a stale approval can't be picked up by a
racing auth.

### Identity selection

`unix-user` identities are preferred over groups; the matching uid
wins over alternatives; first non-root unix-user is the fallback.
See `crates/sentinel-polkit-agent/src/identity.rs`.

### Why XDG autostart, not systemd-user

The agent must inherit the kernel sessionid of the user's compositor.
A `systemd --user` unit would run under `user@<uid>.service` (a
DIFFERENT sessionid), and polkit's `RegisterAuthenticationAgent`
rejects the mismatch with "Passed session and the session the caller
is in differs". Sentinel's autostart entry sets
`X-systemd-skip=true` so the systemd xdg-autostart-generator doesn't
wrap it.

## The helper — `sentinel-helper`

A libcosmic GUI binary that paints the dialog. Per-spawn:

- Initializes the global Fluent translation bundle from `LANG` /
  `LC_*` (locales embedded at compile time).
- Plays the freedesktop sound cue via `canberra-gtk-play` (silent
  fallback if not installed).
- Decides layer-shell vs xdg-toplevel rendering (auto-falls-back to
  xdg-toplevel on Mutter-based desktops).
- Renders the card; emits `ALLOW` / `DENY` / `TIMEOUT` on stdout
  and exits with the matching code.

Keyboard accessibility:
- Tab / Shift+Tab — cycle Allow / Deny (iced default).
- Enter / Space — activate focused button.
- Escape — always denies (intercepted regardless of focus).
- Allow button is disabled for `min_display_time_ms` after the
  dialog appears, blocking instant scripted clicks.

## Wire formats

### Helper → caller

The helper writes one of `ALLOW\n`, `DENY\n`, `TIMEOUT\n` to stdout
and exits with `0` (Allow) or `1` (Deny / Timeout). The
`sentinel_shared::Outcome` enum is the single source of truth for
the parser.

### Audit log

Lines emitted under syslog identifier `pam_sentinel` or
`sentinel-polkit-agent`, AUTH facility:

```
event=auth.allow source=dialog user=alice service=sudo process=pacman uid=1000 latency_ms=2891 session_type=wayland session_class=user session_remote=0
event=auth.allow source=bypass uid=1000
event=auth.deny  source=dialog user=alice service=sudo process=true uid=1000 latency_ms=12440 …
event=auth.timeout source=agent user=alice action=org.freedesktop.policykit.exec process=pacman …
event=auth.headless reason=no-wayland user=alice service=sudo …
```

Format is logfmt (whitespace-separated `key=value`, values quoted
when necessary). Designed for `journalctl -t pam_sentinel
--output=cat | grep event=auth.deny` to be the SRE-friendly query.

### Bypass socket

ASCII protocol, length-bounded:

```
client → server: ?\n
server → client: OK\n     (approval popped, fast-path the auth)
                  or
                 NO\n     (no approval; client falls through to dialog)
```

## Compatibility matrix

See [README#Compatibility](https://github.com/atayozcan/sentinel#compatibility).
The agent's autostart entry uses `NotShowIn=` to exclude desktops
with built-in polkit agents (GNOME, KDE, XFCE, LXDE, Cinnamon, MATE,
LXQt, Pantheon, Budgie) and lets every other compositor pick it up
automatically.

## Threat model

See [Security policy](./security.md) for the explicit trust
boundaries — what the PAM module trusts vs. doesn't, what the agent
will refuse, supply-chain integrity via Sigstore attestations.
