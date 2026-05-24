# Sentinel-KDE

[![License: GPL-3.0-or-later](https://img.shields.io/badge/License-GPL--3.0--or--later-blue.svg)](LICENSE)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-blue.svg)](rust-toolchain.toml)

A Windows-UAC-style confirmation dialog for privilege escalation on **KDE
Plasma** (Wayland). When something asks for elevated rights — `pkexec`, a
polkit action, `sudo` — Sentinel paints a native Breeze/Kirigami dialog
asking you to **Allow** or **Deny**, instead of prompting for a password.

A PAM module (`pam_sentinel.so`), a polkit authentication agent, and a
Qt/QML helper, all in Rust ([cxx-qt](https://github.com/KDAB/cxx-qt) for
the GUI). This is the KDE-native sibling of
[`sentinel`](https://github.com/atayozcan/sentinel) (which targets COSMIC
via libcosmic).

> [!CAUTION]
> Sentinel sits in the **PAM authentication path**. Misconfiguration can
> break `pkexec`/polkit auth. The default install touches **polkit only**
> (`/etc/pam.d/polkit-1`), not `sudo`, and uses `auth sufficient … include
> system-auth`, so a broken module falls back to a password — `sudo` stays
> a recovery path. Still: keep a root shell open (`sudo -i`) during your
> first install until you've confirmed things work.
>
> **Provided as-is, without warranty** (GPL-3.0 §§15–16). Use at your own
> risk.

## How it works

- **`pam_sentinel.so`** — wired into `/etc/pam.d/polkit-1` (and optionally
  `sudo`). On an auth it either fast-paths a pre-approval from the agent
  (no password) or, on failure, falls through to the normal password stack.
- **`sentinel-polkit-agent`** — runs as a **systemd user service**
  (`PartOf=graphical-session.target`), registers with polkitd as the
  session's authentication agent, shows the dialog, and pre-approves the
  auth over a per-user socket (`$XDG_RUNTIME_DIR/sentinel-agent.sock`).
- **`sentinel-helper-kde`** — the Breeze/Kirigami dialog, painted as a
  `zwlr-layer-shell-v1` overlay (KWin) with a windowed fallback. QML is
  embedded (qrc); untrusted process strings are plain-text + length-clipped.

The installer makes Sentinel the session's **sole** polkit agent by masking
`plasma-polkit-agent.service`, and installs a polkit rule making the
install user a polkit **administrator** — Sentinel's no-password model is
UAC-style: *you* confirm *your own* escalation. `root` stays an admin too.
Both are reversed by the uninstaller.

## Requirements (openSUSE Tumbleweed / KDE Plasma 6)

- Rust ≥ 1.85 (`rustup`), `pam-devel`, Qt 6 + KDE Frameworks 6 runtime
  (Kirigami, LayerShellQt, qqc2-desktop-style — already present on Plasma).
- Wayland session (KWin). X11 is not supported.

## Install (from source)

```bash
sudo zypper install rustup pam-devel
rustup default 1.85
sudo ./install.sh            # builds + wires into the polkit auth path
                            # add --enable-sudo to also cover sudo
# then verify:
pkexec true                 # one Sentinel dialog; Allow → no password
```

Remove with `sudo ./uninstall.sh` (restores polkit-kde, removes the rule,
reverts every file from `/var/lib/sentinel/install.state`).

## Standalone dialog (no install)

```bash
cargo build --release -p sentinel-helper-kde
./target/release/sentinel-helper-kde --title "Authentication Required" \
  --message 'pacman wants privileges' --process-exe /usr/bin/pacman \
  --timeout 30 --min-time 800 --randomize
# prints ALLOW / DENY / TIMEOUT, exits 0 / 1
```

## Layout

```
crates/
├── sentinel-shared/        # config schema, Outcome wire enum, CLI parser, /proc + logind readers
├── pam-sentinel/           # cdylib → pam_sentinel.so
├── sentinel-polkit-agent/  # bin → the polkit agent (systemd user service)
└── sentinel-helper-kde/    # bin → the Kirigami dialog (Rust + cxx-qt)
packaging/systemd/user/      # sentinel-polkit-agent.service
scripts/test-install-*.sh    # podman-based install/uninstall tests
```

## License

**GPL-3.0-or-later.** See [LICENSE](LICENSE).
