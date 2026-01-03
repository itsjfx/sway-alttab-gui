mod config;
mod daemon;
mod icon_resolver;
mod keyboard_monitor;
mod ui;
mod ui_commands;
mod ui_handler;
mod window_manager;

use anyhow::{Context, Result};
use config::Config;
use daemon::Daemon;
use gtk4::prelude::*;
use keyboard_monitor::KeyboardMonitor;
use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use tokio::sync::mpsc;
use tracing::{error, info};
use tracing_subscriber;
use ui::SwitcherWindow;

/// Get the path to the pidfile
fn get_pidfile_path() -> Result<PathBuf> {
    // Try to use XDG_RUNTIME_DIR, fall back to ~/.cache
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|home| PathBuf::from(home).join(".cache"))
        })
        .context("Could not determine runtime directory")?;

    Ok(runtime_dir.join("sway-alttab.pid"))
}

/// Check if another instance is already running
fn check_pidfile() -> Result<()> {
    let pidfile = get_pidfile_path()?;

    if pidfile.exists() {
        // Read the PID from the file
        let pid_str = fs::read_to_string(&pidfile)
            .context("Failed to read pidfile")?;
        let pid: u32 = pid_str.trim()
            .parse()
            .context("Invalid PID in pidfile")?;

        // Check if the process is still running
        if process_exists(pid) {
            anyhow::bail!(
                "Another instance of sway-alttab is already running (PID: {}). \
                 If this is incorrect, remove the pidfile at: {}",
                pid,
                pidfile.display()
            );
        } else {
            // Stale pidfile, remove it
            info!("Removing stale pidfile (PID {} not found)", pid);
            fs::remove_file(&pidfile).ok();
        }
    }

    Ok(())
}

/// Check if a process with the given PID exists
fn process_exists(pid: u32) -> bool {
    // Check if /proc/<pid> exists (Linux-specific, but this is for Sway which is Linux-only)
    PathBuf::from(format!("/proc/{}", pid)).exists()
}

/// Create the pidfile
fn create_pidfile() -> Result<PidfileGuard> {
    let pidfile = get_pidfile_path()?;
    let pid = std::process::id();

    fs::write(&pidfile, pid.to_string())
        .context("Failed to write pidfile")?;

    info!("Created pidfile at {} with PID {}", pidfile.display(), pid);

    Ok(PidfileGuard { path: pidfile })
}

/// Guard that removes the pidfile when dropped
struct PidfileGuard {
    path: PathBuf,
}

impl Drop for PidfileGuard {
    fn drop(&mut self) {
        if let Err(e) = fs::remove_file(&self.path) {
            error!("Failed to remove pidfile: {}", e);
        } else {
            info!("Removed pidfile at {}", self.path.display());
        }
    }
}

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

    // Ignore SIGUSR1 signal to prevent crashes
    #[cfg(unix)]
    unsafe {
        use libc::{signal, SIGUSR1, SIG_IGN};
        signal(SIGUSR1, SIG_IGN);
    }

    info!("Starting sway-alttab daemon with GTK UI");
    info!("Workspace mode: {:?}", config.mode);

    // Check if another instance is already running
    check_pidfile()?;

    // Create pidfile (will be automatically removed when the guard is dropped)
    let _pidfile_guard = create_pidfile()?;

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
