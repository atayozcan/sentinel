#!/usr/bin/env bash
# Build a release tarball + a prebuilt-binary tarball into ./dist/.
# Usage: ./scripts/build-release.sh [version]
# If version is omitted, reads it from Cargo.toml workspace.package.version.

set -euo pipefail

cd "$(dirname "$0")/.."

if [[ $# -ge 1 ]]; then
    VERSION="$1"
else
    VERSION="$(grep -m1 '^version' Cargo.toml | sed -E 's/.*"([^"]+)".*/\1/')"
fi

[[ -n "$VERSION" ]] || { echo "could not determine version" >&2; exit 1; }

ARCH="$(uname -m)"
DIST=dist
SRC_TAR="$DIST/sentinel-$VERSION.tar.gz"
BIN_TAR="$DIST/sentinel-$VERSION-$ARCH-linux.tar.gz"

rm -rf "$DIST"
mkdir -p "$DIST"

echo "==> Building release ($VERSION, $ARCH)…"
SENTINEL_PREFIX=/usr SENTINEL_SYSCONFDIR=/etc SENTINEL_LIBEXECDIR=lib \
    cargo build --release --workspace --locked

echo "==> Source tarball → $SRC_TAR"
git archive --format=tar.gz \
    --prefix="sentinel-$VERSION/" \
    -o "$SRC_TAR" \
    HEAD

echo "==> Binary tarball → $BIN_TAR"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

mkdir -p \
    "$STAGE/sentinel-$VERSION/usr/lib/security" \
    "$STAGE/sentinel-$VERSION/usr/lib" \
    "$STAGE/sentinel-$VERSION/etc/security" \
    "$STAGE/sentinel-$VERSION/etc/pam.d" \
    "$STAGE/sentinel-$VERSION/usr/share/doc/sentinel" \
    "$STAGE/sentinel-$VERSION/usr/share/licenses/sentinel"

install -Dm755 target/release/sentinel-helper      "$STAGE/sentinel-$VERSION/usr/lib/sentinel-helper"
install -Dm644 target/release/libpam_sentinel.so   "$STAGE/sentinel-$VERSION/usr/lib/security/pam_sentinel.so"
install -Dm644 config/sentinel.conf                "$STAGE/sentinel-$VERSION/etc/security/sentinel.conf"
install -Dm644 config/polkit-1                     "$STAGE/sentinel-$VERSION/etc/pam.d/polkit-1"
install -Dm644 README.md                           "$STAGE/sentinel-$VERSION/usr/share/doc/sentinel/README.md"
install -Dm644 LICENSE                             "$STAGE/sentinel-$VERSION/usr/share/licenses/sentinel/LICENSE"
install -Dm755 install.sh                          "$STAGE/sentinel-$VERSION/install.sh"
install -Dm755 uninstall.sh                        "$STAGE/sentinel-$VERSION/uninstall.sh"

tar -C "$STAGE" -czf "$BIN_TAR" "sentinel-$VERSION"

echo "==> Checksums"
( cd "$DIST" && sha256sum "$(basename "$SRC_TAR")" "$(basename "$BIN_TAR")" > "sentinel-$VERSION.sha256" )

ls -lh "$DIST"
echo
echo "Done. Artefacts in $DIST/."
