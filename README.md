# Sentinel-KDE

[![CI](https://github.com/atayozcan/sentinel-kde/actions/workflows/ci.yml/badge.svg)](https://github.com/atayozcan/sentinel-kde/actions/workflows/ci.yml)
[![License: GPL-3.0-or-later](https://img.shields.io/badge/License-GPL--3.0--or--later-blue.svg)](LICENSE)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-blue.svg)](rust-toolchain.toml)

A Windows-UAC-style confirmation dialog for privilege escalation on **KDE
Plasma** (Wayland). When something asks for elevated rights — `pkexec`, a
polkit action, `sudo`, `su` — Sentinel paints a native Breeze/Kirigami dialog
asking you to **Allow** or **Deny**, instead of prompting for a password.

<p align="center">
  <img src="docs/src/screenshots/dialog.png" width="480"
       alt="Sentinel confirmation dialog: 'Authentication Required — the application pkexec is requesting elevated privileges', with Allow and Deny buttons">
</p>

It's a PAM module (`pam_sentinel.so`), a polkit authentication agent, and a
Qt/QML helper, all in Rust ([cxx-qt](https://github.com/KDAB/cxx-qt) for the
GUI). The KDE-native sibling of
[`sentinel`](https://github.com/atayozcan/sentinel) (which targets COSMIC).

> [!CAUTION]
> Sentinel sits in the **PAM authentication path** for polkit and — by
> default — `sudo`/`su`. It wires in **prepend-in-place**: `pam_sentinel.so`
> is added as `auth sufficient` *on top of* your distro's existing stack, so a
> denied, broken, headless, or disabled module **always falls through to your
> normal password** — there is no lockout. Pass `--no-sudo` to guard
> polkit only and leave `sudo`/`su` as plain password prompts. Still: keep a
> root shell open (`sudo -i`) during your first install until you've confirmed
> things work.
>
> **Provided as-is, without warranty** (GPL-3.0 §§15–16). Use at your own risk.

## How it works

- **`sentinel-polkit-agent`** — runs as a **systemd user service**
  (`PartOf=graphical-session.target`), registers with polkitd as the session's
  authentication agent, and shows the dialog. On **Allow** it queues a
  one-shot pre-approval and exposes it on the **system D-Bus**
  (`org.sentinel.Agent`).
- **`pam_sentinel.so`** — wired into the polkit (and, by default, `sudo`/`su`)
  PAM stacks. On an auth it asks the agent over D-Bus whether this attempt was
  pre-approved; if so it returns `PAM_SUCCESS` (no password). Otherwise it
  falls through to the normal password stack.
- **`sentinel-helper-kde`** — the Breeze/Kirigami dialog, painted as a
  `zwlr-layer-shell-v1` overlay (KWin) with a windowed fallback. QML is
  embedded (qrc); untrusted process strings are plain-text + length-clipped.

Click **Show details** and the dialog tells you exactly what's asking — the
full command, PID, working directory, requesting user, and polkit action:

<p align="center">
  <img src="docs/src/screenshots/dialog-details.png" width="420"
       alt="The same dialog with details expanded, showing command, PID, working directory, requesting user, and action">
</p>

The installer makes Sentinel the session's **sole** polkit agent (masks
`plasma-polkit-agent.service`) and installs a polkit rule making the install
user a polkit **administrator** — Sentinel's no-password model is UAC-style:
*you* confirm *your own* escalation. `root` stays an admin too. Both are
reversed by the uninstaller.

### Why D-Bus, and SELinux

The bypass goes over the **system D-Bus**, not a unix socket, on purpose.
openSUSE Tumbleweed runs **SELinux enforcing**, and polkit forks
`polkit-agent-helper-1` from polkitd (domain `policykit_t`), which SELinux
forbids from connecting to an arbitrary socket — but it *already* permits
`policykit_t` to D-Bus to the user's session (that's the polkit agent protocol
itself, and how `pam_fprintd` does passwordless auth). So the bypass works
under **enforcing SELinux with no custom policy module and no weakening of
polkit's sandbox**.

## Requirements (openSUSE Tumbleweed / KDE Plasma 6)

- Rust ≥ 1.85 (`rustup`), `pam-devel`, Qt 6 + KDE Frameworks 6 runtime
  (Kirigami, LayerShellQt, qqc2-desktop-style — already present on Plasma).
- A Wayland session (KWin). X11 is not supported.
- Optional: `canberra-gtk-play` for the UAC sound cue (Sentinel otherwise
  falls back to `pw-play`/`paplay`, present on any PipeWire desktop).

## Install (from source)

```bash
sudo zypper install rustup pam-devel
rustup default 1.85
sudo ./install.sh        # builds, then wires into polkit + sudo/su
                         #   --no-sudo   guard polkit only (leave sudo/su alone)
                         #   --rebuild   force a rebuild even if target/release exists
# verify:
pkexec true              # one Sentinel dialog; Allow → no password, exit 0
```

Remove with `sudo ./uninstall.sh` — it restores polkit-kde, removes the rule,
and reverts every file recorded in `/var/lib/sentinel/install.state`.

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
├── pam-sentinel/           # cdylib → pam_sentinel.so  (D-Bus bypass client)
├── sentinel-polkit-agent/  # bin → the polkit agent + org.sentinel.Agent bus service
└── sentinel-helper-kde/    # bin → the Kirigami dialog (Rust + cxx-qt)
packaging/systemd/user/      # sentinel-polkit-agent.service
packaging/dbus/              # org.sentinel.Agent.conf  (system-bus policy)
scripts/test-install-*.sh    # podman-based install/uninstall tests (9 scenarios)
docs/                        # mdBook user guide + reference
```

## Documentation

Full guide in [`docs/`](docs/src/) (mdBook): installation, configuration, PAM
wiring, architecture, security model, troubleshooting.

## License

**GPL-3.0-or-later.** See [LICENSE](LICENSE).
