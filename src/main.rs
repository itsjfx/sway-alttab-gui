mod config;
mod daemon;
mod keyboard_monitor;
mod window_manager;

use anyhow::Result;
use config::Config;
use daemon::Daemon;
use keyboard_monitor::KeyboardMonitor;
use tokio::sync::mpsc;
use tracing::{error, info};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Parse CLI arguments
    let config = Config::parse();

    // Initialize logging
    let log_level = if config.verbose {
        tracing::Level::DEBUG
    } else {
        tracing::Level::INFO
    };

    tracing_subscriber::fmt()
        .with_max_level(log_level)
        .with_target(false)
        .init();

    info!("Starting sway-alttab daemon");
    info!("Workspace mode: {:?}", config.mode);

    // Check keyboard device permissions
    keyboard_monitor::check_permissions()?;

    // Create communication channels
    let (key_tx, key_rx) = mpsc::unbounded_channel();

    // Create and start keyboard monitor
    let keyboard_monitor = KeyboardMonitor::new()?;

    // Spawn keyboard monitoring task
    let monitor_handle = tokio::spawn(async move {
        if let Err(e) = keyboard_monitor.monitor(key_tx).await {
            error!("Keyboard monitor error: {}", e);
        }
    });

    // Create and run daemon
    let daemon = Daemon::new(config)?;
    daemon.run(key_rx).await?;

    // Wait for monitor to finish
    monitor_handle.await?;

    Ok(())
}
