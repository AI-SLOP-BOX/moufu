use moufu_adapter_sdk::MoufuClient;
use moufu_protocol::{AppCapabilities, EntityId, VectorObjectData};
use std::collections::HashMap;
use tokio::time::{sleep, Duration};
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    info!("Starting Amata (Vector Design Tool Mock)...");

    let capabilities = AppCapabilities {
        supports_live_link: true,
        supports_push_edits: true,
        supports_delta_sync: false,
        supports_link_restoration: true,
        supported_data_types: vec!["application/x-moufu-vector".to_string()],
    };

    let client = MoufuClient::connect("127.0.0.1:9478", "Amata", "1.0.0", capabilities).await?;
    info!("Connected to Moufu Hub!");

    let entity_id = EntityId::from("amata://artboard-1/vector-star-1");

    // 1. Amata registers the vector object entity
    let mut metadata = HashMap::new();
    metadata.insert("author".to_string(), "Amata Vector Engine".to_string());
    client.register_entity(
        entity_id.clone(),
        "Golden Star Vector".to_string(),
        "application/x-moufu-vector".to_string(),
        "Amata".to_string(),
        metadata,
    )?;
    info!("Registered entity: {}", entity_id);

    sleep(Duration::from_millis(500)).await;

    // Initial state of the vector object
    let mut star = VectorObjectData {
        shape_type: "star".to_string(),
        fill_color: "#FFD700".to_string(), // Gold
        stroke_color: "#FF8C00".to_string(),
        stroke_width: 2.0,
        points: vec![
            [0.0, -50.0],
            [14.0, -20.0],
            [47.0, -15.0],
            [23.0, 7.0],
            [29.0, 40.0],
            [0.0, 25.0],
            [-29.0, 40.0],
            [-23.0, 7.0],
            [-47.0, -15.0],
            [-14.0, -20.0],
        ],
        rotation: 0.0,
        scale: [1.0, 1.0],
        position: [200.0, 200.0],
    };

    // Send initial version
    client.notify_change(
        entity_id.clone(),
        1,
        false,
        serde_json::to_value(&star)?,
    )?;
    info!("Sent initial state for {}", entity_id);

    // 4. Simulate user editing the object live without saving!
    // Phase A: Rapid dragging (100 updates in 1 second, simulating 100Hz mouse drag)
    info!("Phase A: Rapid mouse drag simulation (100 transient updates)...");
    for v in 2..=100 {
        sleep(Duration::from_millis(10)).await; // 100Hz rapid burst
        star.rotation += 3.6;
        star.position[0] += 1.0;
        // Coalescing in Moufu Hub ensures Kagari doesn't lag or drop behind
        client.notify_change(
            entity_id.clone(),
            v,
            true, // unsaved live drag
            serde_json::to_value(&star)?,
        )?;
    }
    info!("Phase A complete: Coalescer successfully absorbed 100 transient events.");

    // Phase B: Final save commit (is_transient = false)
    sleep(Duration::from_millis(1000)).await;
    info!("Phase B: User saved changes in Amata (is_transient = false)");
    client.notify_change(
        entity_id.clone(),
        101,
        false, // committed save
        serde_json::to_value(&star)?,
    )?;

    // Phase C: Demonstrate Shared Memory frame transfer
    sleep(Duration::from_millis(1000)).await;
    info!("Phase C: Exporting rendered raster frame via Shared Memory zero-copy...");
    let shm_desc = moufu_protocol::SharedMemoryDescriptor {
        shm_id: "moufu_shm_amata_frame_001".to_string(),
        size_bytes: 1920 * 1080 * 4,
        width: 1920,
        height: 1080,
        format: "RGBA8".to_string(),
        stride: 1920 * 4,
    };
    client.notify_change_shm(entity_id.clone(), 102, true, shm_desc)?;

    info!("Simulation complete. Keeping Amata alive to maintain session...");
    loop {
        sleep(Duration::from_secs(60)).await;
    }
}
