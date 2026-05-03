# Building from source

## Toolchain

- Rust **1.85+** (workspace MSRV pinned in `rust-toolchain.toml`).
- Linux build deps: `libpam0g-dev`, `libxkbcommon-dev`,
  `libwayland-dev`, `libfontconfig1-dev`, `libfreetype6-dev`,
  `pkg-config`. (Arch: `pam wayland libxkbcommon fontconfig
  freetype2 mesa vulkan-icd-loader`.)

## Building

```bash
git clone https://github.com/atayozcan/sentinel
cd sentinel
cargo build --release --workspace --locked
```

This produces:

- `target/release/libpam_sentinel.so` — the cdylib
- `target/release/sentinel-helper` — the GUI binary
- `target/release/sentinel-polkit-agent` — the polkit agent

## Running tests

```bash
cargo test --workspace --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

`cargo audit` and `cargo deny check` run in CI; install locally
with `cargo install --locked cargo-audit cargo-deny`.

## Compile-time configuration

The workspace's `build.rs` files bake install paths into the
binaries via env vars at compile time:

| Var | Default | Meaning |
|-----|---------|---------|
| `SENTINEL_PREFIX` | `/usr` | Install prefix for binaries. |
| `SENTINEL_SYSCONFDIR` | `/etc` | Where `sentinel.conf` lives. |
| `SENTINEL_LIBEXECDIR` | `lib` | Subdir under PREFIX for the helper + agent. |

For a custom-prefix build:

```bash
SENTINEL_PREFIX=/usr/local SENTINEL_SYSCONFDIR=/usr/local/etc \
    cargo build --release --workspace --locked
```

The PAM module + agent compile-time-bake the helper's absolute path,
so they always know where to spawn it from.

## Test it locally without an install

```bash
./scripts/dev-test.sh
```

This installs to system paths, compiles a small `pam_authtest` probe
that calls `pam_authenticate()` for a dedicated test service, runs
the probe, and rolls everything back unconditionally on exit. Refuses
to run if Sentinel is already installed (prevents accidental
clobbering of your real config).

## Building distribution packages

```bash
./scripts/build-release.sh 0.7.0
```

Produces `dist/`:
- `sentinel-0.7.0.tar.gz` (source)
- `sentinel-0.7.0-x86_64-linux.tar.gz` (binary, install layout)
- per-arch `.sha256` files

For deb/rpm:
```bash
cargo deb --no-build -p sentinel-helper
cargo generate-rpm -p crates/sentinel-helper
```

## Shell completions and man pages

The two binaries auto-generate completions and man pages:

```bash
sentinel-helper completions bash > /etc/bash_completion.d/sentinel-helper
sentinel-helper man > /usr/share/man/man1/sentinel-helper.1
```

The release tarballs and packages ship these pre-rendered.
