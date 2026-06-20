#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2025 Atay Özcan <atay@oezcan.me>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Runs INSIDE a throwaway container (see scripts/test-install-container.sh).
# Stages the pre-built release artifacts, then exercises install.sh /
# uninstall.sh for one scenario (argv[1]) with loud assertions.
#
# The repo is bind-mounted read-only at /src; we copy just what the
# installer needs into /work (NOT target/, which is huge) and run with
# SENTINEL_SKIP_BUILD=1. A fake non-root invoking user (SUDO_UID) is set so
# the polkit-admin-rule and systemd-activation paths are exercised; the
# systemd --user activation itself no-ops in a container (no user bus) —
# the installer handles that gracefully.

set -uo pipefail

SCENARIO="${1:?scenario name required}"

fail() { echo "  ASSERT FAIL: $*" >&2; exit 1; }
assert_file()   { [[ -f "$1" ]] || fail "expected regular file: $1"; }
assert_exe()    { [[ -x "$1" ]] || fail "expected executable: $1"; }
assert_absent() { [[ ! -e "$1" ]] || fail "expected absent: $1"; }
assert_mode()   { local m; m=$(stat -c %a "$1" 2>/dev/null) || fail "stat $1"; [[ "$m" == "$2" ]] || fail "$1 mode $m != $2"; }
assert_owner0() { local o; o=$(stat -c %u:%g "$1" 2>/dev/null) || fail "stat $1"; [[ "$o" == "0:0" ]] || fail "$1 owner $o != 0:0"; }
assert_grep()   { grep -q -- "$2" "$1" || fail "no /$2/ in $1"; }

# Default install paths.
PAM_SO=/usr/lib64/security/pam_sentinel.so   # openSUSE multilib PAM dir (where pam_unix.so lives)
AGENT=/usr/lib/sentinel-polkit-agent
HELPER=/usr/lib/sentinel-helper-kde
CONF=/etc/security/sentinel.conf
POLKIT1=/etc/pam.d/polkit-1
USERUNIT=/usr/lib/systemd/user/sentinel-polkit-agent.service
DBUSCONF=/usr/share/dbus-1/system.d/org.sentinel.Agent.conf
RULE=/etc/polkit-1/rules.d/49-sentinel-admin.rules
STATE=/var/lib/sentinel/install.state
TESTER=tester

stage() {
    mkdir -p /work/target/release
    cp -a /src/install.sh /src/uninstall.sh /src/Cargo.toml /work/
    cp -a /src/config /src/packaging /work/
    cp -a /src/target/release/libpam_sentinel.so \
          /src/target/release/sentinel-polkit-agent \
          /src/target/release/sentinel-helper-kde /work/target/release/
    cd /work
    id "$TESTER" >/dev/null 2>&1 || useradd -m -u 4242 "$TESTER" 2>/dev/null || true
    export SENTINEL_SKIP_BUILD=1
    # Pretend a normal user ran `sudo ./install.sh` so BUILD_USER resolves.
    export SUDO_UID=4242 SUDO_USER="$TESTER"
    # The minimal test image has no polkit package; wire_pam_service reads the
    # vendor stack from /usr/lib/pam.d, so synthesize a polkit-1 there (the
    # common-* includes already exist via the pam base package).
    mkdir -p /usr/lib/pam.d
    cat > /usr/lib/pam.d/polkit-1 <<'PAMEOF'
#%PAM-1.0
auth       include      common-auth
account    include      common-account
password   include      common-password
session    include      common-session
PAMEOF
}

assert_installed() {
    assert_exe  "$HELPER";  assert_mode "$HELPER" 755;  assert_owner0 "$HELPER"
    assert_exe  "$AGENT";   assert_mode "$AGENT" 755;   assert_owner0 "$AGENT"
    assert_file "$PAM_SO";  assert_mode "$PAM_SO" 755;  assert_owner0 "$PAM_SO"
    assert_file "$CONF"
    assert_file "$POLKIT1"
    assert_grep "$POLKIT1" 'pam_sentinel\.so'   # actually wired in
    assert_grep "$POLKIT1" 'common-auth'         # vendor stack preserved → real password fallback
    assert_file "$USERUNIT"
    assert_file "$DBUSCONF"; assert_grep "$DBUSCONF" "org.sentinel.Agent"
    assert_file "$RULE";    assert_grep "$RULE" "unix-user:$TESTER"
    assert_file "$STATE"
}

case "$SCENARIO" in
  fresh)
    stage
    ./install.sh
    assert_installed
    echo "  OK: fresh install placed all files (root:root) + polkit admin rule for $TESTER"
    ;;

  replace)
    stage
    ./install.sh
    # Re-install over the existing one: revert_previous_install must kick in
    # and the result must be a clean single install, not a doubled state.
    ./install.sh
    assert_installed
    # state file must contain exactly one VERSION line (not two installs stacked)
    local_count=$(grep -c '^VERSION' "$STATE")
    [[ "$local_count" == "1" ]] || fail "state has $local_count VERSION lines, expected 1 (stale state not reverted)"
    echo "  OK: re-install reverted the prior install first (no orphaned/doubled state)"
    ;;

  uninstall)
    stage
    ./install.sh
    ./uninstall.sh -y
    assert_absent "$PAM_SO"
    assert_absent "$AGENT"
    assert_absent "$HELPER"
    assert_absent "$CONF"
    assert_absent "$USERUNIT"
    assert_absent "$DBUSCONF"
    assert_absent "$RULE"
    assert_absent "$STATE"
    echo "  OK: uninstall removed everything + restored pre-install state"
    ;;

  rollback)
    stage
    # Make the config dir a FILE so install -D fails at the sentinel.conf
    # step — AFTER the binaries are already installed → exercises rollback.
    rm -rf /etc/security
    : > /etc/security
    if ./install.sh; then fail "install should have failed"; fi
    assert_absent "$AGENT"       # rolled back (installed before the config step)
    assert_absent "$PAM_SO"      # rolled back
    assert_absent "$STATE"       # never committed
    echo "  OK: failed install rolled back cleanly (errtrace + ERR trap)"
    ;;

  fallback-uninstall)
    stage
    ./install.sh
    rm -f "$STATE"               # simulate lost state file
    ./uninstall.sh -y
    assert_absent "$PAM_SO"
    assert_absent "$AGENT"
    assert_absent "$RULE"
    echo "  OK: fallback uninstall (no state file) cleaned up known paths"
    ;;

  preexisting-config)
    stage
    install -Dm644 /dev/stdin "$POLKIT1" <<'ORIG'
# ORIGINAL-POLKIT1-STACK
auth include common-auth
ORIG
    ./install.sh
    assert_grep "${POLKIT1}.pre-sentinel.bak" 'ORIGINAL-POLKIT1-STACK'
    ./uninstall.sh -y
    assert_grep "$POLKIT1" 'ORIGINAL-POLKIT1-STACK'   # original restored verbatim
    echo "  OK: pre-existing /etc/pam.d/polkit-1 backed up + restored"
    ;;

  sudo-wiring)
    stage
    # Synthesize vendor sudo/su/sudo-i stacks; su carries pam_rootok, which
    # must stay BEFORE our line so root still su's without a prompt.
    cat > /usr/lib/pam.d/sudo <<'PAMEOF'
#%PAM-1.0
auth     include        common-auth
account  include        common-account
session  include        common-session
PAMEOF
    cat > /usr/lib/pam.d/su <<'PAMEOF'
#%PAM-1.0
auth      sufficient  pam_rootok.so
auth      include     common-auth
account   include     common-account
session   include     common-session
PAMEOF
    cp /usr/lib/pam.d/sudo /usr/lib/pam.d/sudo-i
    ./install.sh --enable-sudo
    for svc in sudo su sudo-i; do
        assert_file "/etc/pam.d/$svc"
        assert_grep "/etc/pam.d/$svc" 'pam_sentinel\.so'
    done
    assert_grep /etc/pam.d/su 'pam_rootok'   # preserved
    rootok_ln=$(grep -n pam_rootok /etc/pam.d/su | head -1 | cut -d: -f1)
    sent_ln=$(grep -n pam_sentinel /etc/pam.d/su | head -1 | cut -d: -f1)
    [[ -n "$rootok_ln" && -n "$sent_ln" && "$rootok_ln" -lt "$sent_ln" ]] \
        || fail "su: pam_sentinel must come AFTER pam_rootok (got rootok@$rootok_ln, sentinel@$sent_ln)"
    ./uninstall.sh -y
    for svc in sudo su sudo-i; do assert_absent "/etc/pam.d/$svc"; done   # vendor stack restored
    echo "  OK: --enable-sudo wired sudo/su/sudo-i (rootok order preserved) + clean uninstall"
    ;;

  err-nonroot)
    stage
    command -v runuser >/dev/null 2>&1 || { echo "  SKIP: no runuser"; echo "SCENARIO-OK: $SCENARIO"; exit 0; }
    chmod -R a+rx /work
    if runuser -u nobody -- ./install.sh 2>/dev/null; then fail "should refuse non-root"; fi
    assert_absent "$PAM_SO"
    echo "  OK: refuses to run as non-root"
    ;;

  err-badflag)
    stage
    if ./install.sh --totally-bogus 2>/dev/null; then fail "should reject unknown flag"; fi
    echo "  OK: rejects unknown flag"
    ;;

  *) echo "unknown scenario: $SCENARIO" >&2; exit 2 ;;
esac

echo "SCENARIO-OK: $SCENARIO"
