# Building from source

## Toolchain

- Rust **1.85+** (workspace MSRV pinned in `rust-toolchain.toml`).
- **Backend + KDE helper** native deps: `libpam0g-dev`, `libxkbcommon-dev`,
  `libwayland-dev`, `libfontconfig1-dev`, `libfreetype6-dev`,
  `pkg-config`. (Arch: `pam wayland libxkbcommon fontconfig
  freetype2 mesa vulkan-icd-loader`.)
- **KDE helper** also needs Qt 6 + KF 6 + cxx-qt's private headers
  (openSUSE: `qt6-base-devel qt6-base-private-devel
  qt6-declarative-devel qt6-declarative-private-devel
  kf6-kirigami-imports kf6-qqc2-desktop-style layer-shell-qt6-imports
  qt6-wayland`; Arch: `qt6-base qt6-declarative kirigami
  layer-shell-qt`). It links with `mold`.

## Building

The backend (PAM module + agent) is in `default-members`, so a bare
build skips both GUI toolchains. Build a frontend explicitly.

```bash
git clone https://github.com/atayozcan/sentinel
cd sentinel

cargo build --release --locked                          # backend only (no Qt)
cargo build --release --locked -p sentinel-helper-kde   # + KDE frontend
# `--workspace` builds everything, including the Qt-based KDE helper.
```

This produces:

- `target/release/libpam_sentinel.so` — the cdylib
- `target/release/sentinel-polkit-agent` — the polkit agent
- `target/release/sentinel-helper-kde` — the KDE (Kirigami) dialog

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
./packaging-kde/scripts/dev-test.sh
```

This installs to system paths, compiles a small `pam_authtest` probe
that calls `pam_authenticate()` for a dedicated test service, runs
the probe, and rolls everything back unconditionally on exit. Refuses
to run if Sentinel is already installed (prevents accidental
clobbering of your real config).

## Building distribution packages

```bash
./packaging-kde/scripts/build-release.sh 0.8.0
```

Produces `dist/`:
- `sentinel-kde-0.8.0.tar.gz` (source)
- `sentinel-kde-0.8.0-x86_64-linux.tar.gz` (binary, install layout)
- per-arch `.sha256` files

For an RPM:
```bash
cargo generate-rpm -p crates/sentinel-helper-kde
```

## Shell completions and man pages

`sentinel-polkit-agent` auto-generates its shell completions and man
page:

```bash
sentinel-polkit-agent completions bash > /etc/bash_completion.d/sentinel-polkit-agent
sentinel-polkit-agent man > /usr/share/man/man1/sentinel-polkit-agent.1
```

The release tarballs and packages ship these pre-rendered.
