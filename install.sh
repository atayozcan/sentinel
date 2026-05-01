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
                ENABLED)
                    local unit="${path#systemd:user:}"
                    if [[ -n "${BUILD_USER:-}" ]]; then
                        runuser -u "$BUILD_USER" -- systemctl --user disable --now "$unit" 2>/dev/null || true
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

# -------------- prior-version detection ------------------------------------

PRIOR_VERSION=""
if [[ -f "$STATE_FILE" ]]; then
    PRIOR_VERSION="$(awk -F'\t' '$1=="VERSION"{print $2; exit}' "$STATE_FILE" || true)"
    if [[ -n "$PRIOR_VERSION" ]]; then
        info "Detected prior install: $PRIOR_VERSION → 0.3.0"
    else
        info "Detected prior install (pre-0.3.0): assuming 0.2.x → 0.3.0"
    fi
fi

# -------------- prompts (before any system change) -------------------------

INSTALL_SUDO=0
ENABLE_SUDO_FLAG=0
ENABLE_AGENT_FLAG=""    # "" = default, "yes" or "no" overrides
for arg in "$@"; do
    case "$arg" in
        --enable-sudo)         ENABLE_SUDO_FLAG=1 ;;
        --enable-polkit-agent) ENABLE_AGENT_FLAG=yes ;;
        --no-polkit-agent)     ENABLE_AGENT_FLAG=no ;;
    esac
done
if [[ $ENABLE_SUDO_FLAG -eq 1 ]]; then
    INSTALL_SUDO=1
elif [[ -t 0 && -t 1 ]]; then
    read -r -p "Enable Sentinel for sudo / sudo-rs (/etc/pam.d/sudo)? [y/N] " reply
    [[ "$reply" =~ ^[Yy]$ ]] && INSTALL_SUDO=1
fi

# Polkit agent: enabled by default. Honor explicit --no-polkit-agent. On
# upgrade from 0.2.x, prompt interactively (the agent replaces cosmic-osd
# / polkit-gnome / polkit-kde via Conflicts= in the unit, which is a
# user-visible behaviour change).
ENABLE_AGENT=1
case "$ENABLE_AGENT_FLAG" in
    yes) ENABLE_AGENT=1 ;;
    no)  ENABLE_AGENT=0 ;;
    "")
        if [[ -n "$PRIOR_VERSION" && "$PRIOR_VERSION" =~ ^0\.2\. && -t 0 && -t 1 ]]; then
            warn "v0.3 ships a polkit authentication agent that replaces cosmic-osd /"
            warn "polkit-gnome / polkit-kde so polkit only has one UI to consult."
            read -r -p "Enable Sentinel polkit agent? [Y/n] " reply
            [[ "$reply" =~ ^[Nn]$ ]] && ENABLE_AGENT=0
        fi
        ;;
esac

# -------------- install ----------------------------------------------------

mkdir -p "$STATE_DIR"

# Record the version we're installing as the FIRST entry in the new state.
printf 'VERSION\t0.3.0\t\n' >> "$STATE_TMP"

step "Installing system files…"
install_file 755 target/release/sentinel-helper       "$PREFIX/$LIBEXECDIR/sentinel-helper"
install_file 755 target/release/sentinel-polkit-agent "$PREFIX/$LIBEXECDIR/sentinel-polkit-agent"
install_file 644 target/release/libpam_sentinel.so    "$PREFIX/lib/security/pam_sentinel.so"
install_file 644 config/sentinel.conf                 "$SYSCONFDIR/security/sentinel.conf"
install_file 644 config/polkit-1                      "$SYSCONFDIR/pam.d/polkit-1"
install_file 644 packaging/systemd/sentinel-polkit-agent.service \
    "$PREFIX/lib/systemd/user/sentinel-polkit-agent.service"

# XDG autostart is the canonical deployment for polkit auth agents (see
# polkit-gnome, polkit-kde). Compositors fork autostart entries as direct
# children, so the agent inherits the graphical session's kernel
# sessionid — which is what polkit's session_for_caller check requires.
# A systemctl --user unit cannot satisfy that check (its parent is
# user@1000.service, kernel sessionid != graphical session).
if [[ $ENABLE_AGENT -eq 1 ]]; then
    install_file 644 packaging/xdg-autostart/sentinel-polkit-agent.desktop \
        "$SYSCONFDIR/xdg/autostart/sentinel-polkit-agent.desktop"
fi

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
verify "$PREFIX/$LIBEXECDIR/sentinel-helper"            755 exe
verify "$PREFIX/$LIBEXECDIR/sentinel-polkit-agent"      755 exe
verify "$PREFIX/lib/security/pam_sentinel.so"           644 regular
verify "$SYSCONFDIR/security/sentinel.conf"             644 regular
verify "$SYSCONFDIR/pam.d/polkit-1"                     644 regular
verify "$PREFIX/lib/systemd/user/sentinel-polkit-agent.service"  644 regular
[[ $ENABLE_AGENT -eq 1 ]] && verify "$SYSCONFDIR/xdg/autostart/sentinel-polkit-agent.desktop" 644 regular
[[ $INSTALL_SUDO -eq 1 ]] && verify "$SYSCONFDIR/pam.d/sudo"     644 regular

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
  $PREFIX/$LIBEXECDIR/sentinel-polkit-agent
  $PREFIX/lib/systemd/user/sentinel-polkit-agent.service
  $SYSCONFDIR/security/sentinel.conf
  $SYSCONFDIR/pam.d/polkit-1$([[ $INSTALL_SUDO -eq 1 ]] && printf '\n  %s' "$SYSCONFDIR/pam.d/sudo")

State file:
  $STATE_FILE   (version 0.3.0)

Test the helper directly:
  $PREFIX/$LIBEXECDIR/sentinel-helper --timeout 10 --randomize

Polkit agent:$([[ $ENABLE_AGENT -eq 1 ]] && printf '\n  Installed as XDG autostart entry. Will launch at next graphical session start.\n  Log out and log back in to activate. Once active, polkit auth flows through Sentinel.' || printf '\n  Not installed (--no-polkit-agent). Drop config/sudo or polkit-1 references manually if needed.')

To remove: pkexec ./uninstall.sh
EOF
