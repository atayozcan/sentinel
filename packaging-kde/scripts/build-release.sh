#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2025 Atay Özcan <atay@oezcan.me>
# SPDX-License-Identifier: GPL-3.0-or-later

# Build release tarballs into ./dist/:
#   * a source tarball (git archive), and
#   * a binary "installer bundle": the prebuilt binaries plus install.sh, so
#     the target machine runs `sudo SENTINEL_SKIP_BUILD=1 ./install.sh` — which
#     does the distro-aware PAM / systemd / polkit wiring — with no toolchain.
# Usage: ./scripts/build-release.sh [version]

set -euo pipefail
# Repo root is two levels up (packaging-kde/scripts/) in the monorepo.
# cargo build + target/ + the shared Cargo.toml/README/LICENSE/config
# all live here; KDE-specific files are pulled from packaging-kde/.
cd "$(dirname "$0")/../.."

VERSION="${1:-$(grep -m1 '^version' Cargo.toml | sed -E 's/.*"([^"]+)".*/\1/')}"
[[ -n "$VERSION" ]] || { echo "could not determine version" >&2; exit 1; }

ARCH="$(uname -m)"
DIST=dist
# `:?` bails if DIST is ever unset/empty so the rm below can't hit `.` or /.
: "${DIST:?DIST must be set to a non-empty relative path}"
SRC_TAR="$DIST/sentinel-kde-$VERSION.tar.gz"
BIN_TAR="$DIST/sentinel-kde-$VERSION-$ARCH-linux.tar.gz"

rm -rf "$DIST"
mkdir -p "$DIST"

echo "==> Building release ($VERSION, $ARCH)…"
SENTINEL_PREFIX=/usr SENTINEL_SYSCONFDIR=/etc SENTINEL_LIBEXECDIR=lib \
SENTINEL_HELPER_PATH=/usr/lib/sentinel-helper-kde \
    cargo build --release --workspace --locked

# ---------------------------------------------------------------------------
# Source tarball — generated only on the primary arch (git archive is
# identical across arches, so producing it everywhere just collides at the
# flatten step of a multi-arch matrix).
# ---------------------------------------------------------------------------
if [[ "$ARCH" == "x86_64" ]]; then
    echo "==> Source tarball → $SRC_TAR"
    git archive --format=tar.gz --prefix="sentinel-kde-$VERSION/" -o "$SRC_TAR" HEAD
else
    echo "==> Source tarball: skipped on $ARCH (generated only on x86_64)"
fi

# ---------------------------------------------------------------------------
# Binary installer bundle: prebuilt artifacts + the installer. The target
# machine just extracts and runs `sudo SENTINEL_SKIP_BUILD=1 ./install.sh`,
# which detects the PAM module dir, prepends pam_sentinel into the distro's
# polkit-1 stack, installs the systemd user service + polkit rule, and
# generates completions/man pages from the bundled agent binary.
# ---------------------------------------------------------------------------
echo "==> Binary bundle → $BIN_TAR"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
ROOT="$STAGE/sentinel-kde-$VERSION"

mkdir -p "$ROOT/target/release"
install -Dm755 target/release/libpam_sentinel.so    "$ROOT/target/release/libpam_sentinel.so"
install -Dm755 target/release/sentinel-polkit-agent "$ROOT/target/release/sentinel-polkit-agent"
install -Dm755 target/release/sentinel-broker       "$ROOT/target/release/sentinel-broker"
install -Dm755 target/release/sentinel-helper-kde   "$ROOT/target/release/sentinel-helper-kde"
# Everything install.sh reads at SKIP_BUILD time. The KDE installer +
# its packaging dir live under packaging-kde/; the shared workspace
# manifest/docs and the unified config/ are at the repo root.
cp -a packaging-kde/install.sh packaging-kde/uninstall.sh Cargo.toml README.md LICENSE "$ROOT/"
cp -a config "$ROOT/"
cp -a packaging-kde/packaging "$ROOT/packaging"

tar -C "$STAGE" -czf "$BIN_TAR" "sentinel-kde-$VERSION"

# ---------------------------------------------------------------------------
# Checksums. Per-arch sha256 for the binary bundle; the source tarball gets
# its own only on the arch that produced it.
# ---------------------------------------------------------------------------
echo "==> Checksums"
( cd "$DIST" && sha256sum "$(basename "$BIN_TAR")" > "sentinel-kde-$VERSION-$ARCH-linux.sha256" )
[[ "$ARCH" == "x86_64" ]] && \
    ( cd "$DIST" && sha256sum "$(basename "$SRC_TAR")" > "sentinel-kde-$VERSION.sha256" )

ls -lh "$DIST"
echo
echo "Done. Artefacts in $DIST/."
echo "Install from the binary bundle: extract, then  sudo SENTINEL_SKIP_BUILD=1 ./install.sh"
