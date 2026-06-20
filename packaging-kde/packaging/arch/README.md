# Arch packaging

`PKGBUILD` — stable release. Pulls the GitHub release tarball (`v$pkgver`).
Submit to AUR as **`sentinel-kde`**.

Ships:

- `sentinel-helper-kde` + `sentinel-polkit-agent` to `/usr/lib/`
- `pam_sentinel.so` to `/usr/lib/security/`
- systemd **user** service (`/usr/lib/systemd/user/sentinel-polkit-agent.service`)
  symlinked into `graphical-session.target.wants/`
- DBus system-bus policy (`/usr/share/dbus-1/system.d/org.sentinel.Agent.conf`)
- Polkit admin rule (`/etc/polkit-1/rules.d/49-sentinel-admin.rules`)

Conflicts with `polkit-kde-agent` (only one polkit auth-agent can register per
session) and `provides=polkit-kde-agent` so `plasma-meta`'s dep is satisfied —
pacman can swap them in a single transaction.

> **No `-git` companion.** KSXGitHub's `deploy-aur` runs `updpkgsums`, which
> turns a `-git` PKGBUILD's `source=("…::git+…")` into a real upstream clone
> *inside the AUR working tree*. AUR rejects pushes whose commit contains
> any subdirectory, so the action can't submit a `-git` package end-to-end
> from CI. The canonical AUR package is stable-only; if you want VCS
> tracking, install from this repo directly (`git clone … && sudo
> ./install.sh`).

## Local build + test

```bash
# From the repo root:
cd packaging-kde/packaging/arch
makepkg -Cs                   # clean tree, fetch makedepends
sudo pacman -U sentinel-kde-*.pkg.tar.zst
```

`build()` compiles only this frontend + the backend
(`-p sentinel-helper-kde -p sentinel-polkit-agent -p pam-sentinel`), so it
never pulls libcosmic. There is no `check()`: `fmt`/`clippy`/tests run in CI
(`ci.yml`) on every push and the AUR publish is gated on a green release, so
re-running the suite on every install would be redundant build time.

## Submitting to AUR (first time)

```bash
# 1. Create an SSH key + AUR account at https://aur.archlinux.org/

# 2. Clone the (empty) AUR repo. The repo name IS the package name.
git clone ssh://aur@aur.archlinux.org/sentinel-kde.git aur-sentinel-kde
cd aur-sentinel-kde

# 3. Copy in the PKGBUILD and the install hook.
cp ../packaging/arch/PKGBUILD .
cp ../packaging/arch/sentinel-kde.install .

# 4. Replace the SKIP checksum with the real release tarball SHA-256.
#    (Requires `pacman-contrib` for updpkgsums.)
updpkgsums

# 5. Generate .SRCINFO.
makepkg --printsrcinfo > .SRCINFO

# 6. Verify a clean-chroot build.
makepkg -si --clean

# 7. Commit and push.
git add PKGBUILD .SRCINFO sentinel-kde.install
git commit -m "sentinel-kde ${pkgver}-${pkgrel}: initial release"
git push origin master
```

## Updating on release

For each new tag `vX.Y.Z`:

```bash
cd aur-sentinel-kde
git pull
# Edit PKGBUILD: bump pkgver, reset pkgrel=1.
updpkgsums
makepkg --printsrcinfo > .SRCINFO
makepkg -si --clean      # sanity check
git commit -am "sentinel-kde X.Y.Z-1"
git push
```

The `.github/workflows/release.yml` workflow does steps 4–7 automatically
on every `v*` tag push.

## Activation after install

Pacman's install hook prints the activation steps; the short version:

```bash
systemctl --user mask  plasma-polkit-agent.service   # if you had it
systemctl --user start sentinel-polkit-agent.service
```

The unit auto-starts on next login (it's symlinked into
`graphical-session.target.wants/`).

To also guard `sudo` / `su`:

```bash
sudo install -m644 /usr/share/doc/sentinel-kde/sudo /etc/pam.d/sudo
sudo install -m644 /usr/share/doc/sentinel-kde/su   /etc/pam.d/su
```

Diff against your distro originals first — silently rewriting
`/etc/pam.d/sudo` is a notorious foot-gun.
