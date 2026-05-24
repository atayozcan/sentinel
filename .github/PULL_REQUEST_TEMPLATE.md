<!-- SPDX-FileCopyrightText: 2025 Atay Özcan <atay@oezcan.me> -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
## What & why

<!-- What does this change, and why? Link any related issue (#123). -->

## Checklist

- [ ] `cargo fmt --all -- --check` is clean
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` is clean
- [ ] `cargo test --workspace` passes
- [ ] New files carry the SPDX header (`reuse lint` passes)
- [ ] If this touches `pam-sentinel`, the agent, or the helper: ran
      `sudo ./install.sh && pkexec true` end-to-end (auth-path change)
- [ ] If this touches install/uninstall: ran `./scripts/test-install-container.sh`
- [ ] If this touches the agent↔PAM channel: verified it still works under
      enforcing SELinux (`setenforce 1`)

## Notes for reviewers

<!-- Anything tricky, trade-offs, or things you're unsure about. -->
