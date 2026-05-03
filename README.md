# Sentinel

[![CI](https://github.com/atayozcan/sentinel/actions/workflows/ci.yml/badge.svg)](https://github.com/atayozcan/sentinel/actions/workflows/ci.yml)
[![Latest release](https://img.shields.io/github/v/release/atayozcan/sentinel?include_prereleases&sort=semver)](https://github.com/atayozcan/sentinel/releases/latest)
[![License: GPL-3.0-or-later](https://img.shields.io/badge/License-GPL--3.0--or--later-blue.svg)](LICENSE)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-blue.svg)](rust-toolchain.toml)

A Windows UAC-style confirmation dialog for Linux privilege escalation.
PAM module + libcosmic helper, designed for COSMIC and `sudo-rs`,
Wayland-only.

> [!CAUTION]
> Sentinel sits in the **PAM authentication path**. A misconfiguration
> can lock you out of `sudo`, polkit, or login. Read the wiki's
> [Troubleshooting](https://github.com/atayozcan/sentinel/wiki/Troubleshooting)
> page **before** you install. Open a second root shell during the
> first install (`pkexec bash`) and keep it open until you've verified
> `sudo` still works.
>
> **Provided as-is, without warranty of any kind.** The author takes
> no responsibility for damaged systems, lost work, or any other
> consequence of running this software. See [LICENSE](LICENSE) (GPL-3.0
> sections 15 and 16). Use on production systems at your own risk.

## Documentation

Full docs live on the **[wiki](https://github.com/atayozcan/sentinel/wiki)**:

- [Installation](https://github.com/atayozcan/sentinel/wiki/Installation) — AUR, Debian, Fedora, NixOS, generic tarball, source
- [Configuration](https://github.com/atayozcan/sentinel/wiki/Configuration) — `/etc/security/sentinel.conf` reference
- [PAM Wiring](https://github.com/atayozcan/sentinel/wiki/PAM-Wiring) — `sudo`, `polkit`, `su`
- [Building from Source](https://github.com/atayozcan/sentinel/wiki/Building-from-Source)
- [Architecture](https://github.com/atayozcan/sentinel/wiki/Architecture) — design and security model
- [Troubleshooting](https://github.com/atayozcan/sentinel/wiki/Troubleshooting) — recovery, common failures, debug logging
- [Contributing](https://github.com/atayozcan/sentinel/wiki/Contributing)

## Quick install

```bash
# Arch Linux (AUR)
yay -S sentinel

# Debian / Ubuntu
curl -LO https://github.com/atayozcan/sentinel/releases/latest/download/sentinel_0.6.1-1_amd64.deb
sudo apt install ./sentinel_0.6.1-1_amd64.deb

# Fedora / openSUSE
curl -LO https://github.com/atayozcan/sentinel/releases/latest/download/sentinel-0.6.1-1.x86_64.rpm
sudo dnf install ./sentinel-0.6.1-1.x86_64.rpm

# NixOS — flake at the repo root
nix run github:atayozcan/sentinel -- --timeout 10 --randomize

# From source
git clone https://github.com/atayozcan/sentinel
cd sentinel
pkexec ./install.sh
```

See the [Installation](https://github.com/atayozcan/sentinel/wiki/Installation)
wiki page for full instructions, including the prebuilt binary tarball
and per-distro details.

> **Why `pkexec` for the source install?** The installer needs root
> to write to `/etc/pam.d/`, `/etc/security/`, `/usr/lib/security/`,
> and `/etc/systemd/system/`. `pkexec` routes that elevation through
> polkit (which Sentinel itself can be wired into post-install),
> matches the security model of distros that have phased out `sudo`
> in favour of polkit-mediated elevation, and keeps a clean audit
> trail. `sudo` works too if you prefer.

## What it does

When something requests privilege escalation (`sudo`, `pkexec`, …) and
the PAM stack hits `pam_sentinel.so`, the module spawns
`sentinel-helper`. The helper paints a `zwlr-layer-shell-v1` overlay
surface — full-screen translucent backdrop, exclusive keyboard focus,
dialog card centered — and waits for **Allow**, **Deny**, or a
configurable timeout (auto-deny). Allow → PAM passes auth without a
password. Deny / timeout / no Wayland display → PAM continues to the
next module (typically `pam_unix`, the password prompt).

## Compatibility

| Compositor    | Status        | Notes |
| ------------- | ------------- | ----- |
| cosmic-comp   | tested        | primary target |
| KWin/Wayland  | expected to work | Plasma 6.x ships `zwlr-layer-shell-v1`; Sentinel registers ahead of polkit-kde |
| Hyprland      | expected to work | sample animation/blur rules at `packaging/hyprland/sentinel.conf` |
| Sway          | expected to work | reference wlroots compositor |
| Niri          | expected to work | layer-shell overlay anchors fullscreen as on other wlroots-style compositors |
| Wayfire       | expected to work | wlroots-based |
| River         | expected to work | wlroots-based |
| GNOME/Mutter  | auto-fallback | Mutter has no `zwlr-layer-shell-v1`. Helper detects via `XDG_CURRENT_DESKTOP` and falls back to `xdg-toplevel` (regular window) automatically; force with `--windowed`. |
| Pantheon / Budgie / Unity | auto-fallback | Same as GNOME — Mutter-based. |
| X11 only      | not supported | Wayland-only |

If you've used Sentinel on a compositor in the "expected to work"
list and want it promoted to "tested", open a PR updating this
table — bonus points for a screenshot.

## Project layout

```
.
├── Cargo.toml                  # workspace root
├── crates/
│   ├── sentinel-shared/        # shared schema, parser, /proc + logind readers, log_kv
│   ├── pam-sentinel/           # cdylib → /usr/lib/security/pam_sentinel.so
│   ├── sentinel-helper/        # bin    → /usr/lib/sentinel-helper
│   │   └── locales/            # 12 embedded fluent bundles (en-US, de-DE, …)
│   └── sentinel-polkit-agent/  # bin    → /usr/lib/sentinel-polkit-agent
├── config/                     # /etc/security/sentinel.conf, /etc/pam.d/{polkit-1,sudo}
├── packaging/                  # Arch PKGBUILDs, debian + systemd + xdg, FLATPAK rationale
├── nix/module.nix              # NixOS module
├── flake.nix
├── scripts/build-release.sh    # source + binary tarballs
├── install.sh / uninstall.sh   # transactional installer (auto-rollback, in-place agent restart)
└── .github/workflows/
    ├── ci.yml                  # fmt + clippy + test + build on PRs
    └── release.yml             # tag v* → builds + GH release
```

## License

**GPL-3.0-or-later.** See [LICENSE](LICENSE). GPL-3.0 sections 15 and
16 disclaim all warranty and limit liability — see the
[Home](https://github.com/atayozcan/sentinel/wiki) page of the wiki
for the full quoted text.
