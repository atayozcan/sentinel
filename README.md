# Sentinel

A Windows UAC-style confirmation dialog for Linux privilege escalation,
delivered as a PAM module plus a libcosmic helper. Designed for the COSMIC
desktop and `sudo-rs`.

![dialog overlay](docs/screenshots/dialog-overlay.png)

---

> [!CAUTION]
> **Sentinel sits in the PAM authentication path.** A misconfiguration —
> wrong directive in `/etc/pam.d/sudo`, broken polkit policy, a bug in this
> module — can lock you out of `sudo`, polkit, or even login. **Read the
> "Locked out?" section *before* you install.** Always keep a root shell
> open in another terminal during the first install, or have a recovery
> medium (live USB, single-user mode) ready.
>
> This software is provided **as-is, without warranty of any kind**. The
> author takes no responsibility for damaged systems, lost work, or any
> other consequence of running it. See [License](#license) and the
> `LICENSE` file (GPL-3.0 sections 15 and 16) for the full disclaimer.
> Use it on production systems at your own risk.

---

## Features

- **Graphical confirm/deny dialog** before any PAM-protected privilege escalation
- **libcosmic native UI** — matches the COSMIC desktop visually and theme-wise
- **Layer-shell overlay** — full-screen translucent backdrop with exclusive
  keyboard focus, drawn on top of all other surfaces (`zwlr-layer-shell-v1`)
- **Wayland-only**, designed for `cosmic-comp` and other wlroots-based compositors
- **`sudo-rs` friendly** — drop-in via `/etc/pam.d/sudo`
- **Randomised button positions** to deter automated clickers
- **Configurable timeout** with auto-deny
- **Minimum display time** to block instant scripted clicks
- **Per-service overrides** (`sudo`, `su`, `polkit-1`, etc.)
- **Headless fallback** to standard password authentication
- **Transactional install/uninstall** — every system change is recorded and
  rolled back automatically if anything fails

## Screenshots

| Layer-shell overlay | Randomised buttons |
| ------------------- | ------------------ |
| ![overlay](docs/screenshots/dialog-overlay.png) | ![randomized](docs/screenshots/dialog-randomized.png) |

> If the images above are missing, the project hasn't shipped them yet —
> they live in `docs/screenshots/`.

## Prerequisites

**Runtime**

| Component                | Why                                              |
| ------------------------ | ------------------------------------------------ |
| Linux + libpam           | The PAM module loads into your auth stack        |
| Wayland compositor with  | The helper renders its overlay there             |
| `zwlr-layer-shell-v1`    | Supported by cosmic-comp, Hyprland, Sway, KWin   |
| `libxkbcommon`, `mesa`,  | libcosmic / iced rendering stack                 |
| `fontconfig`, `freetype` |                                                  |
| `vulkan-icd-loader`      | wgpu prefers Vulkan; software fallback works too |

**For source builds**

| Component             | Version |
| --------------------- | ------- |
| Rust + Cargo (stable) | ≥ 1.85  |
| `pkg-config`          | any     |
| `wayland-protocols`   | any     |
| pam dev headers       | any     |

The exact toolchain is pinned in `rust-toolchain.toml`. Rustup will install
the right version automatically.

**Compatibility**

| Compositor   | Status        |
| ------------ | ------------- |
| cosmic-comp  | tested        |
| Hyprland     | should work   |
| Sway         | should work   |
| KWin/Wayland | should work   |
| GNOME/Mutter | **no** — Mutter does not implement `zwlr-layer-shell-v1`. Use `--windowed`. |
| X11 only     | not supported |

## Install

### Arch Linux (AUR)

```bash
yay -S sentinel        # latest release
# or
yay -S sentinel-git    # main branch
```

You can also clone and build the PKGBUILD locally:

```bash
git clone https://github.com/atayozcan/sentinel
cd sentinel/packaging/arch
makepkg -si
```

### Debian / Ubuntu

A `.deb` is published with each release.

```bash
curl -LO https://github.com/atayozcan/sentinel/releases/latest/download/sentinel_0.2.0_amd64.deb
sudo apt install ./sentinel_0.2.0_amd64.deb
```

### Fedora / openSUSE / RHEL

A `.rpm` is published with each release.

```bash
curl -LO https://github.com/atayozcan/sentinel/releases/latest/download/sentinel-0.2.0-1.x86_64.rpm
sudo dnf install ./sentinel-0.2.0-1.x86_64.rpm     # Fedora
sudo zypper install ./sentinel-0.2.0-1.x86_64.rpm  # openSUSE
```

### NixOS

A flake is provided at the repo root:

```bash
nix run github:atayozcan/sentinel#sentinel-helper -- --timeout 10 --randomize
```

For a system module, see `nix/module.nix`.

### Generic Linux (prebuilt tarball)

```bash
curl -LO https://github.com/atayozcan/sentinel/releases/latest/download/sentinel-0.2.0-x86_64-linux.tar.gz
tar xf sentinel-0.2.0-x86_64-linux.tar.gz
cd sentinel-0.2.0
pkexec ./install.sh
```

### From source

```bash
git clone https://github.com/atayozcan/sentinel
cd sentinel
just install        # builds, then runs install.sh under pkexec
# or, manually:
pkexec ./install.sh
```

`install.sh` is **transactional**: every change is logged to
`/var/lib/sentinel/install.state`, and any failure mid-install rolls back
to the exact pre-install state. To enable Sentinel for `sudo` (which is
prompted interactively), pass `--enable-sudo`.

To remove:

```bash
pkexec ./uninstall.sh           # interactive
pkexec ./uninstall.sh --yes     # non-interactive
```

`uninstall.sh` reads the install state file and restores any pre-existing
files it backed up (`polkit-1`, `sudo`, `sentinel.conf`).

## Configuration

Edit `/etc/security/sentinel.conf` (TOML):

```toml
[general]
enabled = true
timeout = 30
randomize_buttons = true
headless_action = "password"  # "allow" | "deny" | "password"
show_process_info = true
log_attempts = true
min_display_time_ms = 500

[appearance]
title = "Authentication Required"
message = 'The application "%p" is requesting elevated privileges.'
secondary = 'Click "Allow" to continue or "Deny" to cancel.'

[services.sudo]
enabled = true
timeout = 30
randomize = true
```

Substitutions inside `appearance.message` and `appearance.secondary`:

| Token | Meaning |
| ----- | ------- |
| `%u`  | User    |
| `%s`  | Service |
| `%p`  | Requesting process |
| `%%`  | Literal `%` |

## PAM wiring

### sudo / sudo-rs

Both use `/etc/pam.d/sudo`. Add the module before the password line:

```
#%PAM-1.0
auth       sufficient pam_sentinel.so
auth       include    system-auth
account    include    system-auth
password   include    system-auth
session    include    system-auth
```

`sufficient` means: if Sentinel says ALLOW, no password is required.
If it says DENY (or there's no display), PAM continues to the next module.

### polkit

`/etc/pam.d/polkit-1` is installed automatically.

## Testing the helper

```bash
just helper-test
# or:
target/release/sentinel-helper --timeout 10 --randomize \
    --process-exe /usr/bin/sudo
```

Pass `--windowed` to render as a normal xdg-toplevel instead of a layer-shell
overlay (useful for debugging on compositors without `zwlr-layer-shell-v1`).

The helper prints `ALLOW`, `DENY`, or `TIMEOUT` to stdout and exits with
0 on allow, 1 otherwise.

## Headless / SSH / TTY behaviour

The helper itself is graphical-only — when `$WAYLAND_DISPLAY` is unset
(SSH session, console login) it prints `DENY` and exits.

The fallback happens **at the PAM layer**: `pam_sentinel.so` reads
`headless_action` from `sentinel.conf`. With the default
`headless_action = "password"`, the PAM stack continues to the next module
(`pam_unix`) and prompts for a password as normal. So your SSH login or
console `sudo` keeps working unchanged.

## Locked out?

If `pam_sentinel.so` ever blocks login or sudo:

1. **Boot into single-user mode** (add `init=/bin/bash` to the kernel
   command line in your boot loader), or
2. **Boot a live USB** and chroot into the install, or
3. **Switch to another TTY** (Ctrl+Alt+F2..F6) and log in as another user
   that doesn't go through Sentinel,

then remove the offending line from `/etc/pam.d/sudo` (or
`/etc/pam.d/system-auth`, or `/etc/pam.d/polkit-1`). If you have the
install-state file, `pkexec /path/to/sentinel-source/uninstall.sh --yes`
will undo everything cleanly.

Practical advice: **before you install, open a second terminal as root
(`pkexec bash`)** and keep it open until you've verified `sudo` works.

## Project layout

```
.
├── Cargo.toml                  # Cargo workspace
├── crates/
│   ├── pam-sentinel/           # cdylib → /usr/lib/security/pam_sentinel.so
│   └── sentinel-helper/        # bin    → /usr/lib/sentinel-helper
├── config/
│   ├── sentinel.conf           # TOML configuration → /etc/security/
│   ├── polkit-1                # /etc/pam.d/polkit-1
│   └── sudo                    # optional /etc/pam.d/sudo
├── packaging/
│   ├── arch/PKGBUILD           # AUR release
│   ├── arch/PKGBUILD-git       # AUR -git package
│   ├── debian/                 # cargo-deb metadata
│   ├── rpm/                    # cargo-generate-rpm metadata
│   └── FLATPAK.md              # why no Flatpak
├── nix/                        # flake module
├── docs/screenshots/           # README assets
├── scripts/build-release.sh    # builds the release tarballs
├── .github/workflows/release.yml
├── install.sh / uninstall.sh   # transactional source-build installer
└── justfile                    # build / lint / install recipes
```

## License

**GPL-3.0-or-later.** See [`LICENSE`](LICENSE) for the full text.

GPL-3.0 ships with explicit no-warranty and limitation-of-liability terms:

> **15. Disclaimer of Warranty.**
> THERE IS NO WARRANTY FOR THE PROGRAM, TO THE EXTENT PERMITTED BY APPLICABLE LAW.
> EXCEPT WHEN OTHERWISE STATED IN WRITING THE COPYRIGHT HOLDERS AND/OR OTHER PARTIES
> PROVIDE THE PROGRAM "AS IS" WITHOUT WARRANTY OF ANY KIND […].
>
> **16. Limitation of Liability.**
> IN NO EVENT […] WILL ANY COPYRIGHT HOLDER, OR ANY OTHER PARTY […], BE LIABLE TO
> YOU FOR DAMAGES, INCLUDING ANY GENERAL, SPECIAL, INCIDENTAL OR CONSEQUENTIAL
> DAMAGES ARISING OUT OF THE USE OR INABILITY TO USE THE PROGRAM […].

You run Sentinel at your own risk.

## Contributing

Issues and PRs welcome. Please run `cargo fmt`, `cargo clippy --workspace
-- -D warnings`, and `just helper-test` before submitting.
