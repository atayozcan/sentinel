#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2025 Atay Özcan <atay@oezcan.me>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Sentinel-KDE installer (source build).
#
# Wires the Sentinel PAM module + polkit agent + Kirigami helper into KDE
# Plasma's authentication path so privilege escalation shows a UAC-style
# confirmation dialog instead of a password prompt.
#
# Transactional: every change is recorded in /var/lib/sentinel/install.state
# and any error rolls back to the pre-install state.
#
# What it does that's Plasma-specific:
#   * Runs Sentinel's agent as a systemd *user* service (the only way it
#     registers cleanly with polkitd on Plasma 6 — see
#     packaging/systemd/user/sentinel-polkit-agent.service).
#   * Masks plasma-polkit-agent.service so Sentinel is the session's sole
#     polkit agent (reversed on uninstall).
#   * Installs a polkit admin rule making the install user a polkit
#     administrator — Sentinel's no-password model needs the logged-in
#     user to confirm their own escalation (UAC-style). root stays admin.
#
# Flags:
#   --enable-sudo   Also wire pam_sentinel into /etc/pam.d/sudo. Default off.
#                   (Silently rewriting /etc/pam.d/sudo is a foot-gun.)
#
# Env:
#   SENTINEL_SKIP_BUILD=1   Skip `cargo build`; install existing
#                           target/release artifacts (CI / container tests).

# -E (errtrace): so the ERR trap is inherited into functions and a failure
# inside install_file() rolls back instead of exiting silently.
set -Eeuo pipefail
umask 022

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
HELPER_PATH="$PREFIX/$LIBEXECDIR/sentinel-helper-kde"

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
        --help|-h) sed -n '2,/^$/p' "${BASH_SOURCE[0]}" | sed 's/^# \?//'; exit 0 ;;
        *) error "Unknown flag: $arg (try --help)" ;;
    esac
done

# -------------- invoking user (for build + systemd --user) -----------------

BUILD_USER=""; BUILD_UID=""
if [[ -n "${PKEXEC_UID:-}" ]]; then
    BUILD_UID="$PKEXEC_UID"
elif [[ -n "${SUDO_UID:-}" && "$SUDO_UID" != "0" ]]; then
    BUILD_UID="$SUDO_UID"
fi
[[ -n "$BUILD_UID" ]] && BUILD_USER="$(getent passwd "$BUILD_UID" | cut -d: -f1 || true)"

# -------------- rollback ---------------------------------------------------

rollback() {
    local rc=$?
    [[ $INSTALL_OK -eq 1 ]] && return
    warn "Install failed (exit $rc). Rolling back…"
    if [[ -s "$ROLLBACK_LOG" ]]; then
        tac "$ROLLBACK_LOG" | while IFS=$'\t' read -r action path backup; do
            case "$action" in
                CREATED)  rm -f -- "$path" || true ;;
                REPLACED) [[ -n "$backup" && -f "$backup" ]] && mv -f -- "$backup" "$path" || true ;;
            esac
        done
    fi
    rm -f -- "$STATE_TMP" "$ROLLBACK_LOG"
    error "Rollback complete. System restored to pre-install state."
}
trap rollback ERR INT TERM

# install_file <mode> <src> <dst> — record pre-state, copy, log for rollback + uninstall.
install_file() {
    local mode="$1" src="$2" dst="$3" backup=""
    mkdir -p "$(dirname "$dst")"
    if [[ -e "$dst" ]]; then
        backup="${dst}.pre-sentinel.bak"
        [[ ! -e "$backup" ]] && cp -a -- "$dst" "$backup"   # don't clobber the real original
        printf 'REPLACED\t%s\t%s\n' "$dst" "$backup" | tee -a "$ROLLBACK_LOG" >> "$STATE_TMP"
    else
        printf 'CREATED\t%s\t\n' "$dst" | tee -a "$ROLLBACK_LOG" >> "$STATE_TMP"
    fi
    install -Dm"$mode" -- "$src" "$dst"
}

# Run a `systemctl --user` command as the invoking user.
user_systemctl() {
    runuser -u "$BUILD_USER" -- env \
        XDG_RUNTIME_DIR="/run/user/$BUILD_UID" \
        DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/$BUILD_UID/bus" \
        systemctl --user "$@"
}

# Replace semantics: if a previous install is recorded, cleanly revert it
# first. This makes re-installs idempotent, repairs a broken/partial prior
# state, and means installing the new build over an old one (even the
# COSMIC `sentinel`) leaves nothing orphaned. Best-effort throughout — a
# missing backup or stale entry must never abort the fresh install.
revert_previous_install() {
    [[ -f "$STATE_FILE" ]] || return 0
    warn "Existing install detected — reverting it before reinstalling…"
    while IFS=$'\t' read -r action path backup; do
        [[ -z "${action:-}" ]] && continue
        case "$action" in
            AGENTUNIT)  [[ -n "$BUILD_USER" && -n "$BUILD_UID" ]] && { user_systemctl disable --now "$path" 2>/dev/null || true; } ;;
            PLASMAMASK) [[ -n "$BUILD_USER" && -n "$BUILD_UID" ]] && { user_systemctl unmask "$path" 2>/dev/null || true; user_systemctl start "$path" 2>/dev/null || true; } ;;
            CREATED)    rm -f -- "$path" 2>/dev/null || true ;;
            REPLACED)   [[ -n "${backup:-}" && -f "$backup" ]] && { mv -f -- "$backup" "$path" 2>/dev/null || true; } ;;
        esac
    done < <(tac "$STATE_FILE")
    rm -f -- "$STATE_FILE"
    systemctl daemon-reload 2>/dev/null || true
}
revert_previous_install

# -------------- build (as the invoking user, not root) ---------------------

BUILD_CRATES=(-p pam-sentinel -p sentinel-polkit-agent -p sentinel-helper-kde)

if [[ "${SENTINEL_SKIP_BUILD:-0}" == "1" ]]; then
    warn "SENTINEL_SKIP_BUILD=1 — using existing target/release artifacts."
else
    step "Building (cargo --release)${BUILD_USER:+ as $BUILD_USER}…"
    build_cmd=(env
        SENTINEL_PREFIX="$PREFIX" SENTINEL_SYSCONFDIR="$SYSCONFDIR"
        SENTINEL_LIBEXECDIR="$LIBEXECDIR" SENTINEL_HELPER_PATH="$HELPER_PATH"
        cargo build --release --locked "${BUILD_CRATES[@]}")
    if [[ -n "$BUILD_USER" ]] && command -v runuser >/dev/null 2>&1; then
        runuser -u "$BUILD_USER" -- "${build_cmd[@]}"
    else
        "${build_cmd[@]}"
    fi
fi

for a in libpam_sentinel.so sentinel-polkit-agent sentinel-helper-kde; do
    [[ -f "target/release/$a" ]] || error "Build artifact missing: target/release/$a"
done

# -------------- install ----------------------------------------------------

mkdir -p "$STATE_DIR"
printf 'VERSION\t%s\t\n' "$(sed -n 's/^version *= *"\([^"]*\)".*/\1/p' Cargo.toml | head -1)" >> "$STATE_TMP"

step "Installing system files…"
install_file 755 target/release/sentinel-helper-kde   "$HELPER_PATH"
install_file 755 target/release/sentinel-polkit-agent "$PREFIX/$LIBEXECDIR/sentinel-polkit-agent"
# pam_sentinel.so needs 0755 — under polkit-agent-helper@'s sandbox
# (NoNewPrivileges), libpam refuses to dlopen .so files without the x bit.
install_file 755 target/release/libpam_sentinel.so    "$PREFIX/lib/security/pam_sentinel.so"

install_file 644 config/sentinel.conf                 "$SYSCONFDIR/security/sentinel.conf"
install_file 644 config/polkit-1                       "$SYSCONFDIR/pam.d/polkit-1"

# systemd *user* unit for the agent (registers cleanly on Plasma 6).
install_file 644 packaging/systemd/user/sentinel-polkit-agent.service \
    "$PREFIX/lib/systemd/user/sentinel-polkit-agent.service"

# Drop-in disabling ProtectHome on polkit-agent-helper@ so pam_sentinel.so
# inside helper-1 can reach the agent's bypass socket in /run/user/<uid>.
install_file 644 packaging/systemd/polkit-agent-helper@.service.d/sentinel.conf \
    "$SYSCONFDIR/systemd/system/polkit-agent-helper@.service.d/sentinel.conf"

# Polkit admin rule: Sentinel's no-password model needs the logged-in user
# to be a polkit administrator (you confirm your own escalation). Without
# it, auth_admin actions (pkexec) authenticate root — whose session has no
# Sentinel agent — and the bypass can't be honored. root stays an admin.
if [[ -n "$BUILD_USER" ]]; then
    rule_tmp="$(mktemp)"
    cat > "$rule_tmp" <<RULE
// SPDX-License-Identifier: GPL-3.0-or-later
// Installed by Sentinel-KDE. Makes the logged-in user a polkit
// administrator so the no-password confirmation works for auth_admin
// actions (e.g. pkexec). root remains an administrator.
polkit.addAdminRule(function(action, subject) {
    return ["unix-user:$BUILD_USER", "unix-user:0"];
});
RULE
    install_file 644 "$rule_tmp" "$SYSCONFDIR/polkit-1/rules.d/49-sentinel-admin.rules"
    rm -f -- "$rule_tmp"
else
    warn "No invoking user — skipping the polkit admin rule. auth_admin (pkexec)"
    warn "will fall back to a password unless you add the rule yourself."
fi

# Optional /etc/pam.d/sudo.
[[ $INSTALL_SUDO -eq 1 ]] && install_file 644 config/sudo "$SYSCONFDIR/pam.d/sudo"

# Agent shell completions + man pages.
step "Generating completions + man pages…"
GEN_DIR="$(mktemp -d)"
target/release/sentinel-polkit-agent completions bash > "$GEN_DIR/c.bash"
target/release/sentinel-polkit-agent completions fish > "$GEN_DIR/c.fish"
target/release/sentinel-polkit-agent completions zsh  > "$GEN_DIR/_c"
target/release/sentinel-polkit-agent man              > "$GEN_DIR/c.1"
install_file 644 "$GEN_DIR/c.bash" "$PREFIX/share/bash-completion/completions/sentinel-polkit-agent"
install_file 644 "$GEN_DIR/c.fish" "$PREFIX/share/fish/vendor_completions.d/sentinel-polkit-agent.fish"
install_file 644 "$GEN_DIR/_c"     "$PREFIX/share/zsh/site-functions/_sentinel-polkit-agent"
install_file 644 "$GEN_DIR/c.1"    "$PREFIX/share/man/man1/sentinel-polkit-agent.1"
install_file 644 packaging/man/sentinel.conf.5 "$PREFIX/share/man/man5/sentinel.conf.5"
install_file 644 packaging/man/pam_sentinel.8  "$PREFIX/share/man/man8/pam_sentinel.8"
rm -rf "$GEN_DIR"

# -------------- verify -----------------------------------------------------

step "Verifying installed files…"
verify() {
    local path="$1" mode="$2" kind="$3" actual owner
    [[ -e "$path" ]] || error "Missing after install: $path"
    [[ "$kind" == exe && ! -x "$path" ]] && error "Not executable: $path"
    actual="$(stat -c '%a' "$path" 2>/dev/null || echo '?')"
    [[ "$actual" == "$mode" ]] || error "Wrong mode on $path: got $actual, want $mode"
    owner="$(stat -c '%u:%g' "$path" 2>/dev/null || echo '?:?')"
    [[ "$owner" == "0:0" ]] || error "Wrong ownership on $path: got $owner, want 0:0"
}
verify "$HELPER_PATH"                                   755 exe
verify "$PREFIX/$LIBEXECDIR/sentinel-polkit-agent"     755 exe
verify "$PREFIX/lib/security/pam_sentinel.so"          755 regular
verify "$SYSCONFDIR/security/sentinel.conf"            644 regular
verify "$SYSCONFDIR/pam.d/polkit-1"                    644 regular
verify "$PREFIX/lib/systemd/user/sentinel-polkit-agent.service" 644 regular
[[ -n "$BUILD_USER" ]] && verify "$SYSCONFDIR/polkit-1/rules.d/49-sentinel-admin.rules" 644 regular
[[ $INSTALL_SUDO -eq 1 ]] && verify "$SYSCONFDIR/pam.d/sudo" 644 regular

systemctl daemon-reload 2>/dev/null || true

# -------------- commit -----------------------------------------------------

mv -f -- "$STATE_TMP" "$STATE_FILE"
chmod 644 "$STATE_FILE"
rm -f -- "$ROLLBACK_LOG"
INSTALL_OK=1

# -------------- activate (systemd --user; reversible, post-commit) ----------

AGENT_OK=0
activate_agent() {
    if [[ -z "$BUILD_USER" || -z "$BUILD_UID" ]] || ! command -v runuser >/dev/null 2>&1; then
        warn "No invoking user; activate after login with:"
        warn "  systemctl --user mask --now plasma-polkit-agent.service"
        warn "  systemctl --user enable --now sentinel-polkit-agent.service"
        return 0
    fi
    if [[ ! -S "/run/user/$BUILD_UID/bus" ]]; then
        warn "No user D-Bus for $BUILD_USER; the agent activates at next login."
        return 0
    fi
    step "Activating Sentinel as the polkit agent (systemd --user)…"
    user_systemctl daemon-reload 2>/dev/null || true
    if user_systemctl mask --now plasma-polkit-agent.service 2>/dev/null; then
        printf 'PLASMAMASK\tplasma-polkit-agent.service\t%s\n' "$BUILD_UID" >> "$STATE_FILE"
    else
        warn "Could not mask plasma-polkit-agent.service (continuing)."
    fi
    local mark; mark=$(date '+%Y-%m-%d %H:%M:%S')
    if user_systemctl enable --now sentinel-polkit-agent.service 2>/dev/null; then
        printf 'AGENTUNIT\tsentinel-polkit-agent.service\t%s\n' "$BUILD_UID" >> "$STATE_FILE"
        for _ in $(seq 1 15); do
            sleep 0.2
            if journalctl -t sentinel-polkit-agent --since "$mark" --no-pager 2>/dev/null \
                 | grep -q "registered as polkit auth agent"; then AGENT_OK=1; break; fi
        done
    else
        warn "Could not enable/start the agent now; it activates at next login."
    fi
}
activate_agent || true

# -------------- done -------------------------------------------------------

info "Installation complete."
cat <<EOF

Installed:
  $PREFIX/lib/security/pam_sentinel.so
  $PREFIX/$LIBEXECDIR/sentinel-polkit-agent  (systemd --user service)
  $HELPER_PATH
  $SYSCONFDIR/pam.d/polkit-1
  $SYSCONFDIR/security/sentinel.conf
  ${BUILD_USER:+$SYSCONFDIR/polkit-1/rules.d/49-sentinel-admin.rules (admin: $BUILD_USER)}

State file: $STATE_FILE
EOF

if [[ $AGENT_OK -eq 1 ]]; then
    cat <<EOF

Sentinel is the active polkit agent. Verify:
  pkexec true        # one Sentinel dialog, Allow → no password
To remove: pkexec ./uninstall.sh   (or sudo ./uninstall.sh)
EOF
else
    cat <<EOF

Agent not confirmed active this session. Log out and back in (Plasma
starts it via the user service), then:
  pkexec true        # one Sentinel dialog, Allow → no password
To remove: sudo ./uninstall.sh
EOF
fi
