# Arch packaging

Two PKGBUILDs:

- `PKGBUILD` — stable release. Pulls the GitHub release tarball
  (`v$pkgver`). Submit to AUR as **`sentinel`**.
- `PKGBUILD-git` — VCS package. Pulls main branch HEAD. Submit as
  **`sentinel-git`**.

## Submitting to AUR (first time)

```bash
# 1. Create the SSH key + AUR account at https://aur.archlinux.org/

# 2. Clone an empty AUR repo (the name is the package name).
git clone ssh://aur@aur.archlinux.org/sentinel.git aur-sentinel
cd aur-sentinel

# 3. Copy in the PKGBUILD and generate .SRCINFO.
cp ../packaging/arch/PKGBUILD .
# Refresh the source checksum to a real value (replaces SKIP):
updpkgsums                          # from `pacman-contrib`
makepkg --printsrcinfo > .SRCINFO

# 4. Verify it builds in a clean chroot.
makepkg -si --clean

# 5. Commit and push.
git add PKGBUILD .SRCINFO
git commit -m "sentinel 0.2.0-1: initial release"
git push origin master

# 6. Repeat for sentinel-git in a separate clone.
git clone ssh://aur@aur.archlinux.org/sentinel-git.git aur-sentinel-git
cd aur-sentinel-git
cp ../packaging/arch/PKGBUILD-git PKGBUILD
makepkg --printsrcinfo > .SRCINFO
git add PKGBUILD .SRCINFO
git commit -m "sentinel-git: initial release"
git push origin master
```

## Updating for a new release

```bash
cd aur-sentinel
# Bump pkgver / pkgrel in PKGBUILD, refresh checksum:
updpkgsums
makepkg --printsrcinfo > .SRCINFO
git commit -am "sentinel $(grep -m1 ^pkgver= PKGBUILD | cut -d= -f2)-1"
git push
```

## Local test (no AUR push needed)

```bash
cd packaging/arch
makepkg -si              # build + install in one step
```
