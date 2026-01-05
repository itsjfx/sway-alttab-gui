mod config;
mod daemon;
mod icon_resolver;
mod ipc;
mod socket_client;
mod socket_server;
mod sway_client;
mod ui;
mod ui_commands;
mod ui_handler;
mod window_manager;
mod window_switcher;

use anyhow::{Context, Result};
use config::{Command, Config};
use daemon::Daemon;
use gtk4::prelude::*;
use icon_resolver::{IconResolver, WmClassIndex};
use ipc::IpcCommand;
use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use tokio::sync::mpsc;
use tracing::{error, info};
use ui::SwitcherWindow;

/// Get the path to the pidfile
fn get_pidfile_path() -> Result<PathBuf> {
    // Try to use XDG_RUNTIME_DIR, fall back to ~/.cache
    let runtime_dir = dirs::runtime_dir()
        .or_else(dirs::cache_dir)
        .context("Could not determine runtime directory")?;

    Ok(runtime_dir.join("sway-alttab.pid"))
}

/// Check if another instance is already running
fn check_pidfile() -> Result<()> {
    let pidfile = get_pidfile_path()?;

    if pidfile.exists() {
        // Read the PID from the file
        let pid_str = fs::read_to_string(&pidfile).context("Failed to read pidfile")?;
        let pid: u32 = pid_str.trim().parse().context("Invalid PID in pidfile")?;

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
            if let Err(e) = fs::remove_file(&pidfile) {
                tracing::warn!("Failed to remove stale pidfile: {}", e);
            }
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

    fs::write(&pidfile, pid.to_string()).context("Failed to write pidfile")?;

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

    // Dispatch based on command
    match config.command() {
        Command::Daemon => run_daemon_mode(config),
        Command::Show => socket_client::send_command_and_exit(IpcCommand::Show),
        Command::Next => socket_client::send_command_and_exit(IpcCommand::Next),
        Command::Prev => socket_client::send_command_and_exit(IpcCommand::Prev),
        Command::Select => socket_client::send_command_and_exit(IpcCommand::Select),
        Command::Cancel => socket_client::send_command_and_exit(IpcCommand::Cancel),
        Command::Status => socket_client::send_command_and_exit(IpcCommand::Status),
        Command::Shutdown => socket_client::send_command_and_exit(IpcCommand::Shutdown),
    }
}

fn run_daemon_mode(config: Config) -> Result<()> {
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

    // Build WMClass index at startup (before GTK, so it's ready when needed)
    info!("Building WMClass index for icon resolution...");
    let wmclass_index = IconResolver::build_wmclass_index();

    // Initialize GTK
    gtk4::init()?;

    // Pre-warm GTK IconTheme cache to avoid slow first alt-tab
    // The first lookup_icon() call triggers GTK to parse and index all icon theme directories
    {
        let theme = gtk4::IconTheme::new();
        let _ = theme.lookup_icon(
            "application-x-executable",
            &[],
            64,
            1,
            gtk4::TextDirection::None,
            gtk4::IconLookupFlags::empty(),
        );
    }

    // Create GTK Application
    let app = gtk4::Application::builder()
        .application_id("com.github.itsjfx.sway-alttab-gui")
        .build();

    let wmclass_index_clone = wmclass_index.clone();
    app.connect_activate(move |app| {
        // Setup CSS
        ui::setup_css();

        // Create SwitcherWindow
        let switcher = Rc::new(RefCell::new(SwitcherWindow::new(app)));

        // Pre-realize window to avoid slow first show
        switcher.borrow().warm_up();

        // Create channels for UI communication
        let (ui_cmd_tx, ui_cmd_rx) = mpsc::unbounded_channel();

        // Setup UI command handler
        ui_handler::handle_ui_commands(switcher.clone(), ui_cmd_rx);

        // Spawn Tokio runtime in a background thread
        let config_clone = config.clone();
        let wmclass_index_for_daemon = wmclass_index_clone.clone();
        std::thread::spawn(move || {
            // Create Tokio runtime
            let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");

            // Run daemon in Tokio runtime
            rt.block_on(async move {
                match run_daemon_async(config_clone, ui_cmd_tx, wmclass_index_for_daemon).await {
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

    // Run GTK application (pass empty args since we already parsed with clap)
    app.run_with_args::<&str>(&[]);

    Ok(())
}

/// Run the async daemon logic with socket IPC
async fn run_daemon_async(
    config: Config,
    ui_cmd_tx: mpsc::UnboundedSender<ui_commands::UiCommand>,
    wmclass_index: WmClassIndex,
) -> Result<()> {
    // Start IPC socket server
    let (ipc_rx, _socket_guard) = socket_server::start_server().await?;

    // Create and run daemon
    let daemon = Daemon::new(config, Some(ui_cmd_tx), wmclass_index)?;
    info!("Starting daemon event loop");
    daemon.run(ipc_rx).await?;

    Ok(())
}
