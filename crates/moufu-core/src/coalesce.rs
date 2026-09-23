use moufu_protocol::{ChangeEvent, EntityId};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio::time::{interval, Duration, Instant};

/// CoalescingBuffer aggregates rapid transient changes (e.g. 120Hz drag updates)
/// and outputs them at a regulated frame rate (e.g. 60Hz), preventing receiver overload.
pub struct CoalescingDispatcher {
    /// Pending transient events: EntityId -> latest ChangeEvent
    pending_transient: Arc<Mutex<HashMap<EntityId, ChangeEvent>>>,
    /// Queue for non-transient (committed) events that must never be dropped
    committed_tx: mpsc::UnboundedSender<ChangeEvent>,
    /// Outgoing channel for dispatching merged/throttled events to subscribers
    out_rx: Arc<Mutex<mpsc::UnboundedReceiver<ChangeEvent>>>,
}

impl CoalescingDispatcher {
    pub fn new(fps: u32) -> Self {
        let (out_tx, out_rx) = mpsc::unbounded_channel::<ChangeEvent>();
        let (committed_tx, mut committed_rx) = mpsc::unbounded_channel::<ChangeEvent>();
        let pending_transient = Arc::new(Mutex::new(HashMap::<EntityId, ChangeEvent>::new()));

        let pending_clone = Arc::clone(&pending_transient);
        let out_tx_clone = out_tx.clone();

        let frame_duration = Duration::from_millis((1000.0 / fps.max(1) as f64) as u64);

        // Background worker loop running at target FPS interval
        tokio::spawn(async move {
            let mut ticker = interval(frame_duration);
            loop {
                ticker.tick().await;

                // 1. Drain all committed (saved) events first (never dropped)
                while let Ok(event) = committed_rx.try_recv() {
                    let _ = out_tx_clone.send(event);
                }

                // 2. Drain all pending transient events (only latest state per entity)
                let mut guard = pending_clone.lock().await;
                for (_, event) in guard.drain() {
                    let _ = out_tx_clone.send(event);
                }
            }
        });

        Self {
            pending_transient,
            committed_tx,
            out_rx: Arc::new(Mutex::new(out_rx)),
        }
    }

    /// Submit an event to the dispatcher.
    /// Transient events will be coalesced (only newest kept per EntityId).
    /// Committed events are strictly queued without loss.
    pub async fn submit(&self, event: ChangeEvent) {
        if event.is_transient {
            let mut guard = self.pending_transient.lock().await;
            guard.insert(event.entity_id.clone(), event);
        } else {
            let _ = self.committed_tx.send(event);
        }
    }

    /// Pull the next ready coalesced event
    pub async fn next_event(&self) -> Option<ChangeEvent> {
        let mut rx = self.out_rx.lock().await;
        rx.recv().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moufu_protocol::PayloadTransport;

    #[tokio::test]
    async fn test_transient_event_coalescing() {
        // High-rate dispatcher (10fps for testing)
        let dispatcher = CoalescingDispatcher::new(10);
        let entity_id = EntityId::from("amata://artboard-1/star");

        // Submit 20 rapid transient events
        for v in 1..=20 {
            dispatcher.submit(ChangeEvent {
                entity_id: entity_id.clone(),
                version: v,
                is_transient: true,
                timestamp: chrono::Utc::now(),
                payload: PayloadTransport::Inline(serde_json::json!({ "v": v })),
                delta: None,
            }).await;
        }

        // Wait for next ticker flush
        let flushed = dispatcher.next_event().await.unwrap();
        // Since transient events are coalesced, we receive the latest version, not 20 individual events
        assert_eq!(flushed.version, 20);
        assert!(flushed.is_transient);
    }
}
