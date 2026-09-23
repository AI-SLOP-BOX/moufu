use moufu_adapter_sdk::MoufuClient;
use moufu_protocol::{AppCapabilities, EntityId, ServerMessage, VectorObjectData};
use std::collections::HashMap;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    info!("Starting Kagari (Video Editing & Compositing Mock)...");

    let capabilities = AppCapabilities {
        supports_live_link: true,
        supports_push_edits: false,
        supports_delta_sync: false,
        supports_link_restoration: true,
        supported_data_types: vec![
            "application/x-moufu-vector".to_string(),
            "video/raw".to_string(),
        ],
    };

    let client = MoufuClient::connect("127.0.0.1:9478", "Kagari", "1.0.0", capabilities).await?;
    info!("Connected to Moufu Hub!");

    let mut event_rx = client.subscribe();

    let target_clip_id = EntityId::from("kagari://timeline-1/track-2/clip-vector-1");
    let source_amata_id = EntityId::from("amata://artboard-1/vector-star-1");

    // 1. Kagari registers its timeline clip entity
    let mut metadata = HashMap::new();
    metadata.insert("track".to_string(), "Track 2".to_string());
    client.register_entity(
        target_clip_id.clone(),
        "Composited Vector Clip".to_string(),
        "application/x-moufu-vector".to_string(),
        "Kagari".to_string(),
        metadata,
    )?;

    // 2. Kagari links the clip to Amata's vector object
    client.create_link(source_amata_id.clone(), target_clip_id.clone())?;
    info!(
        "Created dynamic link: {} ---> {}",
        source_amata_id, target_clip_id
    );

    // 3. Kagari listens for real-time live link updates from Amata
    info!("Kagari is now monitoring dynamic link for live edits...");

    while let Ok(msg) = event_rx.recv().await {
        match msg {
            ServerMessage::EntityUpdated(change) => {
                if change.entity_id == source_amata_id {
                    match change.payload {
                        moufu_protocol::PayloadTransport::Inline(val) => {
                            if let Ok(vector) = serde_json::from_value::<VectorObjectData>(val) {
                                info!(
                                    "🔥 [KAGARI VIEWPORT UPDATED] Live Link synced! (v{}, transient={}) -> Color: {}, Rot: {}°, Pos: {:?}",
                                    change.version, change.is_transient, vector.fill_color, vector.rotation, vector.position
                                );
                            }
                        }
                        moufu_protocol::PayloadTransport::SharedMemory(shm) => {
                            info!(
                                "⚡ [KAGARI ZERO-COPY FRAME] Shm Buffer Received: ID={}, {}x{}, Format={}",
                                shm.shm_id, shm.width, shm.height, shm.format
                            );
                        }
                    }
                }
            }
            ServerMessage::LinkStatusChanged(link) => {
                info!("Link status updated: {:?} ({})", link.status, link.link_id);
            }
            _ => {}
        }
    }

    Ok(())
}
