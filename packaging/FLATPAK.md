# Why Sentinel does not ship a Flatpak

Sentinel is fundamentally a **PAM module**: a shared library
(`pam_sentinel.so`) loaded by the host's `libpam` from
`/usr/lib/security/`. PAM is a host-level authentication framework;
the Flatpak sandbox is built around the inverse principle (apps cannot
touch host security infrastructure). There is no supported path to
install a PAM module from a Flatpak.

That makes the load-bearing piece host-only. The companion
`sentinel-helper` GUI binary could in principle be Flatpak'd alone, but:

1. The PAM module hardcodes the helper path at build time
   (`SENTINEL_PREFIX`/`SENTINEL_LIBEXECDIR`). A Flatpak helper lives at
   `/var/lib/flatpak/exports/bin/...` — paths that vary by install,
   are not predictable from the PAM side, and require runtime
   environment we cannot rely on inside a PAM hook.
2. The helper needs raw Wayland access to bind a `zwlr-layer-shell-v1`
   overlay surface with exclusive keyboard interactivity. The Flatpak
   sandbox can grant Wayland socket access via `--socket=wayland`, but
   the layer-shell protocol is privileged enough that some compositors
   deny it to sandboxed clients.
3. A user installing only the helper Flatpak gets nothing — the PAM
   module is the active component.

## Recommended distribution

| Channel  | Status | Notes |
| -------- | ------ | ----- |
| AUR      | First-class | `sentinel` and `sentinel-git`. See `packaging/arch/PKGBUILD`. |
| .deb     | Shipped | Built by `release.yml`; download from GitHub releases. |
| .rpm     | Shipped | Built by `release.yml`; download from GitHub releases. |
| NixOS    | Shipped | Flake at the repo root, `nix run github:atayozcan/sentinel`. |
| Flatpak  | Not viable | See above. |
| Source   | Supported | `pkexec ./install.sh`. |

If you want to discuss this further, open an issue.
