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

## Threat model

Sentinel has two trust boundaries, each with explicit assumptions:

### 1. The PAM module (`pam_sentinel.so`)

`pam_sentinel.so` is dlopen'd into whatever privileged binary's PAM
stack references it: typically `sudo` / `sudo-rs`, `su`, and
`polkit-agent-helper-1`. The host process is already root (or about to
become root) before our `.so` runs.

- **What we trust:** the host binary's PAM API contract (libpam),
  `/etc/security/sentinel.conf` being root-owned and not user-writable
  (otherwise an unprivileged user could lower their own `timeout = 0`
  and defeat the UAC contract), and the kernel `/proc` interface for
  identifying the requesting user via `loginuid`.

- **What we *don't* trust:** the host binary's environment (sudo and
  helper-1 scrub `LANG` / `LC_*` / `WAYLAND_DISPLAY`; we recover
  what's needed from the requesting user's `/proc/<pid>/environ` with
  per-variable allowlist + bounded length). User-supplied display
  names and locale strings are validated against tight character
  whitelists.

- **What Sentinel will refuse:** non-Wayland sessions (returns
  `headless_action`, default `PAM_IGNORE` to fall through to
  `pam_unix`). Failed dialog launches return `PAM_AUTH_ERR` (never
  silent allow on error).

### 2. The polkit agent (`sentinel-polkit-agent`)

The agent runs as the user inside their compositor's session. It
publishes the
`org.freedesktop.PolicyKit1.AuthenticationAgent` D-Bus interface and
owns the bypass socket at `$XDG_RUNTIME_DIR/sentinel-agent.sock`
(mode `0600`).

- **What we trust:** the user's compositor honoring layer-shell
  exclusive-keyboard semantics, polkit's session-equality check, and
  systemd's per-user `XDG_RUNTIME_DIR` (mode `0700`, owned by the user).

- **What we *don't* trust:** any peer connecting to the bypass socket.
  Each accept() runs `SO_PEERCRED` and refuses non-uid-0 peers; the
  `comm` of the peer must match `polkit-agent-helper-1` (or its
  kernel-truncated `polkit-agent-he`). Same-uid attackers can drive
  polkit directly anyway, so the bypass socket isn't a privilege
  boundary inside the user's session.

- **What Sentinel will refuse:** approval-queue cross-action
  correlation (each `CancelAuthentication` drains the queue, see
  `crates/sentinel-polkit-agent/src/agent.rs`). Approvals expire
  after 1 second so a stale approval can't be picked up by an
  unrelated auth that races in.

### Why we don't ship a `systemd --user` unit

A `systemd --user` unit would run the agent under
`user@<uid>.service`, which has a *different* kernel sessionid from
the user's compositor. Polkit's `RegisterAuthenticationAgent`
session-equality check rejects that mismatch with "Passed session and
the session the caller is in differs". Sentinel ships an XDG
autostart entry with `X-systemd-skip=true` so the systemd
xdg-autostart-generator doesn't wrap it; the compositor forks the
agent as a direct child, inheriting the right sessionid.

### Note: 2026 `polkit-agent-helper-1` SUID stripping

Upstream Arch and Debian are gradually removing the SUID bit from
`/usr/lib/polkit-1/polkit-agent-helper-1` in favour of socket
activation via `polkit-agent-helper@.service`. Sentinel's bypass
socket lives in `$XDG_RUNTIME_DIR` and doesn't depend on the SUID
model either way. The packaged systemd drop-in
(`packaging/systemd/polkit-agent-helper@.service.d/sentinel.conf`)
overrides `ProtectHome=yes` so `pam_sentinel.so` (running inside
helper-1's sandbox) can reach the bypass socket on socket-activated
distros.

### Supply-chain integrity

GitHub release assets (deb / rpm / tarball) are accompanied by
[Sigstore artifact attestations](https://github.blog/2024-05-02-introducing-artifact-attestations-now-in-public-beta/)
generated by the release pipeline. Verify with:

```bash
gh attestation verify <file> --repo atayozcan/sentinel
```

Downstream packagers (AUR `prepare()` hooks, Debian/Fedora build
farms) are encouraged to run this in their PKGBUILDs / spec files.

## Supported versions

| Version | Supported |
|---------|-----------|
| 0.7.x   | ✓ |
| 0.6.x   | security fixes only, until 2026-12-31 |
| < 0.6   | unsupported |

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
