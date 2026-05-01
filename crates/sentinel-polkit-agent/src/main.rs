mod agent;
mod authority;
mod helper1;
mod helper_ui;
mod identity;
mod session;
mod subject;

use anyhow::{Context, Result};
use clap::Parser;
use log::{info, warn};
use sentinel_token::Issuer;
use std::sync::Arc;
use syslog::{BasicLogger, Facility, Formatter3164};
use zbus::Connection;

const AGENT_OBJECT_PATH: &str = "/com/github/sentinel/PolkitAgent";

#[derive(Parser, Debug)]
#[command(version, about = "Sentinel polkit authentication agent")]
struct Args {
    /// Override the systemd login session id (defaults to $XDG_SESSION_ID).
    #[arg(long)]
    session_id: Option<String>,

    /// Verbose logging.
    #[arg(long)]
    debug: bool,
}

fn init_logger(debug: bool) {
    let formatter = Formatter3164 {
        facility: Facility::LOG_AUTH,
        hostname: None,
        process: "sentinel-polkit-agent".into(),
        pid: std::process::id(),
    };
    if let Ok(logger) = syslog::unix(formatter) {
        let _ = log::set_boxed_logger(Box::new(BasicLogger::new(logger)));
        log::set_max_level(if debug {
            log::LevelFilter::Debug
        } else {
            log::LevelFilter::Info
        });
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    init_logger(args.debug);

    // Run on a current-thread tokio runtime; zbus is happy single-threaded
    // and we save a wakeup thread for what's a low-traffic agent.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("build tokio runtime")?;
    rt.block_on(run(args))
}

async fn run(args: Args) -> Result<()> {
    let uid = nix::unistd::getuid().as_raw();
    let issuer = Arc::new(
        Issuer::generate_and_persist(uid).context("write sentinel-agent.secret")?,
    );
    info!("generated session secret for uid {uid}");

    let conn = Connection::system().await.context("connect system bus")?;

    let subject = subject::current(args.session_id.as_deref())
        .context("build unix-session subject")?;

    let agent = agent::Agent::new(issuer.clone(), uid);
    conn.object_server()
        .at(AGENT_OBJECT_PATH, agent)
        .await
        .context("publish AuthenticationAgent object")?;

    let authority = authority::AuthorityProxy::new(&conn)
        .await
        .context("build Authority proxy")?;

    authority
        .register_authentication_agent(&subject, "", AGENT_OBJECT_PATH)
        .await
        .context("Authority.RegisterAuthenticationAgent")?;
    info!("registered as polkit auth agent (object path {AGENT_OBJECT_PATH})");

    // Wait for shutdown signal.
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

    // Best-effort: remove the secret on graceful shutdown so a stale file
    // can't be leveraged after a crash-restart cycle.
    let path = sentinel_token::secret_path_for_uid(uid);
    if let Err(e) = std::fs::remove_file(&path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            warn!("could not remove {}: {e}", path.display());
        }
    }

    info!("shutdown complete");
    Ok(())
}
