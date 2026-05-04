# Installation

Sentinel ships through several channels. AUR is first-class (Arch +
sudo-rs are the primary target); deb / rpm prebuilts cover Debian,
Ubuntu, Fedora, openSUSE; NixOS users get a flake; everyone else
either installs the binary tarball or builds from source.

> **Before you install:** Sentinel sits in the PAM auth path. Open a
> second root shell first (`pkexec bash`) and keep it open until
> you've confirmed `sudo` and `pkexec` still work. The
> [Troubleshooting](./troubleshooting.md) page covers recovery.

## Arch Linux (AUR)

Two packages: `sentinel` (stable releases) and `sentinel-git` (main
branch HEAD).

```bash
yay -S sentinel
# or
paru -S sentinel-git
```

Both `backup=` `/etc/security/sentinel.conf` and `/etc/pam.d/polkit-1`
so a `pacman -Rsn sentinel` won't clobber your customisations.

## Debian / Ubuntu

```bash
curl -LO https://github.com/atayozcan/sentinel/releases/latest/download/sentinel_0.8.0-1_amd64.deb
sudo apt install ./sentinel_0.8.0-1_amd64.deb

# aarch64 (Pi 4/5, Ampere, etc.):
curl -LO https://github.com/atayozcan/sentinel/releases/latest/download/sentinel_0.8.0-1_arm64.deb
sudo apt install ./sentinel_0.8.0-1_arm64.deb
```

After install, the polkit agent autostarts on next graphical login.
Wire `pam_sentinel.so` into `/etc/pam.d/sudo` manually if you want
sudo coverage — see [PAM wiring](./pam-wiring.md).

## Fedora / openSUSE

```bash
curl -LO https://github.com/atayozcan/sentinel/releases/latest/download/sentinel-0.8.0-1.x86_64.rpm
sudo dnf install ./sentinel-0.8.0-1.x86_64.rpm
```

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

```bash
curl -LO https://github.com/atayozcan/sentinel/releases/latest/download/sentinel-0.8.0-x86_64-linux.tar.gz
tar xzf sentinel-0.8.0-x86_64-linux.tar.gz
cd sentinel-0.8.0
sudo ./install.sh
```

## Source

```bash
git clone https://github.com/atayozcan/sentinel
cd sentinel
pkexec ./install.sh
```

The installer:
1. Builds the workspace as the invoking user (cargo target/ stays
   user-owned).
2. Records every replaced file's pre-install state in
   `/var/lib/sentinel/install.state`.
3. Verifies modes/owners on every installed file.
4. Restarts the polkit agent in-place so changes take effect
   without log-out.

`pkexec ./uninstall.sh` rolls everything back to the recorded
pre-install state.

The `--enable-sudo` flag opts into wiring `pam_sentinel.so` into
`/etc/pam.d/sudo` (default: off — see [PAM wiring](./pam-wiring.md)
for why).

## Verifying release artifacts

Every artifact is signed by Sigstore via GitHub's artifact
attestations:

```bash
gh attestation verify sentinel_0.8.0-1_amd64.deb \
    --repo atayozcan/sentinel
```

The signature binds the file's sha256 to the release.yml workflow
run that produced it.
