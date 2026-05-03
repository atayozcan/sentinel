#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2025 Atay Özcan <atay@oezcan.me>
# SPDX-License-Identifier: GPL-3.0-or-later

# scripts/dev-test.sh — build, install to system paths, run a tiny PAM auth
# probe against a dedicated test service, then roll everything back.
#
# Idempotent: each invocation rebuilds with the latest code, exercises the
# full PAM → helper flow, and uninstalls before exiting.
#
# Do NOT run this on a machine where Sentinel is installed for real (the AUR
# package, /etc/pam.d/sudo wired, etc.) — the cleanup would remove that too.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

cyan() { printf '\033[1;36m%s\033[0m\n' "$*"; }
red()  { printf '\033[1;31m%s\033[0m\n' "$*" >&2; }

if [[ -e /usr/lib/security/pam_sentinel.so || -e /etc/security/sentinel.conf ]]; then
    red "Refusing to run: an existing Sentinel install is present."
    red "Uninstall it first (./uninstall.sh) or remove the dev-test guard."
    exit 1
fi

cyan "[1/4] Building workspace…"
cargo build --release --workspace

cyan "[2/4] Compiling PAM auth probe…"
AUTHTEST="$REPO_ROOT/target/sentinel-authtest"
if [[ ! -x "$AUTHTEST" || "$REPO_ROOT/scripts/pam_authtest.rs" -nt "$AUTHTEST" ]]; then
    rustc -O "$REPO_ROOT/scripts/pam_authtest.rs" -l pam -o "$AUTHTEST"
fi

cyan "[3/4] Installing, exercising, and rolling back (pkexec)…"
cyan "      Watch syslog in another terminal:  journalctl -t pam_sentinel -f"

# Everything inside this heredoc runs as root under pkexec.
# Cleanup runs unconditionally via trap on EXIT.
pkexec env \
    REAL_USER="$USER" \
    REPO_ROOT="$REPO_ROOT" \
    AUTHTEST="$AUTHTEST" \
    bash <<'ROOT'
set -euo pipefail
cd "$REPO_ROOT"

cleanup() {
    set +e
    rm -f /usr/lib/security/pam_sentinel.so \
          /usr/lib/sentinel-helper \
          /etc/security/sentinel.conf \
          /etc/pam.d/sentinel-test
    echo
    echo ">>> Rolled back."
}
trap cleanup EXIT

install -Dm644 target/release/libpam_sentinel.so /usr/lib/security/pam_sentinel.so
install -Dm755 target/release/sentinel-helper    /usr/lib/sentinel-helper
install -Dm644 config/sentinel.conf              /etc/security/sentinel.conf

cat > /etc/pam.d/sentinel-test <<EOF
#%PAM-1.0
auth    sufficient  pam_sentinel.so
auth    required    pam_deny.so
account required    pam_permit.so
EOF
chmod 644 /etc/pam.d/sentinel-test

echo
echo ">>> Authenticating user '$REAL_USER' against service 'sentinel-test'."
echo ">>> The Sentinel dialog should appear in your COSMIC session."
echo

probe_started=$(date '+%Y-%m-%d %H:%M:%S')

if "$AUTHTEST" sentinel-test "$REAL_USER"; then
    echo
    echo ">>> Result: ALLOW (exit 0)"
else
    echo
    echo ">>> Result: DENY or error (non-zero exit)"
fi

echo
echo ">>> pam_sentinel syslog (since probe started):"
journalctl -t pam_sentinel --since "$probe_started" --no-pager 2>/dev/null \
    | sed 's/^/    /' \
    || echo "    (journalctl unavailable)"
ROOT

cyan "[4/4] Done."