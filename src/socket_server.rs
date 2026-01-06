use crate::ipc::{get_socket_path, IpcCommand, IpcResponse};
use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Guard that removes the socket file when dropped
pub struct SocketGuard {
    path: PathBuf,
}

impl Drop for SocketGuard {
    fn drop(&mut self) {
        if let Err(e) = fs::remove_file(&self.path) {
            if self.path.exists() {
                error!("Failed to remove socket file: {}", e);
            }
        } else {
            info!("Removed socket file at {}", self.path.display());
        }
    }
}

/// Start the IPC socket server
/// Returns a receiver for incoming commands and a guard that cleans up the socket
pub async fn start_server() -> Result<(mpsc::UnboundedReceiver<IpcCommand>, SocketGuard)> {
    let socket_path = get_socket_path()?;

    // Remove stale socket if it exists
    if socket_path.exists() {
        info!("Removing stale socket at {}", socket_path.display());
        fs::remove_file(&socket_path)?;
    }

    let listener = UnixListener::bind(&socket_path)
        .with_context(|| format!("Failed to bind socket at {}", socket_path.display()))?;

    info!("IPC socket listening at {}", socket_path.display());

    let guard = SocketGuard {
        path: socket_path.clone(),
    };
    let (tx, rx) = mpsc::unbounded_channel();

    // Spawn task to accept connections
    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let tx_clone = tx.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_client(stream, tx_clone).await {
                            debug!("Client connection error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                }
            }
        }
    });

    Ok((rx, guard))
}

/// Handle a single client connection
async fn handle_client(
    stream: UnixStream,
    tx: mpsc::UnboundedSender<IpcCommand>,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    // Read one command per connection
    reader.read_line(&mut line).await?;

    let response = match line.parse::<IpcCommand>() {
        Ok(cmd) => {
            debug!("Received IPC command: {:?}", cmd);

            if tx.send(cmd).is_err() {
                IpcResponse::Error("Daemon is shutting down".to_string())
            } else {
                IpcResponse::Ok
            }
        }
        Err(_) => {
            warn!("Unknown IPC command: {}", line.trim());
            IpcResponse::Error(format!("Unknown command: {}", line.trim()))
        }
    };

    // Send response
    let response_json = serde_json::to_string(&response)?;
    writer.write_all(response_json.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;

    Ok(())
}
