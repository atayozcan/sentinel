#!/bin/sh
# Sentinel-KDE .rpm %preun — revert the install.sh wiring on final removal
# ($1 = 0). On upgrade ($1 = 1) do nothing; the new %post reinstalls.
set -e
[ "$1" = 0 ] || exit 0
D=/usr/lib/sentinel-kde
if [ -d "$D/src" ]; then
    ( cd "$D"/src/sentinel-kde-* 2>/dev/null && ./packaging-kde/uninstall.sh -y ) || true
    rm -rf "$D/src"
fi
exit 0
