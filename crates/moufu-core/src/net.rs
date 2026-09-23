use crate::MoufuHub;
use moufu_protocol::*;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tracing::{error, info, warn};

pub async fn run_tcp_server(hub: Arc<MoufuHub>, bind_addr: &str) -> anyhow::Result<()> {
    let listener = TcpListener::bind(bind_addr).await?;
    info!("Moufu Core Hub listening on {}", bind_addr);

    loop {
        let (socket, remote_addr) = listener.accept().await?;
        info!("Accepted connection from {}", remote_addr);
        let hub_clone = Arc::clone(&hub);

        tokio::spawn(async move {
            if let Err(e) = handle_connection(hub_clone, socket).await {
                warn!("Connection from {} closed with error: {:?}", remote_addr, e);
            }
        });
    }
}

async fn handle_connection(hub: Arc<MoufuHub>, mut stream: TcpStream) -> anyhow::Result<()> {
    let (reader, mut writer) = stream.split();
    let mut lines = BufReader::new(reader).lines();

    // Expect first line to be Handshake
    let first_line = match lines.next_line().await? {
        Some(l) => l,
        None => return Ok(()),
    };

    let handshake_msg: ClientMessage = serde_json::from_str(&first_line)?;
    let (app_name, app_version, capabilities) = match handshake_msg {
        ClientMessage::Handshake {
            app_name,
            app_version,
            capabilities,
        } => (app_name, app_version, capabilities),
        _ => {
            return Err(anyhow::anyhow!(
                "First message must be Handshake"
            ))
        }
    };

    let (client_tx, mut client_rx) = tokio::sync::mpsc::unbounded_channel::<ServerMessage>();
    let session_id = hub
        .handle_handshake(app_name, app_version, capabilities, client_tx)
        .await;

    // Spawn task to send server messages to client socket
    let write_task = tokio::spawn(async move {
        while let Some(msg) = client_rx.recv().await {
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

    // Read loop for messages from client socket
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<ClientMessage>(&line) {
            Ok(msg) => {
                hub.process_message(session_id, msg).await;
            }
            Err(e) => {
                warn!("Failed to parse client message: {} (input: {})", e, line);
            }
        }
    }

    hub.handle_disconnect(session_id).await;
    write_task.abort();
    Ok(())
}
