#!/usr/bin/env bash
# Sentinel installer (source build).
#
# Transactional: every change is recorded in /var/lib/sentinel/install.state
# and any error rolls back to the pre-install state.
#
# For a packaged install on Arch / Debian / Fedora / NixOS, prefer the
# distribution package — they ship the same files plus shell completions
# and man pages.
#
# Flags:
#   --enable-sudo   Also wire pam_sentinel into /etc/pam.d/sudo. Default off.
#                   (Distribution packages never touch /etc/pam.d/sudo;
#                   silently rewriting it is a notorious foot-gun.)

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
STATE_TMP="$(mktemp "${STATE_DIR%/}.install.XXXXXX" 2>/dev/null || mktemp)"
ROLLBACK_LOG="$(mktemp)"
INSTALL_OK=0

# -------------- argv -------------------------------------------------------

INSTALL_SUDO=0
for arg in "$@"; do
    case "$arg" in
        --enable-sudo) INSTALL_SUDO=1 ;;
        --help|-h)
            sed -n '2,/^$/p' "${BASH_SOURCE[0]}" | sed 's/^# \?//'
            exit 0
            ;;
        *) error "Unknown flag: $arg (try --help)" ;;
    esac
done

# -------------- rollback ---------------------------------------------------

rollback() {
    local rc=$?
    if [[ $INSTALL_OK -eq 1 ]]; then
        return
    fi
    warn "Install failed (exit $rc). Rolling back…"
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

# install_file <mode> <src> <dst>
# Records pre-install state, copies, logs for rollback + uninstall.
install_file() {
    local mode="$1" src="$2" dst="$3"
    local backup=""
    mkdir -p "$(dirname "$dst")"
    if [[ -e "$dst" ]]; then
        backup="${dst}.pre-sentinel.bak"
        # Don't clobber an existing backup — that's the *real* original.
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
# target/ cache stays user-owned.
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

[[ -f target/release/sentinel-helper       ]] || error "Build artifact missing: target/release/sentinel-helper"
[[ -f target/release/sentinel-polkit-agent ]] || error "Build artifact missing: target/release/sentinel-polkit-agent"
[[ -f target/release/libpam_sentinel.so    ]] || error "Build artifact missing: target/release/libpam_sentinel.so"

# -------------- install ----------------------------------------------------

mkdir -p "$STATE_DIR"
printf 'VERSION\t%s\t\n' "$(awk -F'"' '/^version/{print $2; exit}' Cargo.toml)" >> "$STATE_TMP"

step "Installing system files…"

# Binaries.
install_file 755 target/release/sentinel-helper       "$PREFIX/$LIBEXECDIR/sentinel-helper"
install_file 755 target/release/sentinel-polkit-agent "$PREFIX/$LIBEXECDIR/sentinel-polkit-agent"

# pam_sentinel.so requires the execute bit (0755) — under
# polkit-agent-helper@.service's sandbox (NoNewPrivileges + various
# Protect*), libpam refuses to dlopen .so files without it.
install_file 755 target/release/libpam_sentinel.so    "$PREFIX/lib/security/pam_sentinel.so"

# Configs.
install_file 644 config/sentinel.conf                 "$SYSCONFDIR/security/sentinel.conf"
install_file 644 config/polkit-1                      "$SYSCONFDIR/pam.d/polkit-1"

# XDG autostart entry — the polkit agent must be a child of the
# compositor (not user@.service) for polkit's session-equality check
# to pass. Compositors fork autostart entries as direct children.
install_file 644 packaging/xdg-autostart/sentinel-polkit-agent.desktop \
    "$SYSCONFDIR/xdg/autostart/sentinel-polkit-agent.desktop"

# Drop-in disabling ProtectHome=yes on the system polkit-agent-helper@
# unit. Without this, /run/user/<uid> is masked inside helper-1's
# sandbox and pam_sentinel.so can't reach the agent's bypass socket.
install_file 644 packaging/systemd/polkit-agent-helper@.service.d/sentinel.conf \
    "$SYSCONFDIR/systemd/system/polkit-agent-helper@.service.d/sentinel.conf"

# Optional /etc/pam.d/sudo. Off by default; opt in with --enable-sudo.
if [[ $INSTALL_SUDO -eq 1 ]]; then
    install_file 644 config/sudo                       "$SYSCONFDIR/pam.d/sudo"
fi

# -------------- shell completions + man pages -----------------------------

step "Generating shell completions and man pages…"
GEN_DIR="$(mktemp -d)"
target/release/sentinel-helper       completions bash > "$GEN_DIR/sentinel-helper.bash"
target/release/sentinel-helper       completions fish > "$GEN_DIR/sentinel-helper.fish"
target/release/sentinel-helper       completions zsh  > "$GEN_DIR/_sentinel-helper"
target/release/sentinel-polkit-agent completions bash > "$GEN_DIR/sentinel-polkit-agent.bash"
target/release/sentinel-polkit-agent completions fish > "$GEN_DIR/sentinel-polkit-agent.fish"
target/release/sentinel-polkit-agent completions zsh  > "$GEN_DIR/_sentinel-polkit-agent"
target/release/sentinel-helper       man              > "$GEN_DIR/sentinel-helper.1"
target/release/sentinel-polkit-agent man              > "$GEN_DIR/sentinel-polkit-agent.1"

install_file 644 "$GEN_DIR/sentinel-helper.bash"        "$PREFIX/share/bash-completion/completions/sentinel-helper"
install_file 644 "$GEN_DIR/sentinel-polkit-agent.bash"  "$PREFIX/share/bash-completion/completions/sentinel-polkit-agent"
install_file 644 "$GEN_DIR/sentinel-helper.fish"        "$PREFIX/share/fish/vendor_completions.d/sentinel-helper.fish"
install_file 644 "$GEN_DIR/sentinel-polkit-agent.fish"  "$PREFIX/share/fish/vendor_completions.d/sentinel-polkit-agent.fish"
install_file 644 "$GEN_DIR/_sentinel-helper"            "$PREFIX/share/zsh/site-functions/_sentinel-helper"
install_file 644 "$GEN_DIR/_sentinel-polkit-agent"      "$PREFIX/share/zsh/site-functions/_sentinel-polkit-agent"
install_file 644 "$GEN_DIR/sentinel-helper.1"           "$PREFIX/share/man/man1/sentinel-helper.1"
install_file 644 "$GEN_DIR/sentinel-polkit-agent.1"     "$PREFIX/share/man/man1/sentinel-polkit-agent.1"
install_file 644 packaging/man/sentinel.conf.5          "$PREFIX/share/man/man5/sentinel.conf.5"
install_file 644 packaging/man/pam_sentinel.8           "$PREFIX/share/man/man8/pam_sentinel.8"

rm -rf "$GEN_DIR"

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
verify "$PREFIX/lib/security/pam_sentinel.so"           755 regular
verify "$SYSCONFDIR/security/sentinel.conf"             644 regular
verify "$SYSCONFDIR/pam.d/polkit-1"                     644 regular
verify "$SYSCONFDIR/xdg/autostart/sentinel-polkit-agent.desktop" 644 regular
verify "$SYSCONFDIR/systemd/system/polkit-agent-helper@.service.d/sentinel.conf" 644 regular
verify "$PREFIX/share/man/man1/sentinel-helper.1"               644 regular
verify "$PREFIX/share/man/man1/sentinel-polkit-agent.1"         644 regular
verify "$PREFIX/share/man/man5/sentinel.conf.5"                 644 regular
verify "$PREFIX/share/man/man8/pam_sentinel.8"                  644 regular
verify "$PREFIX/share/bash-completion/completions/sentinel-helper"       644 regular
verify "$PREFIX/share/zsh/site-functions/_sentinel-helper"               644 regular
verify "$PREFIX/share/fish/vendor_completions.d/sentinel-helper.fish"    644 regular
[[ $INSTALL_SUDO -eq 1 ]] && verify "$SYSCONFDIR/pam.d/sudo" 644 regular

# Reload systemd so the drop-in is picked up before the next
# polkit-agent-helper@ instance starts.
systemctl daemon-reload 2>/dev/null || true

# -------------- commit -----------------------------------------------------

mv -f -- "$STATE_TMP" "$STATE_FILE"
chmod 644 "$STATE_FILE"
rm -f -- "$ROLLBACK_LOG"
INSTALL_OK=1

# -------------- restart polkit agent in place ------------------------------
#
# Without this the user has to log out and back in for the new binary
# to take effect (XDG autostart only fires at session start; there's no
# supervisor that respawns the agent on demand). We do better:
#
#   1. Identify the invoking user + their compositor.
#   2. Sanity-check that this script's audit sessionid matches the
#      compositor's — that's the only sessionid the new agent will
#      inherit, and polkit's RegisterAuthenticationAgent rejects any
#      mismatch with "Passed session and the session the caller is
#      in differs". When the install was kicked off from a terminal
#      that's a descendant of the compositor, this matches naturally
#      (audit sessionid is set at login by setloginuid and inherited
#      through forks, including across the pkexec boundary).
#   3. Kill any running agent owned by that user, then re-spawn the
#      freshly-installed binary as that user with just enough env
#      copied from the live compositor that it can paint a dialog.
#
# If any check fails, fall through with a warning — the installed
# binary will still take over at next graphical login.
#
# Note: this depends on the install being kicked off from a graphical
# session (the typical `pkexec ./install.sh` flow). When running via
# automation or a TTY console session, the sessionid check fails and
# we skip the restart.

restart_polkit_agent() {
    local uid="${PKEXEC_UID:-${SUDO_UID:-}}"
    local user="${BUILD_USER:-}"
    if [[ -z "$user" || -z "$uid" ]]; then
        warn "No invoking user; agent will respawn at next graphical login."
        return 0
    fi

    local comp_pid=""
    for c in cosmic-comp Hyprland sway wayfire kwin_wayland; do
        comp_pid=$(pgrep -u "$uid" -x "$c" 2>/dev/null | head -1)
        [[ -n "$comp_pid" ]] && break
    done
    if [[ -z "$comp_pid" ]]; then
        warn "No supported compositor running for $user; agent will respawn at next graphical login."
        return 0
    fi

    local our_sid comp_sid
    our_sid=$(cat /proc/self/sessionid 2>/dev/null || echo 0)
    comp_sid=$(cat "/proc/$comp_pid/sessionid" 2>/dev/null || echo 1)
    if [[ "$our_sid" != "$comp_sid" ]]; then
        warn "Cannot restart agent in place — installer sessionid ($our_sid) ≠ compositor sessionid ($comp_sid)."
        warn "Re-run from a terminal opened inside the compositor's session, or relog to activate."
        return 0
    fi

    if pgrep -u "$uid" -f sentinel-polkit-agent >/dev/null 2>&1; then
        step "Stopping running polkit agent…"
        pkill -TERM -u "$uid" -f sentinel-polkit-agent 2>/dev/null || true
        for _ in 1 2 3 4 5; do
            sleep 0.2
            pgrep -u "$uid" -f sentinel-polkit-agent >/dev/null 2>&1 || break
        done
        # Force-kill any stragglers that ignored SIGTERM.
        pkill -KILL -u "$uid" -f sentinel-polkit-agent 2>/dev/null || true
        # Clean any stale bypass socket the dying agent may have left.
        rm -f -- "/run/user/$uid/sentinel-agent.sock"
    fi

    # Pull just the env vars the agent + helper need from the live
    # compositor. /proc/<pid>/environ is NUL-separated.
    local runtime_dir="/run/user/$uid"
    local wayland_disp
    wayland_disp=$(tr '\0' '\n' < "/proc/$comp_pid/environ" 2>/dev/null \
        | sed -n 's/^WAYLAND_DISPLAY=//p' | head -1)
    if [[ -z "$wayland_disp" ]]; then
        wayland_disp=$(ls "$runtime_dir"/wayland-* 2>/dev/null \
            | grep -v '\.lock' | head -1 | xargs -n1 basename 2>/dev/null)
    fi
    local xdg_session_id
    xdg_session_id=$(tr '\0' '\n' < "/proc/$comp_pid/environ" 2>/dev/null \
        | sed -n 's/^XDG_SESSION_ID=//p' | head -1)

    step "Starting freshly-installed polkit agent as $user…"
    # Spawn fully detached:
    #   - `setsid -f` puts the agent in its own session and forks, so
    #     this install script returns immediately.
    #   - `setpriv` (instead of runuser) drops uid/gid without opening
    #     a PAM session, so no parent process needs to stick around in
    #     wait() to perform PAM session cleanup.
    # Output is /dev/null because the install script will exit shortly
    # and we don't want the agent inheriting a doomed terminal.
    setsid -f setpriv \
        --reuid="$user" --regid="$user" --init-groups \
        --reset-env \
        -- env \
        HOME="$(getent passwd "$user" | cut -d: -f6)" \
        USER="$user" LOGNAME="$user" \
        PATH="/usr/local/bin:/usr/bin:/bin" \
        XDG_RUNTIME_DIR="$runtime_dir" \
        DBUS_SESSION_BUS_ADDRESS="unix:path=$runtime_dir/bus" \
        ${wayland_disp:+WAYLAND_DISPLAY="$wayland_disp"} \
        ${xdg_session_id:+XDG_SESSION_ID="$xdg_session_id"} \
        "$PREFIX/$LIBEXECDIR/sentinel-polkit-agent" \
        </dev/null >/dev/null 2>&1

    # Wait briefly for the agent to bind its socket — that's the
    # closest proxy for "successfully started".
    local agent_ok=0
    for _ in 1 2 3 4 5 6 7 8 9 10; do
        sleep 0.2
        if [[ -S "/run/user/$uid/sentinel-agent.sock" ]] \
            && pgrep -u "$uid" -f sentinel-polkit-agent >/dev/null 2>&1; then
            agent_ok=1
            break
        fi
    done

    if [[ $agent_ok -eq 1 ]]; then
        info "Polkit agent restarted in place (no relogin required)."
        AGENT_RESTARTED=1
    else
        warn "Polkit agent did not bind its socket within 2 s; will retry at next login."
    fi
}

AGENT_RESTARTED=0
restart_polkit_agent

info "Installation complete."
cat <<EOF

Installed:
  $PREFIX/lib/security/pam_sentinel.so
  $PREFIX/$LIBEXECDIR/sentinel-helper
  $PREFIX/$LIBEXECDIR/sentinel-polkit-agent
  $SYSCONFDIR/security/sentinel.conf
  $SYSCONFDIR/pam.d/polkit-1
  $SYSCONFDIR/xdg/autostart/sentinel-polkit-agent.desktop
  $SYSCONFDIR/systemd/system/polkit-agent-helper@.service.d/sentinel.conf$([[ $INSTALL_SUDO -eq 1 ]] && printf '\n  %s' "$SYSCONFDIR/pam.d/sudo")

State file: $STATE_FILE
EOF

if [[ $AGENT_RESTARTED -eq 1 ]]; then
    cat <<EOF

The polkit agent is already running the new build. Verify with:

  pgrep -af sentinel-polkit-agent
  pkexec true                          # exactly one Sentinel dialog

To remove: pkexec ./uninstall.sh
EOF
else
    cat <<EOF

The polkit agent autostarts at next graphical login. Log out and back
in to activate it. Once active:

  pgrep -af sentinel-polkit-agent     # should show the running agent
  pkexec true                          # exactly one Sentinel dialog

To remove: pkexec ./uninstall.sh
EOF
fi
