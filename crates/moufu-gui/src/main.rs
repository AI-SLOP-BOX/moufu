use eframe::egui;
use moufu_core::{HubEvent, MoufuHub, SessionRecord};
use moufu_protocol::{EntityDescriptor, LinkInfo, LinkStatus};
use std::sync::Arc;
use tokio::sync::broadcast;

pub struct MoufuControlCenterApp {
    hub: Arc<MoufuHub>,
    event_rx: broadcast::Receiver<HubEvent>,
    sessions: Vec<SessionRecord>,
    links: Vec<LinkInfo>,
    entities: Vec<EntityDescriptor>,
    logs: Vec<String>,
    event_count: usize,
}

impl MoufuControlCenterApp {
    pub fn new(hub: Arc<MoufuHub>, _cc: &eframe::CreationContext<'_>) -> Self {
        let event_rx = hub.subscribe_hub_events();
        Self {
            hub,
            event_rx,
            sessions: Vec::new(),
            links: Vec::new(),
            entities: Vec::new(),
            logs: vec!["Moufu Integration Hub Initialized.".to_string()],
            event_count: 0,
        }
    }

    fn check_events(&mut self) {
        while let Ok(event) = self.event_rx.try_recv() {
            self.event_count += 1;
            match event {
                HubEvent::AppConnected(session) => {
                    self.logs.push(format!(
                        "App connected: {} (ID: {})",
                        session.app_name, session.session_id
                    ));
                    self.sessions.retain(|s| s.session_id != session.session_id);
                    self.sessions.push(session);
                }
                HubEvent::AppDisconnected(session_id) => {
                    self.logs.push(format!("App disconnected (ID: {})", session_id));
                    self.sessions.retain(|s| s.session_id != session_id);
                    for l in &mut self.links {
                        // Mark broken if source app disconnected
                    }
                }
                HubEvent::EntityRegistered(entity) => {
                    self.logs.push(format!(
                        "Entity registered: {} [App: {}]",
                        entity.id, entity.source_app
                    ));
                    self.entities.retain(|e| e.id != entity.id);
                    self.entities.push(entity);
                }
                HubEvent::EntityUnregistered(entity_id) => {
                    self.logs.push(format!("Entity unregistered: {}", entity_id));
                    self.entities.retain(|e| e.id != entity_id);
                }
                HubEvent::LinkCreated(link) => {
                    self.logs.push(format!(
                        "Link established: {} -> {}",
                        link.source_entity, link.target_entity
                    ));
                    self.links.retain(|l| l.link_id != link.link_id);
                    self.links.push(link);
                }
                HubEvent::LinkDestroyed(link_id) => {
                    self.logs.push(format!("Link removed: {}", link_id));
                    self.links.retain(|l| l.link_id != link_id);
                }
                HubEvent::LinkRebound(link) => {
                    self.logs.push(format!(
                        "🔄 Restored persisted link: {} -> {}",
                        link.source_entity, link.target_entity
                    ));
                    self.links.retain(|l| l.link_id != link.link_id);
                    self.links.push(link);
                }
                HubEvent::ChangeDispatched {
                    event,
                    subscribers_count,
                } => {
                    self.logs.push(format!(
                        "Event synced: {} (v{}, transient={}) -> {} targets",
                        event.entity_id, event.version, event.is_transient, subscribers_count
                    ));
                    // Update version in UI links
                    for l in &mut self.links {
                        if l.source_entity == event.entity_id {
                            l.last_synced_version = event.version;
                        }
                    }
                }
            }
        }
        if self.logs.len() > 100 {
            self.logs.drain(0..self.logs.len() - 100);
        }
    }
}

impl eframe::App for MoufuControlCenterApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.check_events();
        ctx.request_repaint_after(std::time::Duration::from_millis(100));

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.label(egui::RichText::new("Moufu Integration Hub").strong().size(18.0));
                ui.separator();
                ui.label(format!("Active Apps: {}", self.sessions.len()));
                ui.label(format!("Active Links: {}", self.links.len()));
                ui.label(format!("Events Processed: {}", self.event_count));
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Control Center Dashboard");
            ui.add_space(8.0);

            // Top: Connected Applications
            ui.group(|ui| {
                ui.heading("1. Connected Applications & Capabilities");
                if self.sessions.is_empty() {
                    ui.label(egui::RichText::new("No applications connected.").italics());
                } else {
                    egui::Grid::new("sessions_grid").striped(true).show(ui, |ui| {
                        ui.strong("App Name");
                        ui.strong("Version");
                        ui.strong("Live Link");
                        ui.strong("Push Edits");
                        ui.strong("Supported Types");
                        ui.end_row();

                        for s in &self.sessions {
                            ui.label(&s.app_name);
                            ui.label(&s.app_version);
                            ui.label(if s.capabilities.supports_live_link { "Yes" } else { "No" });
                            ui.label(if s.capabilities.supports_push_edits { "Yes" } else { "No" });
                            ui.label(s.capabilities.supported_data_types.join(", "));
                            ui.end_row();
                        }
                    });
                }
            });

            ui.add_space(8.0);

            // Middle: Active Dynamic Links
            ui.group(|ui| {
                ui.heading("2. Dynamic Links & Synchronization");
                if self.links.is_empty() {
                    ui.label(egui::RichText::new("No active dynamic links.").italics());
                } else {
                    egui::Grid::new("links_grid").striped(true).show(ui, |ui| {
                        ui.strong("Source (Publisher)");
                        ui.strong("Target (Subscriber)");
                        ui.strong("Data Type");
                        ui.strong("Status");
                        ui.strong("Version");
                        ui.end_row();

                        for l in &self.links {
                            ui.label(format!("{}: {}", l.source_app, l.source_entity));
                            ui.label(format!("{}: {}", l.target_app, l.target_entity));
                            ui.label(&l.data_type);
                            let status_text = match l.status {
                                LinkStatus::Active => egui::RichText::new("Active").color(egui::Color32::GREEN),
                                LinkStatus::OutOfSync => egui::RichText::new("OutOfSync").color(egui::Color32::YELLOW),
                                LinkStatus::Broken => egui::RichText::new("Broken").color(egui::Color32::RED),
                                LinkStatus::Paused => egui::RichText::new("Paused").color(egui::Color32::GRAY),
                            };
                            ui.label(status_text);
                            ui.label(format!("v{}", l.last_synced_version));
                            ui.end_row();
                        }
                    });
                }
            });

            ui.add_space(8.0);

            // Bottom: Real-time Event Monitor Log
            ui.group(|ui| {
                ui.heading("3. Real-time Activity Log");
                egui::ScrollArea::vertical().max_height(200.0).stick_to_bottom(true).show(ui, |ui| {
                    for log in &self.logs {
                        ui.monospace(log);
                    }
                });
            });
        });
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let hub = Arc::new(MoufuHub::new());
    let hub_for_server = Arc::clone(&hub);

    // Launch TCP Server in background on 127.0.0.1:9478
    tokio::spawn(async move {
        if let Err(e) = moufu_core::run_tcp_server(hub_for_server, "127.0.0.1:9478").await {
            tracing::error!("Server error: {:?}", e);
        }
    });

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([880.0, 640.0])
            .with_min_inner_size([600.0, 400.0])
            .with_title("Moufu Integration Hub — Control Center"),
        ..Default::default()
    };

    let hub_for_gui = Arc::clone(&hub);
    eframe::run_native(
        "Moufu Control Center",
        native_options,
        Box::new(move |cc| Ok(Box::new(MoufuControlCenterApp::new(hub_for_gui, cc)))),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {:?}", e))?;

    Ok(())
}
