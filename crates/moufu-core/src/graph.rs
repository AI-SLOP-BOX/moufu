use moufu_protocol::*;
use std::collections::HashMap;

/// Internal record of an active application session
#[derive(Debug, Clone)]
pub struct SessionRecord {
    pub session_id: AppSessionId,
    pub app_name: String,
    pub app_version: String,
    pub capabilities: AppCapabilities,
    pub connected_at: chrono::DateTime<chrono::Utc>,
}

/// LinkGraph manages registered entities and the links between them.
#[derive(Debug, Default)]
pub struct LinkGraph {
    /// Entities currently known: EntityId -> Descriptor
    entities: HashMap<EntityId, EntityDescriptor>,
    /// Entity latest cached state and version: EntityId -> (version, payload)
    entity_states: HashMap<EntityId, (u64, PayloadTransport)>,
    /// Entity ownership: EntityId -> AppSessionId
    entity_owners: HashMap<EntityId, AppSessionId>,
    /// Active links: LinkId -> LinkInfo
    links: HashMap<LinkId, LinkInfo>,
    /// Target entity to links map: target_entity -> [LinkId]
    target_to_links: HashMap<EntityId, Vec<LinkId>>,
    /// Source entity to links map: source_entity -> [LinkId]
    source_to_links: HashMap<EntityId, Vec<LinkId>>,
}

impl LinkGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_entity(&mut self, session_id: AppSessionId, descriptor: EntityDescriptor) {
        let id = descriptor.id.clone();
        self.entities.insert(id.clone(), descriptor);
        self.entity_owners.insert(id, session_id);
    }

    pub fn unregister_entity(&mut self, entity_id: &EntityId) {
        self.entities.remove(entity_id);
        self.entity_owners.remove(entity_id);
        self.entity_states.remove(entity_id);

        // Mark associated links as broken
        if let Some(link_ids) = self.source_to_links.get(entity_id) {
            for lid in link_ids {
                if let Some(link) = self.links.get_mut(lid) {
                    link.status = LinkStatus::Broken;
                }
            }
        }
    }

    pub fn get_entity(&self, id: &EntityId) -> Option<&EntityDescriptor> {
        self.entities.get(id)
    }

    pub fn list_entities(&self) -> Vec<EntityDescriptor> {
        self.entities.values().cloned().collect()
    }

    pub fn create_link(
        &mut self,
        source_app: String,
        source_entity: EntityId,
        target_app: String,
        target_entity: EntityId,
        data_type: String,
    ) -> LinkInfo {
        let link_id = LinkId::new();
        let link = LinkInfo {
            link_id,
            source_app,
            source_entity: source_entity.clone(),
            target_app,
            target_entity: target_entity.clone(),
            data_type,
            status: LinkStatus::Active,
            last_synced_version: 0,
            created_at: chrono::Utc::now(),
        };

        self.links.insert(link_id, link.clone());
        self.source_to_links
            .entry(source_entity)
            .or_default()
            .push(link_id);
        self.target_to_links
            .entry(target_entity)
            .or_default()
            .push(link_id);

        link
    }

    pub fn destroy_link(&mut self, link_id: &LinkId) -> Option<LinkInfo> {
        if let Some(link) = self.links.remove(link_id) {
            if let Some(sources) = self.source_to_links.get_mut(&link.source_entity) {
                sources.retain(|id| id != link_id);
            }
            if let Some(targets) = self.target_to_links.get_mut(&link.target_entity) {
                targets.retain(|id| id != link_id);
            }
            Some(link)
        } else {
            None
        }
    }

    pub fn list_links(&self) -> Vec<LinkInfo> {
        self.links.values().cloned().collect()
    }

    pub fn get_links_for_source(&self, source_entity: &EntityId) -> Vec<LinkInfo> {
        self.source_to_links
            .get(source_entity)
            .map(|ids| ids.iter().filter_map(|id| self.links.get(id).cloned()).collect())
            .unwrap_or_default()
    }

    pub fn update_entity_state(&mut self, event: &ChangeEvent) {
        self.entity_states
            .insert(event.entity_id.clone(), (event.version, event.payload.clone()));

        // Update synced version on matching links
        if let Some(link_ids) = self.source_to_links.get(&event.entity_id) {
            for lid in link_ids {
                if let Some(link) = self.links.get_mut(lid) {
                    link.last_synced_version = event.version;
                }
            }
        }
    }

    pub fn get_entity_state(&self, entity_id: &EntityId) -> Option<(u64, PayloadTransport)> {
        self.entity_states.get(entity_id).cloned()
    }

    /// Restore a persisted link into the graph (initially Broken until both apps/entities are present)
    pub fn restore_link(&mut self, mut link: LinkInfo) {
        let has_source = self.entities.contains_key(&link.source_entity);
        let has_target = self.entities.contains_key(&link.target_entity);

        if has_source && has_target {
            link.status = LinkStatus::Active;
        } else {
            link.status = LinkStatus::Broken;
        }

        self.source_to_links
            .entry(link.source_entity.clone())
            .or_default()
            .push(link.link_id);
        self.target_to_links
            .entry(link.target_entity.clone())
            .or_default()
            .push(link.link_id);
        self.links.insert(link.link_id, link);
    }

    /// Check if any broken links can now be re-bound to Active after an entity is registered
    pub fn rebind_links_for_entity(&mut self, entity_id: &EntityId) -> Vec<LinkInfo> {
        let mut activated = Vec::new();
        // Check links where this entity is source or target
        let mut candidates = Vec::new();
        if let Some(lids) = self.source_to_links.get(entity_id) {
            candidates.extend(lids.clone());
        }
        if let Some(lids) = self.target_to_links.get(entity_id) {
            candidates.extend(lids.clone());
        }

        for lid in candidates {
            if let Some(link) = self.links.get_mut(&lid) {
                if link.status == LinkStatus::Broken {
                    let has_source = self.entities.contains_key(&link.source_entity);
                    let has_target = self.entities.contains_key(&link.target_entity);
                    if has_source && has_target {
                        link.status = LinkStatus::Active;
                        activated.push(link.clone());
                    }
                }
            }
        }
        activated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_link_creation_and_rebind() {
        let mut graph = LinkGraph::new();
        let session1 = Uuid::new_v4();
        let session2 = Uuid::new_v4();

        let source_id = EntityId::from("amata://artboard-1/star");
        let target_id = EntityId::from("kagari://comp-1/clip");

        // 1. Simulate restoring a persisted link before entities register
        let persisted_link = LinkInfo {
            link_id: LinkId::new(),
            source_app: "Amata".to_string(),
            source_entity: source_id.clone(),
            target_app: "Kagari".to_string(),
            target_entity: target_id.clone(),
            data_type: "application/x-moufu-vector".to_string(),
            status: LinkStatus::Broken,
            last_synced_version: 0,
            created_at: chrono::Utc::now(),
        };
        graph.restore_link(persisted_link);

        // Link should initially be Broken because entities are missing
        let links = graph.list_links();
        assert_eq!(links[0].status, LinkStatus::Broken);

        // 2. Register Amata's entity
        graph.register_entity(session1, EntityDescriptor {
            id: source_id.clone(),
            name: "Star".to_string(),
            data_type: "application/x-moufu-vector".to_string(),
            source_app: "Amata".to_string(),
            metadata: HashMap::new(),
        });
        let activated = graph.rebind_links_for_entity(&source_id);
        assert!(activated.is_empty(), "Kagari is still missing, link should remain Broken");

        // 3. Register Kagari's entity
        graph.register_entity(session2, EntityDescriptor {
            id: target_id.clone(),
            name: "Clip".to_string(),
            data_type: "application/x-moufu-vector".to_string(),
            source_app: "Kagari".to_string(),
            metadata: HashMap::new(),
        });
        let activated = graph.rebind_links_for_entity(&target_id);
        assert_eq!(activated.len(), 1, "Both entities present; link must rebind to Active");
        assert_eq!(activated[0].status, LinkStatus::Active);
    }
}
