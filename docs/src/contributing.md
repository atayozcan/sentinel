# Contributing

Thanks for thinking about contributing! Sentinel sits in the PAM
authentication path, so reviewers are pickier than average — but the
flow itself is normal GitHub fork-PR-merge.

## Development quickstart

```bash
git clone https://github.com/atayozcan/sentinel
cd sentinel

cargo build --release --workspace
cargo test --workspace --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

## Things reviewers check

The [PR template](https://github.com/atayozcan/sentinel/blob/main/.github/pull_request_template.md)
lists the gate. Most-important items:

- **Sentinel sits in the PAM auth path.** If you touch
  `pam-sentinel`, the agent, or the helper, please run
  `pkexec ./install.sh && pkexec true` end-to-end before opening the
  PR. A regression here can lock people out of `sudo` or polkit.
- **i18n changes** — test with `LANG=tr_TR.UTF-8 pkexec true` or any
  shipped locale. The
  `every_bundle_has_required_keys` and
  `every_bundle_has_matching_placeholders` tests catch most issues
  but not all rendering quirks.
- **Install / uninstall** — please test the rollback path too
  (`pkexec ./uninstall.sh`).
- **i18n: adding a new locale** — see
  `crates/sentinel-helper/src/i18n.rs` doc comment for the four
  steps; the test suite catches missing keys + placeholder drift.

## Architecture references

- [Architecture](./architecture.md) for the design and the trust
  boundaries.
- [Configuration](./configuration.md) for the on-disk schema.
- [PAM wiring](./pam-wiring.md) for the install-time semantics.

## Reporting bugs

Use [bug_report.yml](https://github.com/atayozcan/sentinel/issues/new?template=bug_report.yml)
for general bugs, or [compositor_compat.yml](https://github.com/atayozcan/sentinel/issues/new?template=compositor_compat.yml)
for "did Sentinel work on $compositor" reports (the table in the
README is fed from these).

Security issues go through GitHub Private Vulnerability Reporting —
see the [security policy](./security.md).

## Discussions

Open-ended questions ("would you take a PR for X?", "is this in
scope?") go in [Discussions](https://github.com/atayozcan/sentinel/discussions)
rather than issues.

## License

By contributing you agree your changes ship under
[GPL-3.0-or-later](https://github.com/atayozcan/sentinel/blob/main/LICENSE),
Sentinel's license. New files should carry the SPDX header
(see existing files for the convention; `reuse lint` enforces).
