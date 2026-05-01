#!/usr/bin/env bash
# Sentinel uninstaller.
# Reads /var/lib/sentinel/install.state to restore the system to its exact
# pre-install state. Idempotent: safe to re-run, safe to run after a partial
# install.

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

# Default fallback paths (used only when state file is missing — best-effort
# removal so a half-broken install can still be cleaned up).
FALLBACK_PATHS=(
    "$PREFIX/lib/security/pam_sentinel.so"
    "$PREFIX/$LIBEXECDIR/sentinel-helper"
    "$PREFIX/$LIBEXECDIR/sentinel-polkit-agent"
    "$PREFIX/lib/systemd/user/sentinel-polkit-agent.service"
    "$SYSCONFDIR/security/sentinel.conf"
    "$SYSCONFDIR/pam.d/polkit-1"
)

# Identify the invoking user so we can run `systemctl --user` for them.
BUILD_USER=""
if [[ -n "${PKEXEC_UID:-}" ]]; then
    BUILD_USER="$(getent passwd "$PKEXEC_UID" | cut -d: -f1 || true)"
elif [[ -n "${SUDO_USER:-}" && "$SUDO_USER" != "root" ]]; then
    BUILD_USER="$SUDO_USER"
fi

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

# -------------- state-file driven uninstall --------------------------------

if [[ -f "$STATE_FILE" ]]; then
    step "Restoring pre-install state from $STATE_FILE…"
    failures=0

    # First pass (forward order): handle ENABLED entries — disable systemd
    # --user units before removing their unit files in the reverse pass.
    while IFS=$'\t' read -r action target _; do
        [[ "${action:-}" == "ENABLED" ]] || continue
        unit="${target#systemd:user:}"
        if [[ -n "$BUILD_USER" ]]; then
            if runuser -u "$BUILD_USER" -- systemctl --user disable --now "$unit" 2>/dev/null; then
                info "Disabled $unit (--user, as $BUILD_USER)"
            else
                warn "Could not disable $unit; carrying on"
            fi
        else
            warn "No invoking user detected; skipping systemctl --user disable for $unit"
        fi
    done < "$STATE_FILE"

    # Second pass (reverse order): undo CREATED / REPLACED.
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
            ENABLED|VERSION)
                # Already handled above (ENABLED) or informational (VERSION).
                ;;
            *)
                warn "Unknown state entry: $action $path"
                ;;
        esac
    done < <(tac "$STATE_FILE")

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

for p in "${FALLBACK_PATHS[@]}"; do
    if [[ -e "$p" ]]; then
        # If a pre-sentinel backup exists, prefer restoring it.
        if [[ -f "${p}.pre-sentinel.bak" ]]; then
            mv -f -- "${p}.pre-sentinel.bak" "$p" && info "Restored $p from backup"
        else
            rm -f -- "$p" && info "Removed $p"
        fi
    fi
done

# Sudo file was optional; restore its backup only if one exists.
if [[ -f "$SYSCONFDIR/pam.d/sudo.pre-sentinel.bak" ]]; then
    mv -f -- "$SYSCONFDIR/pam.d/sudo.pre-sentinel.bak" "$SYSCONFDIR/pam.d/sudo" \
        && info "Restored $SYSCONFDIR/pam.d/sudo from backup"
fi

info "Sentinel uninstalled (fallback mode)."
