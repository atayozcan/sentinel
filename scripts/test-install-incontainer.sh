#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2025 Atay Özcan <atay@oezcan.me>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Runs INSIDE a throwaway container (see scripts/test-install-container.sh).
# Stages the pre-built release artifacts + a fake desktop environment, then
# exercises install.sh / uninstall.sh for one scenario (argv[1]) with loud
# assertions. Exits non-zero on the first failed assertion.
#
# The repo is bind-mounted read-only at /src; we copy just what the
# installer needs into /work (NOT target/, which is huge) and run with
# SENTINEL_SKIP_BUILD=1 so the staged artifacts are used as-is.

set -uo pipefail

SCENARIO="${1:?scenario name required}"

fail() { echo "  ASSERT FAIL: $*" >&2; exit 1; }
assert_file()   { [[ -f "$1" ]] || fail "expected regular file: $1"; }
assert_exe()    { [[ -x "$1" ]] || fail "expected executable: $1"; }
assert_absent() { [[ ! -e "$1" ]] || fail "expected absent: $1"; }
assert_mode()   { local m; m=$(stat -c %a "$1" 2>/dev/null) || fail "stat $1"; [[ "$m" == "$2" ]] || fail "$1 mode $m != $2"; }
assert_owner0() { local o; o=$(stat -c %u:%g "$1" 2>/dev/null) || fail "stat $1"; [[ "$o" == "0:0" ]] || fail "$1 owner $o != 0:0"; }
assert_grep()   { grep -q -- "$2" "$1" || fail "no /$2/ in $1"; }
assert_nogrep() { ! grep -q -- "$2" "$1" || fail "unexpected /$2/ in $1"; }

# Default install paths (PREFIX=/usr LIBEXECDIR=lib SYSCONFDIR=/etc).
PAM_SO=/usr/lib/security/pam_sentinel.so
AGENT=/usr/lib/sentinel-polkit-agent
CONF=/etc/security/sentinel.conf
POLKIT1=/etc/pam.d/polkit-1
AUTOSTART=/etc/xdg/autostart/sentinel-polkit-agent.desktop
DROPIN=/etc/systemd/system/polkit-agent-helper@.service.d/sentinel.conf
KDEDESK=/etc/xdg/autostart/org.kde.polkit-kde-authentication-agent-1.desktop
STATE=/var/lib/sentinel/install.state
HELPER=/usr/bin/sentinel-helper-kde

stage() {
    # Copy only what install.sh reads — never the multi-GB target/ tree.
    mkdir -p /work/target/release
    cp -a /src/install.sh /src/uninstall.sh /src/Cargo.toml /work/
    cp -a /src/config /src/packaging /work/
    cp -a /src/target/release/libpam_sentinel.so \
          /src/target/release/sentinel-polkit-agent /work/target/release/
    cd /work
    # Pretend the sentinel-helper-kde package is installed.
    install -Dm755 /src/target/release/sentinel-helper-kde "$HELPER"
    # A fake polkit-kde autostart entry to exercise disable/restore.
    install -Dm644 /dev/stdin "$KDEDESK" <<'DESK'
[Desktop Entry]
Type=Application
Name=PolicyKit Authentication Agent (KDE)
Exec=/usr/libexec/polkit-kde-authentication-agent-1
DESK
    export SENTINEL_SKIP_BUILD=1
}

assert_installed_kde() {
    assert_file "$PAM_SO";  assert_mode "$PAM_SO" 755;  assert_owner0 "$PAM_SO"
    assert_exe  "$AGENT";   assert_mode "$AGENT" 755;   assert_owner0 "$AGENT"
    assert_file "$CONF";    assert_mode "$CONF" 644
    assert_file "$POLKIT1"
    assert_file "$AUTOSTART"
    assert_file "$DROPIN"
    assert_file "$STATE"
    assert_exe  "$HELPER"
    assert_grep "$KDEDESK" '^Hidden=true'
    assert_file "${KDEDESK}.pre-sentinel.bak"
    # The COSMIC helper must NOT have been installed in --kde mode.
    assert_absent /usr/lib/sentinel-helper
}

case "$SCENARIO" in
  fresh)
    stage
    ./install.sh --kde
    assert_installed_kde
    echo "  OK: fresh KDE install placed all files (root:root, correct modes) + disabled polkit-kde"
    ;;

  idempotent)
    stage
    ./install.sh --kde
    ./install.sh --kde      # second run must succeed cleanly
    assert_installed_kde
    # The backup must still be the REAL original, not a sentinel-modified copy.
    assert_nogrep "${KDEDESK}.pre-sentinel.bak" '^Hidden=true'
    echo "  OK: re-install is idempotent; backup still holds the true original"
    ;;

  uninstall)
    stage
    ./install.sh --kde
    ./uninstall.sh -y
    assert_absent "$PAM_SO"
    assert_absent "$AGENT"
    assert_absent "$CONF"
    assert_absent "$AUTOSTART"
    assert_absent "$DROPIN"
    assert_absent "$STATE"
    assert_exe    "$HELPER"                    # package-owned helper left intact
    assert_file   "$KDEDESK"
    assert_nogrep "$KDEDESK" '^Hidden=true'    # polkit-kde re-enabled
    assert_absent "${KDEDESK}.pre-sentinel.bak"
    echo "  OK: uninstall restored pre-install state + re-enabled polkit-kde"
    ;;

  rollback)
    stage
    # Make pam.so's parent dir a regular FILE so install -D fails *after*
    # the agent is already installed → exercises the rollback path.
    rm -rf /usr/lib/security
    : > /usr/lib/security
    if ./install.sh --kde; then fail "install should have failed"; fi
    assert_absent "$AGENT"                     # rolled back
    assert_absent "$CONF"                      # never reached
    assert_absent "$STATE"                     # never committed
    assert_nogrep "$KDEDESK" '^Hidden=true'    # never disabled
    echo "  OK: failed install rolled back cleanly (no partial state)"
    ;;

  fallback-uninstall)
    stage
    ./install.sh --kde
    rm -f "$STATE"                             # simulate a lost state file
    ./uninstall.sh -y
    assert_absent "$PAM_SO"
    assert_absent "$AGENT"
    assert_file   "$KDEDESK"
    assert_nogrep "$KDEDESK" '^Hidden=true'    # restored via fallback glob
    echo "  OK: fallback uninstall (no state file) cleaned up + restored polkit-kde"
    ;;

  preexisting-config)
    stage
    install -Dm644 /dev/stdin "$POLKIT1" <<'ORIG'
# ORIGINAL-POLKIT1-STACK
auth include common-auth
ORIG
    ./install.sh --kde
    assert_grep "${POLKIT1}.pre-sentinel.bak" 'ORIGINAL-POLKIT1-STACK'
    assert_nogrep "$POLKIT1" 'ORIGINAL-POLKIT1-STACK'   # replaced by sentinel's
    ./uninstall.sh -y
    assert_grep "$POLKIT1" 'ORIGINAL-POLKIT1-STACK'     # original restored verbatim
    echo "  OK: pre-existing /etc/pam.d/polkit-1 backed up + restored"
    ;;

  err-nonroot)
    stage
    if ! command -v runuser >/dev/null 2>&1; then
        echo "  SKIP: runuser unavailable in this image"; echo "SCENARIO-OK: $SCENARIO"; exit 0
    fi
    chmod -R a+rx /work                       # let 'nobody' read the script
    # 'nobody' (uid 65534) exists in the base image; no user-management needed.
    if runuser -u nobody -- ./install.sh --kde 2>/dev/null; then fail "should refuse non-root"; fi
    assert_absent "$PAM_SO"
    echo "  OK: refuses to run as non-root (and installs nothing)"
    ;;

  err-badflag)
    stage
    if ./install.sh --totally-bogus 2>/dev/null; then fail "should reject unknown flag"; fi
    echo "  OK: rejects unknown flag"
    ;;

  err-nohelper)
    stage
    rm -f "$HELPER"
    if ./install.sh --kde 2>/dev/null; then fail "should require the KDE helper"; fi
    assert_absent "$PAM_SO"                    # nothing installed on early error
    echo "  OK: errors when the KDE helper is absent (and installs nothing)"
    ;;

  *) echo "unknown scenario: $SCENARIO" >&2; exit 2 ;;
esac

echo "SCENARIO-OK: $SCENARIO"
