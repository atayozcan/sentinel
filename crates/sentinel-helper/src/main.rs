mod app;
mod cli;
mod i18n;

use app::{ConfirmApp, loaded_outcome};
use clap::{CommandFactory, Parser};
use cli::{Args, GenSubcommand};
use std::sync::Arc;

const BIN: &str = "sentinel-helper";

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    if let Some(g) = &args.generate {
        return run_gen(g);
    }

    // Initialize the global translation bundle from $LANG/$LC_*
    // before we touch any UI. Subsequent t() calls are infallible.
    i18n::init();

    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        eprintln!("{BIN}: WAYLAND_DISPLAY not set; this helper is Wayland-only");
        println!("DENY");
        std::process::exit(1);
    }

    // Decide layer-shell vs xdg-toplevel rendering. Priority:
    //   1. --windowed   → always windowed.
    //   2. --layer-shell → always layer-shell (override the heuristic).
    //   3. else: layer-shell unless XDG_CURRENT_DESKTOP indicates a
    //      compositor known to lack `zwlr-layer-shell-v1` (GNOME/Mutter,
    //      Unity, Pantheon — all Mutter-based). Without this auto-
    //      downgrade, the helper hard-fails on those systems and the
    //      user sees no dialog at all.
    let use_windowed = args.windowed || (!args.layer_shell && compositor_lacks_layer_shell());

    if use_windowed && !args.windowed && !args.layer_shell {
        eprintln!(
            "{BIN}: detected compositor without zwlr-layer-shell-v1 \
             (XDG_CURRENT_DESKTOP={:?}); auto-falling back to windowed mode. \
             Pass --layer-shell to override.",
            std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default()
        );
    }

    // Re-bind for the move into the cosmic app: store the resolved
    // mode on the args struct so app.rs sees the same decision.
    let mut args = args;
    args.windowed = use_windowed;

    let mut settings = cosmic::app::Settings::default()
        .transparent(true)
        .exit_on_close(true);
    if !args.windowed {
        // Layer-shell path: no xdg toplevel, the overlay surface is created in init().
        settings = settings.no_main_window(true);
    } else {
        settings = settings
            .size(cosmic::iced::Size::new(460.0, 420.0))
            .resizable(None);
    }

    cosmic::app::run::<ConfirmApp>(settings, Arc::new(args))
        .map_err(|e| anyhow::anyhow!("cosmic app error: {e}"))?;

    let outcome = loaded_outcome();
    println!("{outcome}");
    std::process::exit(outcome.exit_code());
}

/// True for compositors known to *not* implement `zwlr-layer-shell-v1`.
/// Reads `XDG_CURRENT_DESKTOP`; missing env var means "unknown — try
/// layer-shell and let it fail loudly if it doesn't work".
fn compositor_lacks_layer_shell() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP")
        .map(|v| desktop_lacks_layer_shell(&v))
        .unwrap_or(false)
}

/// Pure parser for the `XDG_CURRENT_DESKTOP` value (colon-separated,
/// case-insensitive). Maintaining a *blocklist* (rather than
/// allowlist) means new wlroots-style compositors get the layer-shell
/// path automatically; only known-Mutter-based desktops fall through
/// to xdg-toplevel.
fn desktop_lacks_layer_shell(xdg: &str) -> bool {
    xdg.split(':').any(|d| {
        let d = d.trim();
        d.eq_ignore_ascii_case("GNOME")
            || d.eq_ignore_ascii_case("GNOME-Classic")
            || d.eq_ignore_ascii_case("GNOME-Flashback")
            || d.eq_ignore_ascii_case("Unity")
            || d.eq_ignore_ascii_case("Pantheon") // elementary OS — Mutter-based
            || d.eq_ignore_ascii_case("Budgie") // Budgie 10.x is Mutter-based
    })
}

fn run_gen(g: &GenSubcommand) -> anyhow::Result<()> {
    let mut cmd = Args::command();
    match g {
        GenSubcommand::Completions { shell } => {
            clap_complete::generate(*shell, &mut cmd, BIN, &mut std::io::stdout());
        }
        GenSubcommand::Man => {
            clap_mangen::Man::new(cmd).render(&mut std::io::stdout())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_gnome() {
        assert!(desktop_lacks_layer_shell("GNOME"));
        assert!(desktop_lacks_layer_shell("ubuntu:GNOME"));
        assert!(desktop_lacks_layer_shell("GNOME-Classic"));
    }

    #[test]
    fn detects_other_mutter_based() {
        assert!(desktop_lacks_layer_shell("Unity"));
        assert!(desktop_lacks_layer_shell("Pantheon"));
        assert!(desktop_lacks_layer_shell("Budgie:GNOME"));
    }

    #[test]
    fn allows_wlroots_family() {
        assert!(!desktop_lacks_layer_shell("COSMIC"));
        assert!(!desktop_lacks_layer_shell("Hyprland"));
        assert!(!desktop_lacks_layer_shell("sway"));
        assert!(!desktop_lacks_layer_shell("KDE"));
        assert!(!desktop_lacks_layer_shell("wlroots"));
    }

    #[test]
    fn case_insensitive() {
        assert!(desktop_lacks_layer_shell("gnome"));
        assert!(desktop_lacks_layer_shell("GnOmE"));
    }

    #[test]
    fn empty_string_allows() {
        // Empty XDG_CURRENT_DESKTOP isn't conclusive — try layer-shell.
        assert!(!desktop_lacks_layer_shell(""));
    }

    #[test]
    fn whitespace_around_segments_handled() {
        assert!(desktop_lacks_layer_shell("ubuntu: GNOME"));
    }
}
