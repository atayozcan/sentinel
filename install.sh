#!/usr/bin/env bash
# Sentinel installer (source build).
# Transactional: every change is recorded, and any error rolls back to the
# pre-install state. Idempotent: re-running upgrades cleanly.
#
# For Arch users: prefer the AUR PKGBUILD in packaging/arch/.

set -euo pipefail

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; BLUE='\033[0;34m'; NC='\033[0m'
info()  { printf "${GREEN}[INFO]${NC} %s\n" "$*"; }
warn()  { printf "${YELLOW}[WARN]${NC} %s\n" "$*"; }
step()  { printf "${BLUE}[STEP]${NC} %s\n" "$*"; }
error() { printf "${RED}[ERROR]${NC} %s\n" "$*" >&2; exit 1; }

[[ $EUID -eq 0 ]] || error "Run as root (use pkexec or sudo)."

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

PREFIX=${PREFIX:-/usr}
SYSCONFDIR=${SYSCONFDIR:-/etc}
LIBEXECDIR=${LIBEXECDIR:-lib}

STATE_DIR="/var/lib/sentinel"
STATE_FILE="$STATE_DIR/install.state"
# New state being built up by this run; promoted into place on success.
STATE_TMP="$(mktemp "${STATE_DIR%/}.install.XXXXXX" 2>/dev/null || mktemp)"
# Tracks files we've modified during *this run*, for rollback on failure.
ROLLBACK_LOG="$(mktemp)"
INSTALL_OK=0

# -------------- rollback ---------------------------------------------------

rollback() {
    local rc=$?
    if [[ $INSTALL_OK -eq 1 ]]; then
        return
    fi
    warn "Install failed (exit $rc). Rolling back…"
    # Walk rollback log in reverse: each line is "ACTION\tPATH[\tBACKUP]".
    if [[ -s "$ROLLBACK_LOG" ]]; then
        tac "$ROLLBACK_LOG" | while IFS=$'\t' read -r action path backup; do
            case "$action" in
                CREATED)
                    rm -f -- "$path" || true
                    ;;
                REPLACED)
                    if [[ -n "$backup" && -f "$backup" ]]; then
                        mv -f -- "$backup" "$path" || true
                    fi
                    ;;
            esac
        done
    fi
    rm -f -- "$STATE_TMP" "$ROLLBACK_LOG"
    error "Rollback complete. System restored to pre-install state."
}
trap rollback ERR INT TERM

# -------------- helpers ----------------------------------------------------

# install_file <mode> <src> <dst>
# Records pre-install state, copies, and logs for both rollback and uninstall.
install_file() {
    local mode="$1" src="$2" dst="$3"
    local backup=""
    mkdir -p "$(dirname "$dst")"
    if [[ -e "$dst" ]]; then
        backup="${dst}.pre-sentinel.bak"
        # Don't clobber an existing .pre-sentinel.bak from a previous install
        # — that one is the *real* original. If we're upgrading, the previous
        # install already saved the original; just leave it alone.
        if [[ ! -e "$backup" ]]; then
            cp -a -- "$dst" "$backup"
        fi
        printf 'REPLACED\t%s\t%s\n' "$dst" "$backup" >> "$ROLLBACK_LOG"
        printf 'REPLACED\t%s\t%s\n' "$dst" "$backup" >> "$STATE_TMP"
    else
        printf 'CREATED\t%s\t\n' "$dst" >> "$ROLLBACK_LOG"
        printf 'CREATED\t%s\t\n' "$dst" >> "$STATE_TMP"
    fi
    install -Dm"$mode" -- "$src" "$dst"
}

# -------------- build (as the invoking user, not root) ---------------------

# When invoked via pkexec / sudo, build as the original user so cargo's
# target/ cache stays user-owned. Falls back to root if no original user
# is detectable.
BUILD_USER=""
if [[ -n "${PKEXEC_UID:-}" ]]; then
    BUILD_USER="$(getent passwd "$PKEXEC_UID" | cut -d: -f1 || true)"
elif [[ -n "${SUDO_USER:-}" && "$SUDO_USER" != "root" ]]; then
    BUILD_USER="$SUDO_USER"
fi

step "Building (cargo --release)${BUILD_USER:+ as $BUILD_USER}…"
build_cmd=(env
    SENTINEL_PREFIX="$PREFIX"
    SENTINEL_SYSCONFDIR="$SYSCONFDIR"
    SENTINEL_LIBEXECDIR="$LIBEXECDIR"
    cargo build --release --workspace --locked)

if [[ -n "$BUILD_USER" ]] && command -v runuser >/dev/null 2>&1; then
    runuser -u "$BUILD_USER" -- "${build_cmd[@]}"
else
    "${build_cmd[@]}"
fi

[[ -f target/release/sentinel-helper      ]] || error "Build artifact missing: target/release/sentinel-helper"
[[ -f target/release/libpam_sentinel.so   ]] || error "Build artifact missing: target/release/libpam_sentinel.so"

# -------------- prompts (before any system change) -------------------------

INSTALL_SUDO=0
ENABLE_SUDO_FLAG=0
for arg in "$@"; do
    case "$arg" in
        --enable-sudo) ENABLE_SUDO_FLAG=1 ;;
    esac
done
if [[ $ENABLE_SUDO_FLAG -eq 1 ]]; then
    INSTALL_SUDO=1
elif [[ -t 0 && -t 1 ]]; then
    read -r -p "Enable Sentinel for sudo / sudo-rs (/etc/pam.d/sudo)? [y/N] " reply
    [[ "$reply" =~ ^[Yy]$ ]] && INSTALL_SUDO=1
fi

# -------------- install ----------------------------------------------------

mkdir -p "$STATE_DIR"

step "Installing system files…"
install_file 755 target/release/sentinel-helper       "$PREFIX/$LIBEXECDIR/sentinel-helper"
install_file 644 target/release/libpam_sentinel.so    "$PREFIX/lib/security/pam_sentinel.so"
install_file 644 config/sentinel.conf                 "$SYSCONFDIR/security/sentinel.conf"
install_file 644 config/polkit-1                      "$SYSCONFDIR/pam.d/polkit-1"

if [[ $INSTALL_SUDO -eq 1 ]]; then
    install_file 644 config/sudo                       "$SYSCONFDIR/pam.d/sudo"
fi

# -------------- verify -----------------------------------------------------

step "Verifying installed files…"
verify() {
    local path="$1" expected_mode="$2" expected_kind="$3"
    [[ -e "$path" ]] || error "Missing after install: $path"
    [[ "$expected_kind" == "exe" && ! -x "$path" ]] && error "Not executable: $path"
    [[ "$expected_kind" == "regular" && ! -f "$path" ]] && error "Not a regular file: $path"
    local actual_mode
    actual_mode="$(stat -c '%a' "$path" 2>/dev/null || echo "?")"
    if [[ "$actual_mode" != "$expected_mode" ]]; then
        error "Wrong mode on $path: got $actual_mode, expected $expected_mode"
    fi
    local owner
    owner="$(stat -c '%u:%g' "$path" 2>/dev/null || echo "?:?")"
    [[ "$owner" == "0:0" ]] || error "Wrong ownership on $path: got $owner, expected 0:0 (root:root)"
}
verify "$PREFIX/$LIBEXECDIR/sentinel-helper"        755 exe
verify "$PREFIX/lib/security/pam_sentinel.so"       644 regular
verify "$SYSCONFDIR/security/sentinel.conf"         644 regular
verify "$SYSCONFDIR/pam.d/polkit-1"                 644 regular
[[ $INSTALL_SUDO -eq 1 ]] && verify "$SYSCONFDIR/pam.d/sudo" 644 regular

# -------------- commit -----------------------------------------------------

# Atomic state-file replacement.
mv -f -- "$STATE_TMP" "$STATE_FILE"
chmod 644 "$STATE_FILE"
rm -f -- "$ROLLBACK_LOG"
INSTALL_OK=1

info "Installation complete."
cat <<EOF

Installed:
  $PREFIX/lib/security/pam_sentinel.so
  $PREFIX/$LIBEXECDIR/sentinel-helper
  $SYSCONFDIR/security/sentinel.conf
  $SYSCONFDIR/pam.d/polkit-1$([[ $INSTALL_SUDO -eq 1 ]] && printf '\n  %s' "$SYSCONFDIR/pam.d/sudo")

State file:
  $STATE_FILE

Test the helper directly:
  $PREFIX/$LIBEXECDIR/sentinel-helper --timeout 10 --randomize

To remove: pkexec ./uninstall.sh
EOF
