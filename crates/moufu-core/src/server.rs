use crate::coalesce::CoalescingDispatcher;
use crate::graph::{LinkGraph, SessionRecord};
use crate::persistence::LinkStore;
use moufu_protocol::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, Mutex, RwLock};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Internal event broadcasted by the hub to GUI / CLI monitors
#[derive(Debug, Clone)]
pub enum HubEvent {
    AppConnected(SessionRecord),
    AppDisconnected(AppSessionId),
    EntityRegistered(EntityDescriptor),
    EntityUnregistered(EntityId),
    LinkCreated(LinkInfo),
    LinkDestroyed(LinkId),
    LinkRebound(LinkInfo),
    ChangeDispatched {
        event: ChangeEvent,
        subscribers_count: usize,
    },
}

/// Channel sender to forward server messages to a specific connected client session
type ClientSender = mpsc::UnboundedSender<ServerMessage>;

/// MoufuHub coordinates sessions, entity links, and real-time message routing.
pub struct MoufuHub {
    graph: Arc<RwLock<LinkGraph>>,
    sessions: Arc<RwLock<HashMap<AppSessionId, (SessionRecord, ClientSender)>>>,
    event_tx: broadcast::Sender<HubEvent>,
    coalescer: Arc<CoalescingDispatcher>,
    store: Arc<LinkStore>,
}

impl MoufuHub {
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(1024);
        let store = Arc::new(LinkStore::default_location());
        let coalescer = Arc::new(CoalescingDispatcher::new(60)); // 60 FPS throttled coalescing

        let mut graph = LinkGraph::new();
        // Restore persisted links from disk on startup
        let saved_links = store.load();
        for link in saved_links {
            graph.restore_link(link);
        }

        let graph = Arc::new(RwLock::new(graph));
        let sessions = Arc::new(RwLock::new(HashMap::new()));

        // Start background worker to dispatch coalesced events
        let graph_clone = Arc::clone(&graph);
        let sessions_clone = Arc::clone(&sessions);
        let event_tx_clone = event_tx.clone();
        let coalescer_clone = Arc::clone(&coalescer);

        tokio::spawn(async move {
            while let Some(event) = coalescer_clone.next_event().await {
                // Update graph state
                graph_clone.write().await.update_entity_state(&event);

                // Broadcast to linked sessions
                let sessions = sessions_clone.read().await;
                let mut count = 0;
                for (_, tx) in sessions.values() {
                    let _ = tx.send(ServerMessage::EntityUpdated(event.clone()));
                    count += 1;
                }

                let _ = event_tx_clone.send(HubEvent::ChangeDispatched {
                    event,
                    subscribers_count: count,
                });
            }
        });

        Self {
            graph,
            sessions,
            event_tx,
            coalescer,
            store,
        }
    }

    pub fn subscribe_hub_events(&self) -> broadcast::Receiver<HubEvent> {
        self.event_tx.subscribe()
    }

    pub async fn get_sessions(&self) -> Vec<SessionRecord> {
        self.sessions
            .read()
            .await
            .values()
            .map(|(rec, _)| rec.clone())
            .collect()
    }

    pub async fn get_links(&self) -> Vec<LinkInfo> {
        self.graph.read().await.list_links()
    }

    pub async fn get_entities(&self) -> Vec<EntityDescriptor> {
        self.graph.read().await.list_entities()
    }

    /// Register a new incoming client connection session
    pub async fn handle_handshake(
        &self,
        app_name: String,
        app_version: String,
        capabilities: AppCapabilities,
        client_tx: ClientSender,
    ) -> AppSessionId {
        let session_id = Uuid::new_v4();
        let record = SessionRecord {
            session_id,
            app_name: app_name.clone(),
            app_version,
            capabilities,
            connected_at: chrono::Utc::now(),
        };

        info!(
            "App connected: {} (Session ID: {})",
            app_name, session_id
        );

        self.sessions
            .write()
            .await
            .insert(session_id, (record.clone(), client_tx.clone()));

        let _ = client_tx.send(ServerMessage::HandshakeAck {
            session_id,
            server_version: env!("CARGO_PKG_VERSION").to_string(),
        });

        let _ = self.event_tx.send(HubEvent::AppConnected(record));

        session_id
    }

    /// Handle client disconnection
    pub async fn handle_disconnect(&self, session_id: AppSessionId) {
        info!("App disconnected (Session ID: {})", session_id);
        self.sessions.write().await.remove(&session_id);
        self.graph.write().await.handle_app_disconnected(session_id);
        let _ = self.event_tx.send(HubEvent::AppDisconnected(session_id));
    }

    /// Process a message received from a connected client
    pub async fn process_message(&self, session_id: AppSessionId, msg: ClientMessage) {
        match msg {
            ClientMessage::Handshake { .. } => {
                // Handshake is already handled during connection establishment
            }
            ClientMessage::RegisterEntity(desc) => {
                info!("Register entity: {} ({})", desc.name, desc.id);
                let rebound = {
                    let mut graph = self.graph.write().await;
                    graph.register_entity(session_id, desc.clone());
                    graph.rebind_links_for_entity(&desc.id)
                };
                let _ = self.event_tx.send(HubEvent::EntityRegistered(desc));

                // If any persisted links became active upon this entity registering, notify sessions
                for link in rebound {
                    info!("Restored persisted link: {} -> {}", link.source_entity, link.target_entity);
                    let _ = self.event_tx.send(HubEvent::LinkRebound(link.clone()));
                    let sessions = self.sessions.read().await;
                    for (_, tx) in sessions.values() {
                        let _ = tx.send(ServerMessage::LinkStatusChanged(link.clone()));
                    }
                }
            }
            ClientMessage::UnregisterEntity { entity_id } => {
                info!("Unregister entity: {}", entity_id);
                self.graph.write().await.unregister_entity(&entity_id);
                let _ = self.event_tx.send(HubEvent::EntityUnregistered(entity_id));
            }
            ClientMessage::CreateLink {
                source_entity,
                target_entity,
            } => {
                let link_info = {
                    let mut graph = self.graph.write().await;
                    let source_app = graph
                        .get_entity(&source_entity)
                        .map(|e| e.source_app.clone())
                        .unwrap_or_else(|| "Unknown".to_string());
                    let target_app = graph
                        .get_entity(&target_entity)
                        .map(|e| e.source_app.clone())
                        .unwrap_or_else(|| "Unknown".to_string());
                    let data_type = graph
                        .get_entity(&source_entity)
                        .map(|e| e.data_type.clone())
                        .unwrap_or_else(|| "application/octet-stream".to_string());

                    let link = graph.create_link(
                        source_app,
                        source_entity.clone(),
                        target_app,
                        target_entity.clone(),
                        data_type,
                    );
                    // Persist updated link topology to disk
                    self.store.save(&graph.list_links());
                    link
                };

                info!(
                    "Dynamic link established and persisted: {} -> {} ({})",
                    source_entity, target_entity, link_info.link_id
                );

                let _ = self
                    .event_tx
                    .send(HubEvent::LinkCreated(link_info.clone()));

                // Send back link confirmation to caller if active
                if let Some((_, tx)) = self.sessions.read().await.get(&session_id) {
                    let _ = tx.send(ServerMessage::LinkStatusChanged(link_info));
                }
            }
            ClientMessage::DestroyLink { link_id } => {
                let removed = {
                    let mut graph = self.graph.write().await;
                    let opt = graph.destroy_link(&link_id);
                    if opt.is_some() {
                        self.store.save(&graph.list_links());
                    }
                    opt
                };
                if let Some(_) = removed {
                    info!("Link destroyed: {}", link_id);
                    let _ = self.event_tx.send(HubEvent::LinkDestroyed(link_id));
                }
            }
            ClientMessage::NotifyChange(event) => {
                debug!(
                    "Queued change event for {}: version {}, transient={}",
                    event.entity_id, event.version, event.is_transient
                );
                // Dispatch through the 60fps coalescing queue
                self.coalescer.submit(event).await;
            }
            ClientMessage::PullEntity { entity_id } => {
                let state = self.graph.read().await.get_entity_state(&entity_id);
                if let Some((_, tx)) = self.sessions.read().await.get(&session_id) {
                    if let Some((version, payload)) = state {
                        let _ = tx.send(ServerMessage::EntityState {
                            entity_id,
                            version,
                            payload,
                        });
                    } else {
                        let _ = tx.send(ServerMessage::Error {
                            code: "ENTITY_NOT_FOUND".to_string(),
                            message: format!("Entity '{}' has no recorded state.", entity_id),
                        });
                    }
                }
            }
            ClientMessage::Ping => {
                if let Some((_, tx)) = self.sessions.read().await.get(&session_id) {
                    let _ = tx.send(ServerMessage::Pong);
                }
            }
        }
    }
}
