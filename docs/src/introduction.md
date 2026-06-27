# Sentinel

A Windows UAC-style confirmation dialog for Linux privilege escalation,
delivered as a shared PAM + polkit-agent backend plus a native KDE
Plasma (Kirigami) desktop frontend, `sentinel-helper-kde`. Wayland-only,
`sudo-rs` friendly; the layer-shell dialog renders on KWin/Wayland and
other wlroots-style compositors (Hyprland, Sway, Niri, River, Wayfire).

## What it does

When a privileged binary's PAM stack hits `pam_sentinel.so` (typically
`/etc/pam.d/polkit-1` and optionally `/etc/pam.d/sudo`), the polkit
agent spawns the frontend helper, `sentinel-helper-kde`. The helper
paints a `zwlr-layer-shell-v1` overlay surface — full-screen translucent
backdrop, exclusive keyboard focus, dialog card centered — and waits for
**Allow**, **Deny**, or a configurable timeout (auto-deny).

- **Allow** → PAM passes auth without a password.
- **Deny / timeout / no Wayland display** → PAM continues to the next
  module (typically `pam_unix`, the password prompt).

Sentinel also ships `sentinel-polkit-agent`, a per-user polkit
authentication agent that registers with the session and forwards
polkit-mediated auth requests through the same Allow/Deny dialog. Its
one-shot pre-approval reaches `pam_sentinel.so` over the system D-Bus
(`org.sentinel.Agent`), which keeps the bypass working under SELinux.

## Threat model & where to start

Sentinel sits in the **PAM authentication path**. A misconfiguration
can lock you out of `sudo`, polkit, or login. Read the
[Troubleshooting](./troubleshooting.md) page **before** you install,
and open a second root shell during the first install (`pkexec bash`)
until you've verified `sudo` still works.

For the security model, see [Architecture](./architecture.md) and
[Security policy](./security.md).

## Where to read next

- **First install:** [Installation](./installation.md)
- **Customize the dialog:** [Configuration](./configuration.md)
- **Wire into sudo / su:** [PAM wiring](./pam-wiring.md)
- **Something broke:** [Troubleshooting](./troubleshooting.md)
- **Curious about the design:** [Architecture](./architecture.md)
- **Want to contribute:** [Contributing](./contributing.md)
