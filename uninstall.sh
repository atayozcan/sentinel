#!/usr/bin/env bash
# Sentinel uninstaller.
#
# Reads /var/lib/sentinel/install.state and walks each `CREATED` /
# `REPLACED` entry in reverse to restore the system to its exact
# pre-install state. Idempotent: safe to re-run, safe after a partial
# install. Falls back to best-effort path-based cleanup if the state
# file is missing.

set -euo pipefail

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; BLUE='\033[0;34m'; NC='\033[0m'
info()  { printf "${GREEN}[INFO]${NC} %s\n" "$*"; }
warn()  { printf "${YELLOW}[WARN]${NC} %s\n" "$*"; }
step()  { printf "${BLUE}[STEP]${NC} %s\n" "$*"; }
error() { printf "${RED}[ERROR]${NC} %s\n" "$*" >&2; exit 1; }

[[ $EUID -eq 0 ]] || error "Run as root (use pkexec or sudo)."

PREFIX=${PREFIX:-/usr}
SYSCONFDIR=${SYSCONFDIR:-/etc}
LIBEXECDIR=${LIBEXECDIR:-lib}

STATE_DIR="/var/lib/sentinel"
STATE_FILE="$STATE_DIR/install.state"

ASSUME_YES=0
for arg in "$@"; do
    [[ "$arg" == "-y" || "$arg" == "--yes" ]] && ASSUME_YES=1
done

if [[ -t 0 && -t 1 ]]; then
    read -r -p "Remove Sentinel? [y/N] " reply
    [[ "$reply" =~ ^[Yy]$ ]] || { info "Aborted."; exit 0; }
elif [[ $ASSUME_YES -eq 0 ]]; then
    error "Refusing to uninstall non-interactively. Pass -y/--yes to confirm."
fi

# -------------- stop running agent ----------------------------------------
#
# The agent binary is about to be deleted. Without stopping the running
# process: it stays alive (binary stays mapped) but any future polkit
# auth that triggers a fresh `polkit-agent-helper-1` -> `pam_sentinel.so`
# load will fail because the .so is gone. Cleaner to stop it now.
#
# We can also unlink any orphan bypass socket so the next install starts
# from a clean state.

stop_polkit_agent() {
    local uid="${PKEXEC_UID:-${SUDO_UID:-}}"
    if [[ -z "$uid" ]]; then
        warn "Could not identify invoking user; skipping agent stop."
        return 0
    fi
    if pgrep -u "$uid" -f sentinel-polkit-agent >/dev/null 2>&1; then
        step "Stopping running polkit agent…"
        pkill -TERM -u "$uid" -f sentinel-polkit-agent 2>/dev/null || true
        for _ in 1 2 3 4 5; do
            sleep 0.2
            pgrep -u "$uid" -f sentinel-polkit-agent >/dev/null 2>&1 || break
        done
        pkill -KILL -u "$uid" -f sentinel-polkit-agent 2>/dev/null || true
    fi
    rm -f -- "/run/user/$uid/sentinel-agent.sock"
}
stop_polkit_agent

# -------------- state-file driven uninstall --------------------------------

if [[ -f "$STATE_FILE" ]]; then
    step "Restoring pre-install state from $STATE_FILE…"
    failures=0
    while IFS=$'\t' read -r action path backup; do
        [[ -z "${action:-}" ]] && continue
        case "$action" in
            CREATED)
                if [[ -e "$path" ]]; then
                    rm -f -- "$path" && info "Removed $path" || { warn "Could not remove $path"; failures=$((failures+1)); }
                else
                    warn "Already gone: $path"
                fi
                ;;
            REPLACED)
                if [[ -n "${backup:-}" && -f "$backup" ]]; then
                    mv -f -- "$backup" "$path" && info "Restored $path from backup" || { warn "Could not restore $path"; failures=$((failures+1)); }
                else
                    warn "Backup missing for $path; leaving current file in place"
                fi
                ;;
            VERSION)
                # Informational; nothing to undo.
                ;;
            *)
                warn "Unknown state entry: $action $path"
                ;;
        esac
    done < <(tac "$STATE_FILE")

    systemctl daemon-reload 2>/dev/null || true
    rm -f -- "$STATE_FILE"
    rmdir --ignore-fail-on-non-empty "$STATE_DIR" 2>/dev/null || true

    if [[ $failures -gt 0 ]]; then
        warn "Uninstall finished with $failures non-fatal issue(s); see warnings above."
    else
        info "Sentinel uninstalled cleanly."
    fi
    exit 0
fi

# -------------- fallback (no state file) -----------------------------------

warn "No install state file at $STATE_FILE."
warn "Falling back to best-effort removal of known paths."

FALLBACK_PATHS=(
    "$PREFIX/lib/security/pam_sentinel.so"
    "$PREFIX/$LIBEXECDIR/sentinel-helper"
    "$PREFIX/$LIBEXECDIR/sentinel-polkit-agent"
    "$SYSCONFDIR/security/sentinel.conf"
    "$SYSCONFDIR/pam.d/polkit-1"
    "$SYSCONFDIR/xdg/autostart/sentinel-polkit-agent.desktop"
    "$SYSCONFDIR/systemd/system/polkit-agent-helper@.service.d/sentinel.conf"
    "$PREFIX/share/man/man1/sentinel-helper.1"
    "$PREFIX/share/man/man1/sentinel-polkit-agent.1"
    "$PREFIX/share/man/man5/sentinel.conf.5"
    "$PREFIX/share/man/man8/pam_sentinel.8"
    "$PREFIX/share/bash-completion/completions/sentinel-helper"
    "$PREFIX/share/bash-completion/completions/sentinel-polkit-agent"
    "$PREFIX/share/fish/vendor_completions.d/sentinel-helper.fish"
    "$PREFIX/share/fish/vendor_completions.d/sentinel-polkit-agent.fish"
    "$PREFIX/share/zsh/site-functions/_sentinel-helper"
    "$PREFIX/share/zsh/site-functions/_sentinel-polkit-agent"
)

for p in "${FALLBACK_PATHS[@]}"; do
    if [[ -e "$p" ]]; then
        if [[ -f "${p}.pre-sentinel.bak" ]]; then
            mv -f -- "${p}.pre-sentinel.bak" "$p" && info "Restored $p from backup"
        else
            rm -f -- "$p" && info "Removed $p"
        fi
    fi
done

# Sudo wiring was optional; restore the backup only if one exists.
if [[ -f "$SYSCONFDIR/pam.d/sudo.pre-sentinel.bak" ]]; then
    mv -f -- "$SYSCONFDIR/pam.d/sudo.pre-sentinel.bak" "$SYSCONFDIR/pam.d/sudo" \
        && info "Restored $SYSCONFDIR/pam.d/sudo from backup"
fi

systemctl daemon-reload 2>/dev/null || true
info "Sentinel uninstalled (fallback mode)."
