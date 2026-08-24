use crate::clipboard::{ClipboardCommand, ClipboardItem};
use crate::error::{AppError, AppResult};
use crate::protocol::{ClipboardEvent, Hello, PROTOCOL_VERSION, WireMessage};
use crate::sync_engine::{RemoteEventResult, SyncEngine};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex, mpsc::Sender};
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::{Mutex as AsyncMutex, broadcast};
use tokio::time::{sleep, timeout};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{WebSocketStream, accept_async, connect_async};
use tracing::{debug, info, warn};
use uuid::Uuid;

const DISCOVERY_PORT: u16 = 8766;
const DISCOVERY_MAGIC: &[u8] = b"CLIPSYNC-P2P\0";
const DISCOVERY_INTERVAL: Duration = Duration::from_secs(2);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const RECONNECT_INTERVAL: Duration = Duration::from_secs(3);

#[derive(Clone, Debug)]
struct RoutedEvent {
    event: ClipboardEvent,
    exclude: Option<Uuid>,
}

#[derive(Debug, Serialize, Deserialize)]
struct DiscoveryBeacon {
    protocol_version: u16,
    device_id: Uuid,
    websocket_port: u16,
}

#[derive(Default, Debug)]
struct PeerRegistry {
    active: Mutex<HashSet<Uuid>>,
    connecting: Mutex<HashSet<SocketAddr>>,
    scheduled: Mutex<HashSet<SocketAddr>>,
}

impl PeerRegistry {
    fn acquire(&self, peer_id: Uuid) -> bool {
        self.active
            .lock()
            .expect("peer registry poisoned")
            .insert(peer_id)
    }

    fn release(&self, peer_id: Uuid) {
        self.active
            .lock()
            .expect("peer registry poisoned")
            .remove(&peer_id);
    }

    fn begin_connect(&self, address: SocketAddr) -> bool {
        self.connecting
            .lock()
            .expect("peer registry poisoned")
            .insert(address)
    }

    fn end_connect(&self, address: SocketAddr) {
        self.connecting
            .lock()
            .expect("peer registry poisoned")
            .remove(&address);
    }

    fn schedule(&self, address: SocketAddr) -> bool {
        self.scheduled
            .lock()
            .expect("peer registry poisoned")
            .insert(address)
    }
}

pub struct Node {
    pub device_id: Uuid,
    listen_port: AtomicU16,
    engine: AsyncMutex<SyncEngine>,
    clipboard_commands: Sender<ClipboardCommand>,
    events: broadcast::Sender<RoutedEvent>,
    registry: Arc<PeerRegistry>,
}

impl Node {
    pub fn new(
        device_id: Uuid,
        listen_port: u16,
        clipboard_commands: Sender<ClipboardCommand>,
    ) -> Arc<Self> {
        let (events, _) = broadcast::channel(256);
        Arc::new(Self {
            device_id,
            listen_port: AtomicU16::new(listen_port),
            engine: AsyncMutex::new(SyncEngine::new(device_id)),
            clipboard_commands,
            events,
            registry: Arc::new(PeerRegistry::default()),
        })
    }

    pub async fn start(
        self: Arc<Self>,
        bind: SocketAddr,
        manual_peers: Vec<SocketAddr>,
        discovery_enabled: bool,
    ) -> AppResult<()> {
        let listener = TcpListener::bind(bind).await?;
        let actual_port = listener.local_addr()?.port();
        self.listen_port.store(actual_port, Ordering::Relaxed);
        info!(address = %listener.local_addr()?, device_id = %self.device_id, "peer listener started");

        let accept_node = Arc::clone(&self);
        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, address)) => {
                        let node = Arc::clone(&accept_node);
                        tokio::spawn(async move {
                            if let Err(error) = node.handle_incoming(stream, address).await {
                                debug!(%error, %address, "incoming peer connection ended");
                            }
                        });
                    }
                    Err(error) => {
                        warn!(%error, "peer listener accept failed");
                        sleep(RECONNECT_INTERVAL).await;
                    }
                }
            }
        });

        for address in manual_peers {
            if self.registry.schedule(address) {
                let node = Arc::clone(&self);
                tokio::spawn(async move { node.reconnect_to(address).await });
            }
        }

        if discovery_enabled {
            self.start_discovery().await?;
        }

        Ok(())
    }

    pub async fn publish_local(&self, item: ClipboardItem) -> AppResult<()> {
        let (kind, payload) = item.into_parts();
        let event = self.engine.lock().await.local_event(kind, payload)?;
        self.route(event, None);
        Ok(())
    }

    pub fn shutdown_clipboard(&self) {
        let _ = self.clipboard_commands.send(ClipboardCommand::Shutdown);
    }

    async fn handle_incoming(
        self: Arc<Self>,
        stream: TcpStream,
        address: SocketAddr,
    ) -> AppResult<()> {
        let websocket = accept_async(stream).await?;
        self.run_connection(websocket, false, address).await
    }

    async fn reconnect_to(self: Arc<Self>, address: SocketAddr) {
        loop {
            if !self.registry.begin_connect(address) {
                sleep(RECONNECT_INTERVAL).await;
                continue;
            }

            let result = match connect_async(format!("ws://{address}")).await {
                Ok((websocket, _)) => self.clone().run_connection(websocket, true, address).await,
                Err(error) => Err(AppError::WebSocket(error)),
            };
            self.registry.end_connect(address);

            if let Err(error) = result {
                debug!(%error, %address, "peer connection failed; retrying");
            }
            sleep(RECONNECT_INTERVAL).await;
        }
    }

    async fn run_connection<S>(
        self: Arc<Self>,
        websocket: WebSocketStream<S>,
        outgoing: bool,
        address: SocketAddr,
    ) -> AppResult<()>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    {
        let (mut writer, mut reader) = websocket.split();
        send_wire(
            &mut writer,
            WireMessage::Hello(Hello {
                protocol_version: PROTOCOL_VERSION,
                device_id: self.device_id,
                listen_port: self.listen_port.load(Ordering::Relaxed),
            }),
        )
        .await?;

        let hello = timeout(HANDSHAKE_TIMEOUT, reader.next())
            .await
            .map_err(|_| AppError::Protocol("peer handshake timed out".into()))?
            .ok_or_else(|| AppError::Protocol("peer closed during handshake".into()))??;
        let peer_id = match hello {
            Message::Binary(frame) => match WireMessage::decode(&frame)? {
                WireMessage::Hello(hello) => {
                    if hello.protocol_version != PROTOCOL_VERSION {
                        return Err(AppError::Protocol(format!(
                            "unsupported peer protocol {}",
                            hello.protocol_version
                        )));
                    }
                    hello.device_id
                }
                _ => {
                    return Err(AppError::Protocol(
                        "first peer message was not Hello".into(),
                    ));
                }
            },
            _ => return Err(AppError::Protocol("peer Hello must be binary".into())),
        };

        if peer_id == self.device_id {
            return Err(AppError::Protocol("refusing connection to self".into()));
        }
        if !direction_allowed(self.device_id, peer_id, outgoing) {
            return Err(AppError::Protocol(
                "duplicate connection direction rejected".into(),
            ));
        }
        if !self.registry.acquire(peer_id) {
            return Err(AppError::Protocol(
                "peer already has an active connection".into(),
            ));
        }
        let active_peer = ActivePeer {
            registry: Arc::clone(&self.registry),
            peer_id,
        };
        info!(%peer_id, %address, outgoing, "peer connected");

        let known = self
            .engine
            .lock()
            .await
            .current()
            .map(|event| event.version.clone());
        send_wire(&mut writer, WireMessage::StateRequest { known }).await?;
        let mut events = self.events.subscribe();

        let result = loop {
            tokio::select! {
                incoming = reader.next() => {
                    match incoming {
                        Some(Ok(message)) => {
                            if let Some(result) = self.handle_message(&mut writer, peer_id, message).await? {
                                break result;
                            }
                        }
                        Some(Err(error)) => break Err(AppError::WebSocket(error)),
                        None => break Ok(()),
                    }
                }
                routed = events.recv() => {
                    match routed {
                        Ok(routed) if routed.exclude != Some(peer_id) => {
                            send_wire(&mut writer, WireMessage::Event(routed.event)).await?;
                        }
                        Ok(_) => {}
                        Err(broadcast::error::RecvError::Lagged(_)) => {
                            let known = self.engine.lock().await.current().map(|event| event.version.clone());
                            send_wire(&mut writer, WireMessage::StateRequest { known }).await?;
                        }
                        Err(broadcast::error::RecvError::Closed) => break Ok(()),
                    }
                }
            }
        };

        drop(active_peer);
        result
    }

    async fn handle_message<S>(
        &self,
        writer: &mut futures_util::stream::SplitSink<WebSocketStream<S>, Message>,
        source: Uuid,
        message: Message,
    ) -> AppResult<Option<AppResult<()>>>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    {
        match message {
            Message::Binary(frame) => match WireMessage::decode(&frame)? {
                WireMessage::Event(event) => {
                    self.handle_event(event, source).await?;
                }
                WireMessage::StateRequest { known } => {
                    let engine = self.engine.lock().await;
                    if engine.should_send_state(known.as_ref()) {
                        send_wire(
                            writer,
                            WireMessage::State {
                                latest: engine.current().cloned(),
                            },
                        )
                        .await?;
                    }
                }
                WireMessage::State {
                    latest: Some(event),
                } => {
                    self.handle_event(event, source).await?;
                }
                WireMessage::State { latest: None } => {}
                WireMessage::Ping(value) => send_wire(writer, WireMessage::Pong(value)).await?,
                WireMessage::Pong(_) => {}
                WireMessage::Hello(_) => {
                    return Err(AppError::Protocol(
                        "Hello is only valid during handshake".into(),
                    ));
                }
            },
            Message::Ping(data) => writer.send(Message::Pong(data)).await?,
            Message::Close(_) => return Ok(Some(Ok(()))),
            Message::Text(_) => {
                return Err(AppError::Protocol(
                    "text WebSocket frames are unsupported".into(),
                ));
            }
            Message::Pong(_) => {}
            Message::Frame(_) => {}
        }
        Ok(None)
    }

    async fn handle_event(&self, event: ClipboardEvent, source: Uuid) -> AppResult<()> {
        let decision = self.engine.lock().await.accept_remote(event.clone())?;
        match decision {
            RemoteEventResult::Duplicate => {}
            RemoteEventResult::ForwardOnly => self.route(event, Some(source)),
            RemoteEventResult::ApplyAndForward => {
                if let Some(item) = event_to_item(&event)? {
                    self.clipboard_commands
                        .send(ClipboardCommand::Apply(item))
                        .map_err(|_| AppError::ChannelClosed)?;
                }
                self.route(event, Some(source));
            }
        }
        Ok(())
    }

    fn route(&self, event: ClipboardEvent, exclude: Option<Uuid>) {
        let _ = self.events.send(RoutedEvent { event, exclude });
    }

    async fn start_discovery(self: Arc<Self>) -> AppResult<()> {
        let receiver = UdpSocket::bind(("0.0.0.0", DISCOVERY_PORT)).await?;
        let sender = UdpSocket::bind(("0.0.0.0", 0)).await?;
        sender.set_broadcast(true)?;

        let announce_node = Arc::clone(&self);
        tokio::spawn(async move {
            let beacon = DiscoveryBeacon {
                protocol_version: PROTOCOL_VERSION,
                device_id: announce_node.device_id,
                websocket_port: announce_node.listen_port.load(Ordering::Relaxed),
            };
            let Ok(encoded) = bincode::serialize(&beacon) else {
                return;
            };
            let mut packet = DISCOVERY_MAGIC.to_vec();
            packet.extend(encoded);
            loop {
                if let Err(error) = sender
                    .send_to(&packet, ("255.255.255.255", DISCOVERY_PORT))
                    .await
                {
                    debug!(%error, "discovery beacon failed");
                }
                sleep(DISCOVERY_INTERVAL).await;
            }
        });

        let listen_node = Arc::clone(&self);
        tokio::spawn(async move {
            let mut packet = vec![0u8; 1024];
            loop {
                let Ok((size, source)) = receiver.recv_from(&mut packet).await else {
                    break;
                };
                if size <= DISCOVERY_MAGIC.len() || !packet.starts_with(DISCOVERY_MAGIC) {
                    continue;
                }
                let Ok(beacon) =
                    bincode::deserialize::<DiscoveryBeacon>(&packet[DISCOVERY_MAGIC.len()..size])
                else {
                    continue;
                };
                if beacon.protocol_version != PROTOCOL_VERSION
                    || beacon.device_id == listen_node.device_id
                    || beacon.websocket_port == 0
                    || listen_node.device_id > beacon.device_id
                {
                    continue;
                }
                let address = SocketAddr::new(source.ip(), beacon.websocket_port);
                if listen_node.registry.schedule(address) {
                    let node = Arc::clone(&listen_node);
                    tokio::spawn(async move { node.reconnect_to(address).await });
                }
            }
        });
        Ok(())
    }
}

struct ActivePeer {
    registry: Arc<PeerRegistry>,
    peer_id: Uuid,
}

impl Drop for ActivePeer {
    fn drop(&mut self) {
        self.registry.release(self.peer_id);
    }
}

fn direction_allowed(local: Uuid, peer: Uuid, outgoing: bool) -> bool {
    if local < peer { outgoing } else { !outgoing }
}

async fn send_wire<S>(
    writer: &mut futures_util::stream::SplitSink<WebSocketStream<S>, Message>,
    message: WireMessage,
) -> AppResult<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    writer
        .send(Message::Binary(message.encode()?.into()))
        .await?;
    Ok(())
}

fn event_to_item(event: &ClipboardEvent) -> AppResult<Option<ClipboardItem>> {
    Ok(Some(match event.kind {
        crate::protocol::ClipboardKind::Text => ClipboardItem::Text(
            String::from_utf8(event.payload.clone())
                .map_err(|_| AppError::Protocol("text payload is not UTF-8".into()))?,
        ),
        crate::protocol::ClipboardKind::Png => ClipboardItem::Png(event.payload.clone()),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_lower_device_id_dials() {
        let lower = Uuid::from_u128(1);
        let higher = Uuid::from_u128(2);
        assert!(direction_allowed(lower, higher, true));
        assert!(!direction_allowed(lower, higher, false));
        assert!(direction_allowed(higher, lower, false));
        assert!(!direction_allowed(higher, lower, true));
    }
}
