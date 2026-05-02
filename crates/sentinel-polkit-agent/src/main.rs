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
use clap::Parser;
use log::{info, warn};
use syslog::{BasicLogger, Facility, Formatter3164};
use zbus::Connection;

const AGENT_OBJECT_PATH: &str = "/com/github/sentinel/PolkitAgent";

#[derive(Parser, Debug)]
#[command(version, about = "Sentinel polkit authentication agent")]
struct Args {
    /// Override the systemd login session id (defaults to $XDG_SESSION_ID
    /// or /proc/self/sessionid).
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

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("build tokio runtime")?;
    rt.block_on(run(args))
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

    let subject = subject::current(args.session_id.as_deref())
        .context("build unix-session subject")?;

    let agent = agent::Agent::new(uid, queue);
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
