# Arch packaging

Two PKGBUILDs:

- `PKGBUILD` — stable release. Pulls the GitHub release tarball
  (`v$pkgver`). Submit to AUR as **`sentinel-kde`**.
- `PKGBUILD-git` — VCS package. Pulls main branch HEAD. Submit as
  **`sentinel-kde-git`**.

Both ship the same set of files, install the systemd **user** service
(`/usr/lib/systemd/user/sentinel-polkit-agent.service`) symlinked into
`graphical-session.target.wants/`, and conflict with `polkit-kde-agent`
(only one polkit auth-agent can register per session). They also
`provides=polkit-kde-agent` so `plasma-meta`'s dep is satisfied — pacman
can swap them in a single transaction.

## Local build + test

```bash
# From the repo root:
cd packaging/arch
makepkg -Cs                   # clean tree, fetch makedepends
sudo pacman -U sentinel-kde-*.pkg.tar.zst

# Or build the -git variant:
ln -sf PKGBUILD-git PKGBUILD  # makepkg always reads ./PKGBUILD
makepkg -Cs
```

`makepkg --check` runs the workspace's pure-Rust tests (PAM module +
agent + shared); the Qt/QML helper's unit tests are skipped because
they need a Wayland compositor.

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

Same flow for `sentinel-kde-git`, but skip step 4 (VCS packages keep
`sha256sums=('SKIP')`).

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

The `-git` variant only needs a push when the PKGBUILD itself changes
(deps, build flags, file layout). `pkgver` is recomputed at build time
by the `pkgver()` function — no manual bump.

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
