# Changelog

All notable changes per release. Detailed prose for each version lives
in `.github/release-notes/v*.md` and is mirrored on the GitHub
[Releases](https://github.com/atayozcan/sentinel/releases) page.

The format loosely follows
[Keep a Changelog](https://keepachangelog.com/), with version numbers
following [Semantic Versioning](https://semver.org/).

## [0.7.0] — 2026-05-04

Bigger release: dialog UX final piece + supply-chain integrity +
docs site + repo polish.

- **Dialog process names:** `sudo -v` (cred-cache, used by topgrade
  / paru) now walks up to PPid and shows the user-facing originator
  (paru, topgrade, your shell) instead of `sudo-rs`. Closes the
  dialog-process-name fix series begun in v0.6.1.
- **Sigstore artifact attestations:** every release artifact (deb,
  rpm, tarball, both arches) is signed via
  `actions/attest-build-provenance@v3`. Verify with
  `gh attestation verify <file> --repo atayozcan/sentinel`.
- **Threat model:** explicit section in SECURITY.md covering trust
  boundaries (PAM + agent), what each refuses, why no
  `systemd --user` unit, and the 2026 `polkit-agent-helper-1`
  SUID-stripping context.
- **Docs site:** wiki content migrated to `docs/` (mdBook),
  deployed to <https://atayozcan.github.io/sentinel/> by
  `.github/workflows/docs.yml`. PR-reviewable, versioned,
  searchable.
- **REUSE / SPDX compliance:** per-file headers across the repo +
  `REUSE.toml` + `reuse-action` CI job. New badge.
- **OpenSSF Scorecard:** weekly + push-triggered
  `.github/workflows/scorecard.yml`. New badge.
- **Compositor compatibility issue template:** structured
  YAML form feeding the README compat table.
- **Agent integration test:** `tests/agent_flow.rs` drives the
  Allow / Deny / Timeout / cancel-drain paths end-to-end with mock
  helper + mock helper-1 via env-var test seams. Agent crate now
  exposes a `[lib]` target.

[Full notes](https://github.com/atayozcan/sentinel/releases/tag/v0.7.0)

## [0.6.1] — 2026-05-04

Patch: dialog process names + AUR publish.

- **Dialog and audit logs now show the elevated program, not the
  wrapper**, for both sudo (PAM module path) and gparted-style
  apps that have their own polkit action but call pkexec
  internally. New shared `sentinel_shared::strip_elevation_prefix`
  helper recognises pkexec/sudo/sudo-rs/su/doas and strips both
  standalone flags and value-taking flags (`--user`/`-u`/etc).
- **AUR publish unblocked.** `KSXGitHub/github-actions-deploy-aur`
  bumped from v2.7.2 (broken against the 2026-04 archlinux:base
  with `==> ERROR: There is no secret key available to sign with.`)
  to v4.1.3.

[Full notes](https://github.com/atayozcan/sentinel/releases/tag/v0.6.1)

## [0.6.0] — 2026-05-03

Comprehensive review follow-through: bug fixes in PAM module + agent,
distribution-pipeline corrections (aarch64 release, broader compositor
autostart), code-structure cleanups, and new repo-level docs.

- Bug fixes: `timeout = 0` honoured on the parent side, bypass-queue
  cancel-drain, `resolve_user` prefers `pamh.get_user()`, agent kill
  uses `pkill -fx <abs-path>` (was `pkill -f` matching shell siblings),
  cookie-prefix is multi-byte safe, Wayland socket detection requires
  a real socket.
- Distribution: aarch64 prebuilt deb/rpm/tarball, Niri/River/wlroots
  compositors get the autostart entry (`OnlyShowIn` removed),
  `X-systemd-skip=true` for forward-compat.
- UX: empty default `secondary` (was naming the buttons explicitly,
  conflicted with `randomize_buttons`), generic shield fallback icon
  (`system-lock-screen`), consistent details-panel height,
  `event=auth.headless` audit line.
- Internals: shared `audit::init_syslog` + `audit_emit` fallback,
  shared `POLKIT_PAM_SERVICE` const, structured zbus `MethodError`
  matching, `RenderMode` enum and `Args::effective_render_mode`
  method, placeholder-parity test across locales, dropped dead
  `POLKIT_AGENT_HELPER_CANDIDATES` build-script bake.
- CI / repo: `cargo deny` job + `deny.toml`, `cargo deb / rpm`
  smoke job on PRs, pre-tag release-notes guard, `CONTRIBUTING.md`,
  `CHANGELOG.md`, `CODEOWNERS`, `.github/FUNDING.yml`, README badges.

[Full notes](https://github.com/atayozcan/sentinel/releases/tag/v0.6.0)

## [0.5.2] — 2026-05-03

Internal cleanup: shared crate rename + log dedup. No user-facing
behavior changes, no config changes, no wire-format changes.

- Renamed internal workspace crate `sentinel-config` → `sentinel-shared`.
- Collapsed four near-identical `event=auth.*` log statements in the
  PAM module's dialog path into a single match on `Outcome`.
- Switched `pam-sentinel`'s locale env reader to call shared
  `sentinel_shared::procfs::read_environ_var`.

[Full notes](https://github.com/atayozcan/sentinel/releases/tag/v0.5.2)

## [0.5.1] — 2026-05-03

UAC-style polish + fixes for the install race.

- New `[audio].sound_name` config option — UAC-style audio cue when
  the dialog appears, played through `canberra-gtk-play`.
- Audit log lines (`event=auth.*`) now include logind session
  metadata (`session_type`, `session_class`, `session_remote`).
- Fixed a registration race in `install.sh`'s in-place agent restart
  flow.

[Full notes](https://github.com/atayozcan/sentinel/releases/tag/v0.5.1)

## [0.5.0] — 2026-05-02

Localized dialog + tighter agent.

- 12 embedded fluent translation bundles for the helper's UI chrome
  (en-US, de-DE, es-ES, fr-FR, it-IT, ja-JP, nl-NL, pl-PL, pt-BR,
  ru-RU, tr-TR, zh-CN).
- `event=auth.*` audit lines emitted in logfmt for journald querying.
- libcosmic dependency pinned to a specific commit for reproducible
  builds.
- Layer-shell vs xdg-toplevel auto-fallback for Mutter-based desktops.
- Process icon resolved from the desktop's icon theme.

[Full notes](https://github.com/atayozcan/sentinel/releases/tag/v0.5.0)

## [0.4.1] — 2026-05-02

Hotfix: agent registration silently failed on systems with
`systemd-xdg-autostart-generator`. The autostart entry now sets
`X-GNOME-Autostart-enabled=false` so the systemd wrapper is bypassed
and the agent inherits the compositor's session id.

[Full notes](https://github.com/atayozcan/sentinel/releases/tag/v0.4.1)

## [0.4.0] — 2026-05-02

Sentinel is now your polkit authentication agent.

- `sentinel-polkit-agent` registers as the session's polkit
  authentication agent, replacing process-tree-trust with a Unix
  socket bypass to `polkit-agent-helper-1`.
- Expandable details panel in the dialog showing PID, cmdline, cwd,
  requesting user, polkit action.
- Hardened layout: long fields scroll, max widths/heights bound the
  dialog on tiny displays.

[Full notes](https://github.com/atayozcan/sentinel/releases/tag/v0.4.0)

## [0.3.1] — 2026-05-02

Hotfix: agent registration never succeeded in v0.3.0 because the
agent used `logind.GetSessionByPID()` which requires polkit
authorization the unprivileged agent doesn't have. Replaced with
`/proc/self/sessionid`.

[Full notes](https://github.com/atayozcan/sentinel/releases/tag/v0.3.1)

## [0.3.0] — 2026-05-02

First polkit agent release. (Yanked — see 0.3.1 for the fix.)

[Full notes](https://github.com/atayozcan/sentinel/releases/tag/v0.3.0)

## [0.2.1] — 2026-05-01

Patch: pkexec dialog never appearing. Fixed `display::detect_for_user`
to walk `/run/user/<uid>` for the wayland socket when
`WAYLAND_DISPLAY` is unset (which is normal inside socket-activated
PAM stacks).

[Full notes](https://github.com/atayozcan/sentinel/releases/tag/v0.2.1)

## [0.2.0] — 2026-05-01

**Full Rust rewrite.** The C++ helper and PAM module from v0.1.0 are
replaced. Same UAC-style confirmation dialog flow, smaller dependency
surface, structured logging.

[Full notes](https://github.com/atayozcan/sentinel/releases/tag/v0.2.0)
