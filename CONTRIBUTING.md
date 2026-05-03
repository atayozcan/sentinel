# Contributing to Sentinel

Thanks for your interest in Sentinel! Full guidance lives on the
**[Contributing wiki page](https://github.com/atayozcan/sentinel/wiki/Contributing)**;
this file is the short version for first-time contributors landing in
the repo.

## Reporting bugs and security issues

- **Bugs that aren't security issues**: open a
  [bug report](https://github.com/atayozcan/sentinel/issues/new/choose).
- **Security vulnerabilities**: see [SECURITY.md](.github/SECURITY.md).
  *Please don't open public issues for these.*

## Development quickstart

```bash
git clone https://github.com/atayozcan/sentinel
cd sentinel

# Build the workspace.
cargo build --release --workspace

# Run the test suite.
cargo test --workspace --locked

# Format + clippy (CI gate).
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

To exercise the PAM module + helper end-to-end on your own machine
without affecting your live polkit setup, use the dev-test wrapper:

```bash
./scripts/dev-test.sh
```

This installs into `/usr/lib/security/`, runs an isolated PAM probe,
and rolls back unconditionally on exit.

For installing for real (replaces your polkit auth path — keep a root
shell open until you've confirmed `pkexec` works):

```bash
pkexec ./install.sh
```

The installer is transactional: every replaced file is backed up to
`<path>.pre-sentinel.bak` and the state is recorded in
`/var/lib/sentinel/install.state`. `pkexec ./uninstall.sh` rolls
everything back from that state file.

## Pull requests

The
[pull-request template](.github/pull_request_template.md) lists the
checks reviewers will look for. The big ones:

- **Sentinel sits in the PAM auth path.** If you touch
  `pam-sentinel`, the polkit agent, or the helper UI, please run
  `pkexec ./install.sh && pkexec true` end-to-end before opening the
  PR. A regression here can lock people out of `sudo` / polkit.
- **i18n changes**: test with `LANG=tr_TR.UTF-8 pkexec true` (or any
  shipped locale) — the `every_bundle_has_required_keys` and
  `every_bundle_has_matching_placeholders` tests catch most issues but
  not all rendering quirks.
- **Install/uninstall**: please test the rollback path too
  (`pkexec ./uninstall.sh`).

## Discussion

Open-ended questions — design tradeoffs, "would you take a PR for X",
"is this in scope" — go in
[Discussions](https://github.com/atayozcan/sentinel/discussions)
rather than issues.

## License

By contributing you agree your changes ship under
[GPL-3.0-or-later](LICENSE), Sentinel's license.
