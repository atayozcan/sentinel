use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(version, about = "Sentinel confirmation helper")]
pub struct Args {
    #[arg(long, default_value = "Authentication Required")]
    pub title: String,

    #[arg(long, default_value = "An application is requesting elevated privileges.")]
    pub message: String,

    #[arg(long, default_value = "Click Allow to continue or Deny to cancel.")]
    pub secondary: String,

    #[arg(long)]
    pub process_exe: Option<String>,

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
    /// overlay. Debugging/headless-testing only.
    #[arg(long)]
    pub windowed: bool,
}
