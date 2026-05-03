mod agent;
mod approval_queue;
mod authority;
mod helper1;
mod helper_ui;
mod identity;
mod session;
mod socket_server;
mod subject;

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser, Subcommand};
use log::{info, warn};
use sentinel_shared::audit;
use zbus::Connection;

const BIN: &str = "sentinel-polkit-agent";
const AGENT_OBJECT_PATH: &str = "/com/github/sentinel/PolkitAgent";

#[derive(Parser, Debug)]
#[command(version, about = "Sentinel polkit authentication agent")]
struct Args {
    /// Internal helper subcommands (completions, man page generation).
    /// Hidden — used by the installer and packaging.
    #[command(subcommand)]
    generate: Option<GenSubcommand>,

    /// Override the systemd login session id (defaults to $XDG_SESSION_ID
    /// or /proc/self/sessionid).
    #[arg(long)]
    session_id: Option<String>,

    /// Verbose logging.
    #[arg(long)]
    debug: bool,
}

#[derive(Subcommand, Debug, Clone)]
#[command(hide = true)]
enum GenSubcommand {
    /// Print a shell completion script to stdout.
    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
    /// Print a roff(1)-formatted man page to stdout.
    Man,
}

fn init_logger(debug: bool) {
    audit::init_syslog(
        BIN,
        if debug {
            log::LevelFilter::Debug
        } else {
            log::LevelFilter::Info
        },
    );
}

fn main() -> Result<()> {
    let args = Args::parse();

    if let Some(g) = &args.generate {
        return run_gen(g);
    }

    init_logger(args.debug);

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("build tokio runtime")?;
    rt.block_on(run(args))
}

fn run_gen(g: &GenSubcommand) -> Result<()> {
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

async fn run(args: Args) -> Result<()> {
    let uid = nix::unistd::getuid().as_raw();
    let queue = approval_queue::ApprovalQueue::new();

    // Bring up the bypass socket *before* anything else so a polkit
    // auth that races us has somewhere to ask.
    let socket_queue = queue.clone();
    let socket_task = tokio::spawn(async move {
        if let Err(e) = socket_server::serve(uid, socket_queue).await {
            warn!("agent socket server exited: {e:#}");
        }
    });

    let conn = Connection::system().await.context("connect system bus")?;

    let subject =
        subject::current(args.session_id.as_deref()).context("build unix-session subject")?;

    let agent = agent::Agent::new(uid, queue);
    conn.object_server()
        .at(AGENT_OBJECT_PATH, agent)
        .await
        .context("publish AuthenticationAgent object")?;

    let authority = authority::AuthorityProxy::new(&conn)
        .await
        .context("build Authority proxy")?;

    // Retry loop: another polkit agent (cosmic-osd, polkit-gnome,
    // polkit-kde, …) may currently hold the registration, especially
    // right after install.sh's restart-in-place flow where the
    // compositor's supervisor races us to respawn its own agent. We
    // back off and retry; if the competitor eventually exits, gets
    // killed, or backs off, we win. Compositors that hard-respawn
    // their agent forever (cosmic-session → cosmic-osd) are not
    // solvable from this side — see the install.sh diagnostic.
    const REGISTER_RETRIES: u32 = 8;
    const REGISTER_BACKOFF: std::time::Duration = std::time::Duration::from_millis(500);
    let mut last_err: Option<zbus::Error> = None;
    let mut registered = false;
    for attempt in 1..=REGISTER_RETRIES {
        match authority
            .register_authentication_agent(&subject, "", AGENT_OBJECT_PATH)
            .await
        {
            Ok(()) => {
                registered = true;
                if attempt > 1 {
                    info!(
                        "registered as polkit auth agent on attempt {attempt} \
                         (a competitor was holding the registration)"
                    );
                } else {
                    info!("registered as polkit auth agent (object path {AGENT_OBJECT_PATH})");
                }
                break;
            }
            Err(e) => {
                if !is_agent_already_registered(&e) {
                    // Different error — propagate immediately, no retry
                    // (typos in the object path, polkitd not running,
                    // session id mismatch, etc. — none of these
                    // self-heal by waiting).
                    return Err(e).context("Authority.RegisterAuthenticationAgent");
                }
                warn!(
                    "registration attempt {attempt}/{REGISTER_RETRIES}: \
                     another agent is registered, retrying in {REGISTER_BACKOFF:?}"
                );
                last_err = Some(e);
                tokio::time::sleep(REGISTER_BACKOFF).await;
            }
        }
    }
    if !registered {
        return Err(last_err
            .map(anyhow::Error::from)
            .unwrap_or_else(|| anyhow::anyhow!("registration failed after retries")))
        .context("Authority.RegisterAuthenticationAgent");
    }

    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .context("install SIGTERM handler")?;
    let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
        .context("install SIGINT handler")?;
    tokio::select! {
        _ = sigterm.recv() => info!("SIGTERM"),
        _ = sigint.recv() => info!("SIGINT"),
    }

    if let Err(e) = authority
        .unregister_authentication_agent(&subject, AGENT_OBJECT_PATH)
        .await
    {
        warn!("UnregisterAuthenticationAgent failed: {e}");
    }

    socket_task.abort();
    socket_server::unlink_socket(uid);

    info!("shutdown complete");
    Ok(())
}

/// Discriminate "polkitd already has an agent registered" from genuine
/// registration errors. Polkit returns
/// `org.freedesktop.PolicyKit1.Error.Failed` with description
/// containing `already exists` when the session already has an agent;
/// any other error name (object-path typo, dbus daemon down, session
/// mismatch) is non-recoverable and shouldn't be retried.
///
/// Done as a structured match on `zbus::Error::MethodError` rather than
/// substring-matching the `Debug` rendering — far less likely to break
/// when zbus or polkitd reword their messages.
fn is_agent_already_registered(e: &zbus::Error) -> bool {
    match e {
        zbus::Error::MethodError(name, detail, _) => {
            name.as_str().ends_with(".Error.Failed")
                && detail
                    .as_deref()
                    .is_some_and(|d| d.contains("already exists"))
        }
        _ => false,
    }
}
