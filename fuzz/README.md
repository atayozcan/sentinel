<!--
SPDX-FileCopyrightText: 2026 Atay Özcan <atay@oezcan.me>
SPDX-License-Identifier: GPL-3.0-or-later
-->
# Fuzz targets

[`cargo-fuzz`](https://rust-fuzz.github.io/book/) (libFuzzer) harnesses
for Sentinel's parsing surfaces — the inputs that cross a trust-ish
boundary into the auth path.

| Target | Function | Property |
|--------|----------|----------|
| `verdict_parse` | `Verdict::from_str` | Helper stdout → backend verdict. Never panics; `Display` is a stable canonical fixed point of parse∘display. |
| `strip_elevation` | `strip_elevation_prefix` | Parses untrusted `/proc/<pid>/cmdline`. Never panics. |
| `format_message` | `format_message` | `%u`/`%s`/`%p`/`%%` substitution on admin templates. Never panics. |

## Running

`cargo-fuzz` needs **nightly**, but the repo pins stable via
`rust-toolchain.toml` — so override it per-invocation:

```sh
cargo install cargo-fuzz
cargo +nightly fuzz run verdict_parse                       # run until a crash
cargo +nightly fuzz run verdict_parse -- -max_total_time=60 # time-boxed
cargo +nightly fuzz check                                   # build every target, no run
```

Corpora and crash artifacts land under `corpus/` and `artifacts/`
(gitignored). CI smoke-fuzzes each target for 60 s per push (see
`.github/workflows/fuzz.yml`); the weekly schedule runs longer.
