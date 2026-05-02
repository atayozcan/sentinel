<!--
Thanks for sending a PR. Sentinel sits in the PAM authentication
path — a regression here can lock people out of `sudo` or polkit.
The checklist below is what reviewers will be looking for.
-->

## What this changes

<!-- One or two sentences. The "why" is more useful than the "what" — the diff shows the what. -->

## Verification

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] If touching the helper UI, the PAM module, or the agent:
      ran `pkexec ./install.sh` and exercised `pkexec true` end-to-end
      (and `LANG=tr_TR.UTF-8` if i18n changed)
- [ ] If touching install / uninstall: tested the rollback path too
- [ ] If user-facing: updated the wiki page(s) and / or release notes

## Risk

<!-- Anything reviewers should pay extra attention to? Race
conditions, sandboxing, distros / compositors not in CI, etc. -->

## Related issues

<!-- e.g. Closes #123 -->
