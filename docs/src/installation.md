# Installation

Sentinel ships through several channels. AUR is first-class (Arch +
sudo-rs are the primary target); NixOS users get a flake; everyone else
either installs the prebuilt binary tarball or builds from source.

> **Before you install:** Sentinel sits in the PAM auth path. Open a
> second root shell first (`pkexec bash`) and keep it open until
> you've confirmed `sudo` and `pkexec` still work. The
> [Troubleshooting](./troubleshooting.md) page covers recovery.

## Arch Linux (AUR)

`sentinel-kde` (KDE Plasma) is built from this repository.

```bash
yay -S sentinel-kde       # KDE Plasma / Kirigami dialog
```

It `backup=`s `/etc/security/sentinel.conf` and `/etc/pam.d/polkit-1`
so a `pacman -Rsn` won't clobber your customisations.

## NixOS

The repo's `flake.nix` exposes a NixOS module:

```nix
{
  inputs.sentinel.url = "github:atayozcan/sentinel";

  outputs = { self, nixpkgs, sentinel, ... }: {
    nixosConfigurations.<host> = nixpkgs.lib.nixosSystem {
      modules = [
        sentinel.nixosModules.default
        ({ ... }: {
          services.sentinel.enable = true;
          services.sentinel.enableForSudo = false;  # opt-in
        })
      ];
    };
  };
}
```

Or run the helper ad-hoc without installing:

```bash
nix run github:atayozcan/sentinel -- --timeout 10 --randomize
```

## Generic binary tarball

Each release publishes a prebuilt bundle per arch,
`sentinel-kde-<ver>-<arch>-linux.tar.gz`. Extract and run its
`install.sh` with `SENTINEL_SKIP_BUILD=1` — no toolchain needed.

```bash
curl -LO https://github.com/atayozcan/sentinel/releases/latest/download/sentinel-kde-0.9.0-x86_64-linux.tar.gz
tar xzf sentinel-kde-0.9.0-x86_64-linux.tar.gz
cd sentinel-kde-0.9.0
sudo SENTINEL_SKIP_BUILD=1 ./install.sh
```

## Source

The KDE frontend installs from `packaging-kde/`, pulling in the shared
backend.

```bash
git clone https://github.com/atayozcan/sentinel
cd sentinel
pkexec ./packaging-kde/install.sh
```

The installer:
1. Builds the workspace as the invoking user (cargo target/ stays
   user-owned).
2. Records every replaced file's pre-install state in
   `/var/lib/sentinel/install.state`.
3. Verifies modes/owners on every installed file.
4. Restarts the polkit agent in-place so changes take effect
   without log-out.

`pkexec ./packaging-kde/uninstall.sh` rolls everything back to the
recorded pre-install state.

The `--enable-sudo` flag opts into wiring `pam_sentinel.so` into
`/etc/pam.d/sudo` (default: off — see [PAM wiring](./pam-wiring.md)
for why).

## Verifying release artifacts

Every artifact is signed by Sigstore via GitHub's artifact
attestations:

```bash
gh attestation verify sentinel-kde-0.9.0-x86_64-linux.tar.gz \
    --repo atayozcan/sentinel
```

The signature binds the file's sha256 to the release.yml workflow
run that produced it.
