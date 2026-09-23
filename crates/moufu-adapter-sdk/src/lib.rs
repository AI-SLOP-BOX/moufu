use moufu_protocol::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::{broadcast, mpsc, Mutex};
use tracing::{debug, error, info};

/// Client handle for an application to communicate with Moufu Integration Hub
#[derive(Clone)]
pub struct MoufuClient {
    tx: mpsc::UnboundedSender<ClientMessage>,
    event_tx: broadcast::Sender<ServerMessage>,
    session_id: Arc<Mutex<Option<AppSessionId>>>,
}

impl MoufuClient {
    /// Connect to Moufu daemon or core server via TCP
    pub async fn connect(
        hub_addr: &str,
        app_name: &str,
        app_version: &str,
        capabilities: AppCapabilities,
    ) -> anyhow::Result<Self> {
        let stream = TcpStream::connect(hub_addr).await?;
        let (reader, mut writer) = stream.into_split();

        let (tx, mut rx) = mpsc::unbounded_channel::<ClientMessage>();
        let (event_tx, _) = broadcast::channel::<ServerMessage>(512);

        // Send handshake immediately
        let handshake = ClientMessage::Handshake {
            app_name: app_name.to_string(),
            app_version: app_version.to_string(),
            capabilities,
        };
        let handshake_json = serde_json::to_string(&handshake)?;
        writer.write_all(handshake_json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;

        // Background write loop
        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                if let Ok(serialized) = serde_json::to_string(&msg) {
                    if writer.write_all(serialized.as_bytes()).await.is_err()
                        || writer.write_all(b"\n").await.is_err()
                        || writer.flush().await.is_err()
                    {
                        break;
                    }
                }
            }
        });

        let session_id_arc = Arc::new(Mutex::new(None));
        let session_id_clone = Arc::clone(&session_id_arc);
        let event_tx_clone = event_tx.clone();

        // Background read loop
        tokio::spawn(async move {
            let mut lines = BufReader::new(reader).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if line.trim().is_empty() {
                    continue;
                }
                if let Ok(server_msg) = serde_json::from_str::<ServerMessage>(&line) {
                    if let ServerMessage::HandshakeAck { session_id, .. } = &server_msg {
                        *session_id_clone.lock().await = Some(*session_id);
                    }
                    let _ = event_tx_clone.send(server_msg);
                }
            }
        });

        Ok(Self {
            tx,
            event_tx,
            session_id: session_id_arc,
        })
    }

    /// Subscribe to server incoming messages (updates, state changes, errors)
    pub fn subscribe(&self) -> broadcast::Receiver<ServerMessage> {
        self.event_tx.subscribe()
    }

    /// Register an entity exposed by this application
    pub fn register_entity(
        &self,
        id: EntityId,
        name: String,
        data_type: String,
        source_app: String,
        metadata: HashMap<String, String>,
    ) -> anyhow::Result<()> {
        self.tx.send(ClientMessage::RegisterEntity(EntityDescriptor {
            id,
            name,
            data_type,
            source_app,
            metadata,
        }))?;
        Ok(())
    }

    /// Create a dynamic link between a source entity and target entity
    pub fn create_link(&self, source_entity: EntityId, target_entity: EntityId) -> anyhow::Result<()> {
        self.tx.send(ClientMessage::CreateLink {
            source_entity,
            target_entity,
        })?;
        Ok(())
    }

    /// Send an inlined JSON change event (e.g. live vector/parameter edit)
    pub fn notify_change_inline(
        &self,
        entity_id: EntityId,
        version: u64,
        is_transient: bool,
        payload: serde_json::Value,
    ) -> anyhow::Result<()> {
        let event = ChangeEvent {
            entity_id,
            version,
            is_transient,
            timestamp: chrono::Utc::now(),
            payload: PayloadTransport::Inline(payload),
            delta: None,
        };
        self.tx.send(ClientMessage::NotifyChange(event))?;
        Ok(())
    }

    /// Send a zero-copy shared memory change event (e.g. high-res video frame / texture)
    pub fn notify_change_shm(
        &self,
        entity_id: EntityId,
        version: u64,
        is_transient: bool,
        shm: SharedMemoryDescriptor,
    ) -> anyhow::Result<()> {
        let event = ChangeEvent {
            entity_id,
            version,
            is_transient,
            timestamp: chrono::Utc::now(),
            payload: PayloadTransport::SharedMemory(shm),
            delta: None,
        };
        self.tx.send(ClientMessage::NotifyChange(event))?;
        Ok(())
    }

    /// Backward-compatible notify_change (defaults to inline)
    pub fn notify_change(
        &self,
        entity_id: EntityId,
        version: u64,
        is_transient: bool,
        payload: serde_json::Value,
    ) -> anyhow::Result<()> {
        self.notify_change_inline(entity_id, version, is_transient, payload)
    }

    /// Request current state of an entity
    pub fn pull_entity(&self, entity_id: EntityId) -> anyhow::Result<()> {
        self.tx.send(ClientMessage::PullEntity { entity_id })?;
        Ok(())
    }
}
