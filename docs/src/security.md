# Security policy

The full text lives at
[`.github/SECURITY.md`](https://github.com/atayozcan/sentinel/blob/main/.github/SECURITY.md)
in the repo root and is what GitHub renders on the
[security tab](https://github.com/atayozcan/sentinel/security).
This page is a brief summary; reporters should follow the canonical
copy.

## Reporting a vulnerability

1. **Preferred:** [GitHub Private Vulnerability Reporting](https://github.com/atayozcan/sentinel/security)
   ("Report a vulnerability" button).
2. **Email:** `atay@oezcan.me` with subject "Sentinel security".

Please don't open public issues for security bugs.

## Threat model summary

Sentinel has two trust boundaries:

1. **The PAM module** (`pam_sentinel.so`) runs in-process of whatever
   privileged binary's PAM stack references it — sudo, helper-1, su.
   It trusts libpam, root-owned `/etc/security/sentinel.conf`, and
   the kernel's `/proc/<pid>/loginuid`. It doesn't trust the host
   binary's environment (locale variables are recovered from the
   user's `/proc/<pid>/environ` against a strict allowlist).

2. **The polkit agent** runs as the user, owns the bypass socket at
   `$XDG_RUNTIME_DIR/sentinel-agent.sock` (mode `0600`). The bypass
   protocol verifies peer uid via `SO_PEERCRED` and the peer's
   `comm` against the kernel-truncated `polkit-agent-helper-1`.

Detailed threat model in `.github/SECURITY.md`.

## Supply-chain integrity

Every release artifact ships with a Sigstore artifact attestation
binding the file's sha256 to the GitHub Actions run that produced
it. Verify:

```bash
gh attestation verify <file> --repo atayozcan/sentinel
```

Downstream packagers (AUR, Debian, Fedora) are encouraged to run
this in their build hooks.

## Out of scope

- Same-uid attacks (a process running as your user can drive polkit
  directly; Sentinel is a UI confirmation, not a sandbox).
- Compositor / kernel issues themselves.
- Issues in upstream sudo, polkit, pam_unix, polkit-agent-helper-1.

When in doubt, send the report anyway and we'll triage.
