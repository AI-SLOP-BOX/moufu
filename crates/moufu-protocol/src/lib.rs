use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Unique identifier for an application instance connection.
pub type AppSessionId = Uuid;

/// Identifier for an entity (e.g., document, artboard, layer, vector object, clip).
/// Usually formatted like "amata://doc-1/vector-star-1" or "kagari://timeline-1/track-2/clip-1".
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntityId(pub String);

impl std::fmt::Display for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl<T: Into<String>> From<T> for EntityId {
    fn from(s: T) -> Self {
        Self(s.into())
    }
}

/// Identifier for a link established between a source entity and a target entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LinkId(pub Uuid);

impl LinkId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for LinkId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for LinkId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Capabilities declared by an application upon connecting to Moufu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppCapabilities {
    /// Can notify live, unsaved edits in real time
    pub supports_live_link: bool,
    /// Can receive and apply remote edits from Moufu
    pub supports_push_edits: bool,
    /// Supports granular delta/patch sync instead of full payloads
    pub supports_delta_sync: bool,
    /// Can be restored automatically across restarts
    pub supports_link_restoration: bool,
    /// Supported MIME/Data types (e.g. "application/x-moufu-vector", "image/svg+xml", "video/raw")
    pub supported_data_types: Vec<String>,
}

impl Default for AppCapabilities {
    fn default() -> Self {
        Self {
            supports_live_link: true,
            supports_push_edits: false,
            supports_delta_sync: false,
            supports_link_restoration: true,
            supported_data_types: vec!["application/json".to_string()],
        }
    }
}

/// Metadata and registration info of an entity exported by an application.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityDescriptor {
    pub id: EntityId,
    pub name: String,
    pub data_type: String,
    pub source_app: String,
    pub metadata: HashMap<String, String>,
}

/// Descriptor for shared memory buffer (used for high-bandwidth frames/textures)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedMemoryDescriptor {
    pub shm_id: String,
    pub size_bytes: usize,
    pub width: u32,
    pub height: u32,
    pub format: String, // e.g. "RGBA8", "BGRA8", "RAW"
    pub stride: usize,
}

/// Transport mechanism for event payload
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "transport", content = "content")]
pub enum PayloadTransport {
    /// Inlined JSON or binary object
    Inline(serde_json::Value),
    /// Zero-copy shared memory buffer pointer/descriptor
    SharedMemory(SharedMemoryDescriptor),
}

/// Change event carrying updated entity state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangeEvent {
    pub entity_id: EntityId,
    pub version: u64,
    /// If true, this change is in-flight/unsaved (live preview).
    /// If false, it has been committed/saved to disk in the source application.
    pub is_transient: bool,
    pub timestamp: DateTime<Utc>,
    /// Payload containing the updated entity data
    pub payload: PayloadTransport,
    /// Optional delta/patch representation if supported
    pub delta: Option<serde_json::Value>,
}

/// Status of a dynamic link between two entities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LinkStatus {
    Active,
    OutOfSync,
    Broken,
    Paused,
}

/// Information describing a link between source and destination entities.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinkInfo {
    pub link_id: LinkId,
    pub source_app: String,
    pub source_entity: EntityId,
    pub target_app: String,
    pub target_entity: EntityId,
    pub data_type: String,
    pub status: LinkStatus,
    pub last_synced_version: u64,
    pub created_at: DateTime<Utc>,
}

/// Messages sent from an Application (Adapter) to Moufu Core.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ClientMessage {
    /// Handshake initiating connection and declaring capabilities
    Handshake {
        app_name: String,
        app_version: String,
        capabilities: AppCapabilities,
    },
    /// Register an entity that this app owns and makes available for linking
    RegisterEntity(EntityDescriptor),
    /// Unregister an entity
    UnregisterEntity {
        entity_id: EntityId,
    },
    /// Request to create a link from a source entity to a target entity
    CreateLink {
        source_entity: EntityId,
        target_entity: EntityId,
    },
    /// Remove an existing link
    DestroyLink {
        link_id: LinkId,
    },
    /// Emit a live or committed edit change event
    NotifyChange(ChangeEvent),
    /// Request current state of an entity
    PullEntity {
        entity_id: EntityId,
    },
    /// Heartbeat ping
    Ping,
}

/// Messages sent from Moufu Core to an Application (Adapter).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ServerMessage {
    /// Handshake acknowledgment
    HandshakeAck {
        session_id: AppSessionId,
        server_version: String,
    },
    /// Notification that an entity linked to this app was updated
    EntityUpdated(ChangeEvent),
    /// Notification about link creation or status update
    LinkStatusChanged(LinkInfo),
    /// Response with the latest state of an entity
    EntityState {
        entity_id: EntityId,
        version: u64,
        payload: PayloadTransport,
    },
    /// General notification or error
    Error {
        code: String,
        message: String,
    },
    /// Heartbeat pong
    Pong,
}

/// Vector object data model (used in Amata & Kagari MVP)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorObjectData {
    pub shape_type: String, // "star", "rectangle", "circle", "bezier"
    pub fill_color: String, // Hex string or RGBA
    pub stroke_color: String,
    pub stroke_width: f32,
    pub points: Vec<[f32; 2]>,
    pub rotation: f32,
    pub scale: [f32; 2],
    pub position: [f32; 2],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_message_serialization() {
        let msg = ClientMessage::NotifyChange(ChangeEvent {
            entity_id: EntityId::from("amata://artboard-1/vector-star-1"),
            version: 42,
            is_transient: true,
            timestamp: Utc::now(),
            payload: PayloadTransport::Inline(serde_json::json!({ "rotation": 45.0 })),
            delta: None,
        });

        let json = serde_json::to_string(&msg).unwrap();
        let parsed: ClientMessage = serde_json::from_str(&json).unwrap();
        if let ClientMessage::NotifyChange(event) = parsed {
            assert_eq!(event.version, 42);
            assert!(event.is_transient);
            assert_eq!(event.entity_id.0, "amata://artboard-1/vector-star-1");
        } else {
            panic!("Mismatched message variant");
        }
    }

    #[test]
    fn test_shared_memory_transport_serialization() {
        let shm = SharedMemoryDescriptor {
            shm_id: "moufu_shm_test_123".to_string(),
            size_bytes: 1920 * 1080 * 4,
            width: 1920,
            height: 1080,
            format: "RGBA8".to_string(),
            stride: 1920 * 4,
        };
        let transport = PayloadTransport::SharedMemory(shm.clone());
        let json = serde_json::to_string(&transport).unwrap();
        let parsed: PayloadTransport = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, PayloadTransport::SharedMemory(shm));
    }
}
