#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2025 Atay Özcan <atay@oezcan.me>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Host orchestrator: exercise install.sh / uninstall.sh inside throwaway
# openSUSE containers (podman), one fresh container per scenario, so the
# host's PAM stack is never touched. Requires pre-built RELEASE artifacts:
#
#   cargo build --release -p pam-sentinel -p sentinel-polkit-agent
#   cargo build --release -p sentinel-helper-kde     # for the KDE helper
#
# Usage: scripts/test-install-container.sh [scenario ...]

set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IMAGE="${SENTINEL_TEST_IMAGE:-registry.opensuse.org/opensuse/tumbleweed:latest}"
PODMAN="${PODMAN:-podman}"

command -v "$PODMAN" >/dev/null || { echo "podman is required"; exit 1; }
for a in libpam_sentinel.so sentinel-polkit-agent sentinel-helper-kde; do
    [[ -f "$REPO/target/release/$a" ]] \
        || { echo "missing target/release/$a — build the release artifacts first"; exit 1; }
done

ALL=(fresh idempotent uninstall rollback fallback-uninstall preexisting-config
     err-nonroot err-badflag err-nohelper)
SCENARIOS=("$@"); [[ ${#SCENARIOS[@]} -eq 0 ]] && SCENARIOS=("${ALL[@]}")

pass=0; fail=0; failed=()
for s in "${SCENARIOS[@]}"; do
    echo "════════════════════════════ $s ════════════════════════════"
    # label=disable: read the bind mount under SELinux without relabeling
    # the host repo. --network=none: the installer needs no network.
    if "$PODMAN" run --rm --network=none --security-opt label=disable \
        -v "$REPO":/src:ro "$IMAGE" \
        bash /src/scripts/test-install-incontainer.sh "$s"; then
        pass=$((pass + 1))
    else
        fail=$((fail + 1)); failed+=("$s")
    fi
done

echo "═══════════════════════════════════════════════════════════════"
echo "PASS=$pass  FAIL=$fail"
if [[ $fail -ne 0 ]]; then
    echo "FAILED SCENARIOS: ${failed[*]}"
    exit 1
fi
echo "ALL CONTAINER TESTS PASSED"
