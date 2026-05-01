prefix       := env_var_or_default("PREFIX", "/usr")
sysconfdir   := env_var_or_default("SYSCONFDIR", "/etc")
libexecdir   := env_var_or_default("LIBEXECDIR", "lib")

default: build

# Build the workspace in release mode with the install prefix baked in.
build:
    SENTINEL_PREFIX={{prefix}} \
    SENTINEL_SYSCONFDIR={{sysconfdir}} \
    SENTINEL_LIBEXECDIR={{libexecdir}} \
    cargo build --release --workspace

# Quick type-check without linking.
check:
    cargo check --workspace

# Run clippy with deny on warnings.
lint:
    cargo clippy --workspace --all-targets -- -D warnings

# Format all code.
fmt:
    cargo fmt --all

# Run the helper for a quick UI smoke test (layer-shell overlay).
# DENY exits 1 by design (PAM contract); the attribute keeps just quiet.
[no-exit-message]
helper-test:
    cargo run --release --bin sentinel-helper -- \
        --timeout 10 --randomize \
        --process-exe /usr/bin/sudo

# Same, but rendered as a regular xdg-toplevel window (compositor fallback).
[no-exit-message]
helper-test-windowed:
    cargo run --release --bin sentinel-helper -- \
        --windowed --timeout 10 --randomize \
        --process-exe /usr/bin/sudo

# Build, install, run pamtester against a throwaway PAM service, then roll
# everything back. Re-runnable: cleans up before each fresh test.
dev-test:
    ./scripts/dev-test.sh

# Install via the bundled script (requires root).
install: build
    pkexec ./install.sh

# Uninstall (requires root).
uninstall:
    pkexec ./uninstall.sh

# Build the AUR package locally with makepkg.
aur:
    cd packaging/arch && makepkg -f

clean:
    cargo clean
