// SPDX-FileCopyrightText: 2025 Atay Özcan <atay@oezcan.me>
// SPDX-License-Identifier: GPL-3.0-or-later
mod app;
mod audio;
mod cli;
mod i18n;

use app::{ConfirmApp, loaded_outcome};
use clap::{CommandFactory, Parser};
use cli::{Args, GenSubcommand, RenderMode};
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

    // Fire the UAC-style audio cue before starting the cosmic event
    // loop — gives the user immediate auditory feedback that
    // something needs their attention, even if the dialog hasn't
    // painted yet. Empty sound_name = no-op.
    audio::play_named(&args.sound_name);

    let xdg_current_desktop = std::env::var("XDG_CURRENT_DESKTOP").ok();
    let render_mode = args.effective_render_mode(xdg_current_desktop.as_deref());

    if render_mode == RenderMode::Windowed && !args.windowed && !args.layer_shell {
        eprintln!(
            "{BIN}: detected compositor without zwlr-layer-shell-v1 \
             (XDG_CURRENT_DESKTOP={:?}); auto-falling back to windowed mode. \
             Pass --layer-shell to override.",
            xdg_current_desktop.as_deref().unwrap_or("")
        );
    }

    // Carry the resolved mode on `args.windowed` so `app.rs` reads the
    // same decision without needing the env-var lookup. This is the
    // single mutation; everything else flows through `Arc<Args>`.
    let mut args = args;
    args.windowed = render_mode == RenderMode::Windowed;

    let mut settings = cosmic::app::Settings::default()
        .transparent(true)
        .exit_on_close(true);
    if render_mode == RenderMode::LayerShell {
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
    use cli::desktop_lacks_layer_shell;

    // Sanity check that the parser is still exposed where main.rs would
    // historically call it. The thorough test cases live in cli.rs.

    #[test]
    fn desktop_blocklist_matches_gnome() {
        assert!(desktop_lacks_layer_shell("GNOME"));
        assert!(desktop_lacks_layer_shell("ubuntu:GNOME"));
    }

    #[test]
    fn desktop_blocklist_allows_wlroots() {
        assert!(!desktop_lacks_layer_shell("COSMIC"));
        assert!(!desktop_lacks_layer_shell("Hyprland"));
        assert!(!desktop_lacks_layer_shell("sway"));
    }
}
