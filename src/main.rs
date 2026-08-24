mod clipboard;
mod error;
mod network;
mod protocol;
mod sync_engine;

use clap::Parser;
use clipboard::{ArboardBackend, spawn_worker};
use error::{AppError, AppResult};
use network::Node;
use std::net::SocketAddr;
use std::sync::mpsc::Receiver;
use tokio::sync::mpsc;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

#[derive(Debug, Parser)]
#[command(
    name = "clipboard-sync",
    version,
    about = "Peer-to-peer clipboard synchronization"
)]
struct Args {
    #[arg(long, default_value = "0.0.0.0:8765")]
    bind: SocketAddr,
    #[arg(long = "peer", value_name = "HOST:PORT")]
    peers: Vec<SocketAddr>,
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    discovery: bool,
    #[arg(long)]
    device_id: Option<Uuid>,
}

#[tokio::main]
async fn main() -> AppResult<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    let args = Args::parse();
    let device_id = args.device_id.unwrap_or_else(Uuid::new_v4);
    let backend = ArboardBackend::new()?;
    let (clipboard_commands, clipboard_rx, worker) = spawn_worker(backend);
    let (local_tx, mut local_rx) = mpsc::unbounded_channel();
    let bridge =
        tokio::task::spawn_blocking(move || bridge_clipboard_events(clipboard_rx, local_tx));

    let node = Node::new(device_id, args.bind.port(), clipboard_commands);
    node.clone()
        .start(args.bind, args.peers, args.discovery)
        .await?;
    info!(%device_id, "clipboard peer is running");

    loop {
        tokio::select! {
            Some(item) = local_rx.recv() => {
                if let Err(error) = node.publish_local(item).await {
                    warn!(%error, "failed to publish local clipboard event");
                }
            }
            result = tokio::signal::ctrl_c() => {
                result.map_err(AppError::Network)?;
                break;
            }
            else => break,
        }
    }

    node.shutdown_clipboard();
    bridge.abort();
    let _ = worker.join();
    Ok(())
}

fn bridge_clipboard_events(
    source: Receiver<clipboard::ClipboardItem>,
    target: mpsc::UnboundedSender<clipboard::ClipboardItem>,
) {
    while let Ok(item) = source.recv() {
        if target.send(item).is_err() {
            break;
        }
    }
}
