mod config;
mod daemon;
mod icon_resolver;
mod keyboard_monitor;
mod ui;
mod ui_commands;
mod ui_handler;
mod window_manager;

use anyhow::Result;
use config::Config;
use daemon::Daemon;
use gtk4::prelude::*;
use keyboard_monitor::KeyboardMonitor;
use std::cell::RefCell;
use std::rc::Rc;
use tokio::sync::mpsc;
use tracing::{error, info};
use tracing_subscriber;
use ui::SwitcherWindow;

fn main() -> Result<()> {
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

    info!("Starting sway-alttab daemon with GTK UI");
    info!("Workspace mode: {:?}", config.mode);

    // Check keyboard device permissions
    keyboard_monitor::check_permissions()?;

    // Initialize GTK
    gtk4::init()?;

    // Create GTK Application
    let app = gtk4::Application::builder()
        .application_id("com.github.sway-alttab")
        .build();

    app.connect_activate(move |app| {
        // Setup CSS
        ui::setup_css();

        // Create SwitcherWindow
        let switcher = Rc::new(RefCell::new(SwitcherWindow::new(app)));

        // Create channels for UI communication
        let (ui_cmd_tx, ui_cmd_rx) = mpsc::unbounded_channel();
        let (ui_resp_tx, _ui_resp_rx) = mpsc::unbounded_channel();

        // Setup UI command handler
        ui_handler::handle_ui_commands(switcher.clone(), ui_cmd_rx, ui_resp_tx);

        // Spawn Tokio runtime in a background thread
        let config_clone = config.clone();
        std::thread::spawn(move || {
            // Create Tokio runtime
            let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");

            // Run daemon in Tokio runtime
            rt.block_on(async move {
                match run_daemon(config_clone, ui_cmd_tx).await {
                    Ok(_) => {
                        info!("Daemon exited normally");
                    }
                    Err(e) => {
                        error!("Daemon error: {}", e);
                    }
                }
            });

            error!("Daemon thread exiting");
        });
    });

    // Run GTK application
    app.run();

    Ok(())
}

/// Run the async daemon logic within the GLib event loop
async fn run_daemon(config: Config, ui_cmd_tx: mpsc::UnboundedSender<ui_commands::UiCommand>) -> Result<()> {
    // Create communication channels
    let (key_tx, key_rx) = mpsc::unbounded_channel();

    // Create and start keyboard monitor
    let keyboard_monitor = KeyboardMonitor::new()?;

    // Spawn keyboard monitoring in a dedicated blocking thread
    std::thread::spawn(move || {
        if let Err(e) = keyboard_monitor.monitor_blocking(key_tx) {
            error!("Keyboard monitor error: {}", e);
        }
    });

    // Create and run daemon
    let daemon = Daemon::new(config, Some(ui_cmd_tx))?;
    info!("Starting daemon event loop");
    daemon.run(key_rx).await?;

    error!("Daemon run() returned - this should never happen!");
    Ok(())
}
