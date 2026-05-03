// SPDX-FileCopyrightText: 2025 Atay Özcan <atay@oezcan.me>
// SPDX-License-Identifier: GPL-3.0-or-later
use clap::{Parser, Subcommand};

/// How the helper paints its dialog. Resolved by [`Args::effective_render_mode`]
/// from `--windowed` / `--layer-shell` and the `XDG_CURRENT_DESKTOP`
/// blocklist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
    /// `zwlr-layer-shell-v1` overlay covering the whole output.
    LayerShell,
    /// Plain `xdg-toplevel` window. Used on Mutter-based desktops that
    /// don't implement `zwlr-layer-shell-v1`.
    Windowed,
}

#[derive(Parser, Debug, Clone)]
#[command(version, about = "Sentinel confirmation helper")]
pub struct Args {
    /// Internal helper subcommands (completions, man page generation).
    /// Hidden — used by the installer and packaging.
    #[command(subcommand)]
    pub generate: Option<GenSubcommand>,

    #[arg(long, default_value = "Authentication Required")]
    pub title: String,

    #[arg(
        long,
        default_value = "An application is requesting elevated privileges."
    )]
    pub message: String,

    #[arg(long, default_value = "Click Allow to continue or Deny to cancel.")]
    pub secondary: String,

    #[arg(long)]
    pub process_exe: Option<String>,

    /// Full command line of the requesting process. Shown in the expanded
    /// "details" section.
    #[arg(long)]
    pub process_cmdline: Option<String>,

    /// PID of the requesting process. Shown in the expanded section.
    #[arg(long)]
    pub process_pid: Option<i32>,

    /// Working directory of the requesting process. Shown in the expanded
    /// section.
    #[arg(long)]
    pub process_cwd: Option<String>,

    /// Username of the user requesting elevation. Shown in the expanded
    /// section.
    #[arg(long)]
    pub requesting_user: Option<String>,

    /// Polkit action id or PAM service name. Shown in the expanded section.
    #[arg(long)]
    pub action: Option<String>,

    /// Auto-deny timeout in seconds (0 = no timeout).
    #[arg(long, default_value_t = 30)]
    pub timeout: u64,

    /// Minimum display time in milliseconds before Allow is enabled.
    #[arg(long, default_value_t = 500)]
    pub min_time: u64,

    /// Randomize Allow/Deny button positions.
    #[arg(long)]
    pub randomize: bool,

    /// Render as a regular xdg-toplevel window instead of a layer-shell
    /// overlay. Use this on compositors without `zwlr-layer-shell-v1`
    /// (notably GNOME/Mutter). Also auto-selected when
    /// `XDG_CURRENT_DESKTOP` matches a known-bad compositor — see
    /// `compositor_lacks_layer_shell()` in main.rs.
    #[arg(long)]
    pub windowed: bool,

    /// Force the layer-shell path even on compositors detected as
    /// lacking `zwlr-layer-shell-v1`. Override for the auto-downgrade
    /// heuristic, mainly useful for testing.
    #[arg(long, conflicts_with = "windowed")]
    pub layer_shell: bool,

    /// Freedesktop sound name to play when the dialog appears
    /// (UAC-style audio cue). Empty string = silent. Resolved via
    /// `canberra-gtk-play` if installed; otherwise no sound is
    /// played and we don't error out.
    #[arg(long, default_value_t = String::new())]
    pub sound_name: String,
}

impl Args {
    /// Decide whether to use layer-shell or xdg-toplevel rendering.
    ///
    /// Priority:
    /// 1. `--windowed` → always windowed.
    /// 2. `--layer-shell` → always layer-shell (override the heuristic).
    /// 3. else: layer-shell unless `XDG_CURRENT_DESKTOP` indicates a
    ///    compositor known to lack `zwlr-layer-shell-v1` (GNOME/Mutter,
    ///    Unity, Pantheon, Budgie — all Mutter-based).
    ///
    /// Without this auto-downgrade, the helper hard-fails on those
    /// systems and the user sees no dialog at all.
    pub fn effective_render_mode(&self, xdg_current_desktop: Option<&str>) -> RenderMode {
        if self.windowed {
            return RenderMode::Windowed;
        }
        if self.layer_shell {
            return RenderMode::LayerShell;
        }
        match xdg_current_desktop {
            Some(d) if desktop_lacks_layer_shell(d) => RenderMode::Windowed,
            _ => RenderMode::LayerShell,
        }
    }
}

/// Pure parser for the `XDG_CURRENT_DESKTOP` value (colon-separated,
/// case-insensitive). Maintaining a *blocklist* (rather than allowlist)
/// means new wlroots-style compositors get the layer-shell path
/// automatically; only known-Mutter-based desktops fall through to
/// xdg-toplevel.
pub fn desktop_lacks_layer_shell(xdg: &str) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn args_with_flags(windowed: bool, layer_shell: bool) -> Args {
        Args {
            generate: None,
            title: String::new(),
            message: String::new(),
            secondary: String::new(),
            process_exe: None,
            process_cmdline: None,
            process_pid: None,
            process_cwd: None,
            requesting_user: None,
            action: None,
            timeout: 30,
            min_time: 500,
            randomize: false,
            windowed,
            layer_shell,
            sound_name: String::new(),
        }
    }

    #[test]
    fn windowed_flag_wins() {
        let a = args_with_flags(true, false);
        assert_eq!(
            a.effective_render_mode(Some("COSMIC")),
            RenderMode::Windowed
        );
        assert_eq!(a.effective_render_mode(Some("GNOME")), RenderMode::Windowed);
    }

    #[test]
    fn layer_shell_flag_overrides_heuristic() {
        let a = args_with_flags(false, true);
        // GNOME would normally trigger windowed fallback; --layer-shell forces layer-shell.
        assert_eq!(
            a.effective_render_mode(Some("GNOME")),
            RenderMode::LayerShell
        );
    }

    #[test]
    fn auto_falls_back_on_mutter() {
        let a = args_with_flags(false, false);
        assert_eq!(a.effective_render_mode(Some("GNOME")), RenderMode::Windowed);
        assert_eq!(
            a.effective_render_mode(Some("ubuntu:GNOME")),
            RenderMode::Windowed
        );
        assert_eq!(
            a.effective_render_mode(Some("Pantheon")),
            RenderMode::Windowed
        );
    }

    #[test]
    fn auto_uses_layer_shell_on_wlroots_family() {
        let a = args_with_flags(false, false);
        assert_eq!(
            a.effective_render_mode(Some("COSMIC")),
            RenderMode::LayerShell
        );
        assert_eq!(
            a.effective_render_mode(Some("Hyprland")),
            RenderMode::LayerShell
        );
        assert_eq!(a.effective_render_mode(Some("KDE")), RenderMode::LayerShell);
    }

    #[test]
    fn missing_env_treats_as_unknown_layer_shell() {
        // No XDG_CURRENT_DESKTOP set; try layer-shell and let it fail loudly if unsupported.
        let a = args_with_flags(false, false);
        assert_eq!(a.effective_render_mode(None), RenderMode::LayerShell);
    }
}

#[derive(Subcommand, Debug, Clone)]
#[command(hide = true)]
pub enum GenSubcommand {
    /// Print a shell completion script to stdout.
    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
    /// Print a roff(1)-formatted man page to stdout.
    Man,
}
