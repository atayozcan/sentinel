mod app;
mod cli;
mod result;

use app::{ConfirmApp, loaded_outcome};
use clap::Parser;
use cli::Args;
use std::sync::Arc;

fn main() -> anyhow::Result<()> {
    let args = Arc::new(Args::parse());

    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        eprintln!("sentinel-helper: WAYLAND_DISPLAY not set; this helper is Wayland-only");
        println!("DENY");
        std::process::exit(1);
    }

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

    cosmic::app::run::<ConfirmApp>(settings, args.clone())
        .map_err(|e| anyhow::anyhow!("cosmic app error: {e}"))?;

    let outcome = loaded_outcome();
    println!("{outcome}");
    std::process::exit(outcome.exit_code());
}
