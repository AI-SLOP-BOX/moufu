use moufu_protocol::LinkInfo;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{error, info, warn};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct RegistrySnapshot {
    pub links: Vec<LinkInfo>,
}

/// LinkStore provides persistence of link topologies across application and Moufu restarts.
pub struct LinkStore {
    storage_path: PathBuf,
}

impl LinkStore {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self {
            storage_path: path.as_ref().to_path_buf(),
        }
    }

    pub fn default_location() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let dir = PathBuf::from(home).join(".moufu");
        let _ = fs::create_dir_all(&dir);
        Self::new(dir.join("links_registry.json"))
    }

    /// Load persisted links from disk
    pub fn load(&self) -> Vec<LinkInfo> {
        if !self.storage_path.exists() {
            return Vec::new();
        }
        match fs::read_to_string(&self.storage_path) {
            Ok(content) => match serde_json::from_str::<RegistrySnapshot>(&content) {
                Ok(snapshot) => {
                    info!(
                        "Loaded {} persisted links from {:?}",
                        snapshot.links.len(),
                        self.storage_path
                    );
                    snapshot.links
                }
                Err(e) => {
                    warn!("Failed to parse link registry at {:?}: {}", self.storage_path, e);
                    Vec::new()
                }
            },
            Err(e) => {
                warn!("Failed to read link registry at {:?}: {}", self.storage_path, e);
                Vec::new()
            }
        }
    }

    /// Save current links to disk
    pub fn save(&self, links: &[LinkInfo]) {
        let snapshot = RegistrySnapshot {
            links: links.to_vec(),
        };
        if let Ok(json) = serde_json::to_string_pretty(&snapshot) {
            if let Err(e) = fs::write(&self.storage_path, json) {
                error!("Failed to save links registry to {:?}: {}", self.storage_path, e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moufu_protocol::{EntityId, LinkId, LinkStatus};

    #[test]
    fn test_link_store_save_and_load() {
        let temp_dir = std::env::temp_dir().join(format!("moufu_test_{}", uuid::Uuid::new_v4()));
        let _ = fs::create_dir_all(&temp_dir);
        let file_path = temp_dir.join("test_links.json");

        let store = LinkStore::new(&file_path);
        let link = LinkInfo {
            link_id: LinkId::new(),
            source_app: "Amata".to_string(),
            source_entity: EntityId::from("amata://artboard/star"),
            target_app: "Kagari".to_string(),
            target_entity: EntityId::from("kagari://timeline/clip"),
            data_type: "application/x-moufu-vector".to_string(),
            status: LinkStatus::Active,
            last_synced_version: 15,
            created_at: chrono::Utc::now(),
        };

        store.save(&[link.clone()]);
        let loaded = store.load();

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].link_id, link.link_id);
        assert_eq!(loaded[0].source_entity, link.source_entity);
        assert_eq!(loaded[0].last_synced_version, 15);

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
