use clap::{Parser, Subcommand};

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
    /// overlay. Debugging/headless-testing only.
    #[arg(long)]
    pub windowed: bool,
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
