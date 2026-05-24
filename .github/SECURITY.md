<!-- SPDX-FileCopyrightText: 2025 Atay Özcan <atay@oezcan.me> -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Security Policy

Sentinel-KDE sits in the PAM authentication path, so security reports are taken
seriously and triaged promptly.

## Supported versions

Sentinel-KDE is pre-1.0. Only the latest `main` (and most recent tagged
release) is supported; please reproduce on current `main` before reporting.

## Reporting a vulnerability

**Please report privately — do not open a public issue.**

1. **Preferred:** GitHub Private Vulnerability Reporting — the **Security** tab
   → **Report a vulnerability**.
2. **Email:** `atay@oezcan.me`, subject "Sentinel security".

Please include: affected component (`pam-sentinel`, `sentinel-polkit-agent`,
`sentinel-helper-kde`), reproduction steps, and your environment (distro,
Plasma/polkit versions, SELinux mode). I'll acknowledge within a few days and
keep you updated through to a fix + disclosure.

## Trust boundaries

The threat model — what the PAM module trusts, what the agent refuses, the
D-Bus bypass hardening, and the SELinux posture — is documented in
[`docs/src/security.md`](../docs/src/security.md). "Same-uid" attacks are
explicitly out of scope: a process already running as you can drive polkit
directly; Sentinel is a UI confirmation, not a sandbox.
