<!-- SPDX-FileCopyrightText: 2025 Atay Özcan <atay@oezcan.me> -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Contributing to Sentinel-KDE

Thanks for your interest! Sentinel sits in the PAM authentication path, so
reviewers are pickier than average — but the flow is normal GitHub fork-PR-merge.
The full guide is in [`docs/src/contributing.md`](docs/src/contributing.md).

## Reporting

- **Bugs** (not security): <https://github.com/atayozcan/sentinel-kde/issues/new/choose>
- **Security vulnerabilities**: see [`.github/SECURITY.md`](.github/SECURITY.md) —
  *please don't open public issues for these.*

## Development quickstart

```bash
sudo zypper install rustup pam-devel        # + Qt6/KF6 devel for the helper
rustup default 1.85

git clone https://github.com/atayozcan/sentinel-kde
cd sentinel-kde

cargo build --release --workspace
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

Install for real (replaces your polkit/sudo auth path — keep a `sudo -i` shell
open until you've confirmed it works):

```bash
sudo ./install.sh        # transactional; sudo ./uninstall.sh reverts everything
```

## Pull requests

The [PR template](.github/pull_request_template.md) lists the checklist. The big
ones:

- **Auth-path changes** (`pam-sentinel`, the agent, the helper): run
  `sudo ./install.sh && pkexec true` end-to-end before opening the PR.
- **Install/uninstall changes**: run `./scripts/test-install-container.sh`
  (9 podman scenarios).
- **Agent↔PAM channel changes**: verify they still work under enforcing SELinux
  (`setenforce 1`).
- **SPDX headers** on new files (`reuse lint` enforces).

## License

By contributing you agree your changes ship under **GPL-3.0-or-later** — see
[LICENSE](LICENSE).
