#!/usr/bin/env bash
# Build release tarballs + generate shell completions and man pages
# into ./dist/. Usage: ./scripts/build-release.sh [version]

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
# Belt-and-braces: the `rm -rf "$DIST"` below would do something nasty
# if DIST ever resolved to empty / root / `.` due to a refactor. The
# `:?` parameter expansion bails with an error message if DIST is
# unset OR empty, so the rm only runs against a real path.
: "${DIST:?DIST must be set to a non-empty relative path}"
SRC_TAR="$DIST/sentinel-$VERSION.tar.gz"
BIN_TAR="$DIST/sentinel-$VERSION-$ARCH-linux.tar.gz"

rm -rf "$DIST"
mkdir -p "$DIST"

echo "==> Building release ($VERSION, $ARCH)…"
SENTINEL_PREFIX=/usr SENTINEL_SYSCONFDIR=/etc SENTINEL_LIBEXECDIR=lib \
    cargo build --release --workspace --locked

# ---------------------------------------------------------------------------
# Generate completions + man pages from the freshly-built binaries. Lives
# under target/release/share/ so cargo-deb / cargo-generate-rpm / install.sh
# can reference them as plain assets.
# ---------------------------------------------------------------------------
echo "==> Generating completions + man pages → target/release/share/"
SHARE=target/release/share
mkdir -p "$SHARE"
for bin in sentinel-helper sentinel-polkit-agent; do
    target/release/$bin completions bash > "$SHARE/$bin.bash"
    target/release/$bin completions fish > "$SHARE/$bin.fish"
    target/release/$bin completions zsh  > "$SHARE/_$bin"
    target/release/$bin man              > "$SHARE/$bin.1"
done

# ---------------------------------------------------------------------------
# Source tarball — generated only on the primary arch.
#
# `git archive HEAD` is bit-for-bit identical regardless of which
# build runner produces it. In a multi-arch matrix (release.yml),
# every arch generating the same `sentinel-$VERSION.tar.gz` ends up
# colliding at the flatten step. Skipping it on non-primary arches
# means each artefact has a unique name and `mv` to a single dist/
# directory just works — no dedup logic needed.
# ---------------------------------------------------------------------------
SKIP_SOURCE_TARBALL=0
if [[ "$ARCH" != "x86_64" ]]; then
    SKIP_SOURCE_TARBALL=1
fi
if [[ $SKIP_SOURCE_TARBALL -eq 0 ]]; then
    echo "==> Source tarball → $SRC_TAR"
    git archive --format=tar.gz \
        --prefix="sentinel-$VERSION/" \
        -o "$SRC_TAR" \
        HEAD
else
    echo "==> Source tarball: skipped on $ARCH (generated only on x86_64)"
fi

# ---------------------------------------------------------------------------
# Binary tarball mirroring the install layout.
# ---------------------------------------------------------------------------
echo "==> Binary tarball → $BIN_TAR"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
ROOT="$STAGE/sentinel-$VERSION"

install -Dm755 target/release/sentinel-helper                       "$ROOT/usr/lib/sentinel-helper"
install -Dm755 target/release/sentinel-polkit-agent                 "$ROOT/usr/lib/sentinel-polkit-agent"
install -Dm755 target/release/libpam_sentinel.so                    "$ROOT/usr/lib/security/pam_sentinel.so"
install -Dm644 config/sentinel.conf                                 "$ROOT/etc/security/sentinel.conf"
install -Dm644 config/polkit-1                                      "$ROOT/etc/pam.d/polkit-1"
install -Dm644 config/sudo                                          "$ROOT/usr/share/doc/sentinel/sudo"
install -Dm644 packaging/xdg-autostart/sentinel-polkit-agent.desktop \
    "$ROOT/etc/xdg/autostart/sentinel-polkit-agent.desktop"
install -Dm644 packaging/systemd/polkit-agent-helper@.service.d/sentinel.conf \
    "$ROOT/etc/systemd/system/polkit-agent-helper@.service.d/sentinel.conf"
install -Dm644 README.md                                            "$ROOT/usr/share/doc/sentinel/README.md"
install -Dm644 LICENSE                                              "$ROOT/usr/share/licenses/sentinel/LICENSE"
install -Dm755 install.sh                                           "$ROOT/install.sh"
install -Dm755 uninstall.sh                                         "$ROOT/uninstall.sh"

# Completions + man pages.
install -Dm644 "$SHARE/sentinel-helper.bash"        "$ROOT/usr/share/bash-completion/completions/sentinel-helper"
install -Dm644 "$SHARE/sentinel-polkit-agent.bash"  "$ROOT/usr/share/bash-completion/completions/sentinel-polkit-agent"
install -Dm644 "$SHARE/sentinel-helper.fish"        "$ROOT/usr/share/fish/vendor_completions.d/sentinel-helper.fish"
install -Dm644 "$SHARE/sentinel-polkit-agent.fish"  "$ROOT/usr/share/fish/vendor_completions.d/sentinel-polkit-agent.fish"
install -Dm644 "$SHARE/_sentinel-helper"            "$ROOT/usr/share/zsh/site-functions/_sentinel-helper"
install -Dm644 "$SHARE/_sentinel-polkit-agent"      "$ROOT/usr/share/zsh/site-functions/_sentinel-polkit-agent"
install -Dm644 "$SHARE/sentinel-helper.1"           "$ROOT/usr/share/man/man1/sentinel-helper.1"
install -Dm644 "$SHARE/sentinel-polkit-agent.1"     "$ROOT/usr/share/man/man1/sentinel-polkit-agent.1"
install -Dm644 packaging/man/sentinel.conf.5        "$ROOT/usr/share/man/man5/sentinel.conf.5"
install -Dm644 packaging/man/pam_sentinel.8         "$ROOT/usr/share/man/man8/pam_sentinel.8"

tar -C "$STAGE" -czf "$BIN_TAR" "sentinel-$VERSION"

# ---------------------------------------------------------------------------
# Checksums. Per-arch sha256 file for the binary tarball; the source
# tarball gets its own sha256 only on the arch that produced it.
# ---------------------------------------------------------------------------
echo "==> Checksums"
( cd "$DIST" && sha256sum "$(basename "$BIN_TAR")" > "sentinel-$VERSION-$ARCH-linux.sha256" )
if [[ $SKIP_SOURCE_TARBALL -eq 0 ]]; then
    ( cd "$DIST" && sha256sum "$(basename "$SRC_TAR")" > "sentinel-$VERSION.sha256" )
fi

ls -lh "$DIST"
echo
echo "Done. Artefacts in $DIST/."
