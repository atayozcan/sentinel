mod app;
mod cli;
mod result;

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

    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        eprintln!("{BIN}: WAYLAND_DISPLAY not set; this helper is Wayland-only");
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
