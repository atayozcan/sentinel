# Security Policy

Sentinel sits in the PAM authentication path and ships a polkit
authentication agent. Vulnerabilities in either component can lead
to silent privilege escalation or auth bypass on user systems.
**Please report them privately, not via public issues.**

## Reporting a vulnerability

Use one of the following, in order of preference:

1. **GitHub Private Vulnerability Reporting** — preferred. From
   the [Security tab](https://github.com/atayozcan/sentinel/security)
   click *"Report a vulnerability"*. This sends an encrypted report
   only the maintainer can read and lets us coordinate a fix without
   public disclosure.

2. **Email** — `atay@oezcan.me`. Mention "Sentinel security" in
   the subject line. PGP key on request.

Please include:

- The Sentinel version (`sentinel-helper --version` /
  `pgrep -af sentinel-polkit-agent` / `pacman -Qi sentinel`)
- Distro + kernel + compositor (`uname -a`, `echo $XDG_CURRENT_DESKTOP`)
- Reproduction steps, ideally a minimal PAM stack snippet
- Whether the issue requires local user access, root, or is remote
- Your preferred attribution for the eventual advisory (real name,
  handle, or "anonymous reporter")

## Response timeline

For confirmed vulnerabilities affecting a released version:

| Severity | First response | Patch target |
|----------|----------------|--------------|
| Critical (auth bypass, silent privilege escalation, lock-out across reboots) | within 48 hours | within 7 days |
| High (PAM stack misbehavior, race exploitable by local user) | within 1 week | within 2 weeks |
| Medium / low | within 2 weeks | next minor release |

The maintainer is one person; complex issues may take longer.
You'll be kept in the loop.

## Disclosure

Once a fix is shipped:

- A GitHub Security Advisory is published from the [Security tab](https://github.com/atayozcan/sentinel/security/advisories).
- Release notes for the patched version mention the CVE / advisory
  ID (if assigned) and credit the reporter.
- The advisory is also pushed to the
  [RustSec Advisory Database](https://rustsec.org/) when it affects
  a published crate (currently none — Sentinel is bin/cdylib only).

## Supported versions

| Version | Supported |
|---------|-----------|
| 0.5.x   | ✓         |
| 0.4.x   | security fixes only, until 2026-12-31 |
| < 0.4   | unsupported (the v0.3.x line was yanked; upgrade to 0.5) |

## Out of scope

The following do **not** qualify as Sentinel vulnerabilities, even
though they may affect your security posture:

- A user with the same uid as the protected account can drive polkit
  directly. Sentinel is a UI confirmation, not a sandbox.
- A malicious compositor or process running as your user that injects
  events via `wlr_virtual_pointer` / `virtual_keyboard`. Realistic
  threat model is clickjacking apps; layer-shell + exclusive keyboard
  is sufficient. See the *"Why layer-shell instead of session-lock"*
  section in the [Architecture](https://github.com/atayozcan/sentinel/wiki/Architecture)
  wiki page.
- Issues in `sudo` / `polkit` / `pam_unix` / `polkit-agent-helper-1`
  themselves — please report those upstream.

If you're not sure whether something is in scope, send the report
anyway and we'll triage together.
