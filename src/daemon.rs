use crate::config::Config;
use crate::icon_resolver::WmClassIndex;
use crate::keyboard_monitor::KeyEvent;
use crate::ui_commands::UiCommand;
use crate::window_manager::WindowManager;
use crate::window_switcher::WindowSwitcher;
use anyhow::Result;
use futures_lite::stream::StreamExt;
use swayipc_async::{Connection, Event, EventType, WindowChange};
use tokio::sync::mpsc;
use tracing::{debug, error, info};

#[derive(Debug, Clone)]
enum WindowEvent {
    Focus(i64), // Window ID that received focus
}

pub struct Daemon {
    window_manager: WindowManager,
    config: Config,
    shift_pressed: bool,
    alt_pressed: bool,
    /// Active window switcher session, or None if idle
    switcher: Option<WindowSwitcher>,
    ui_tx: Option<mpsc::UnboundedSender<UiCommand>>,
    wmclass_index: WmClassIndex,
}

impl Daemon {
    pub fn new(
        config: Config,
        ui_tx: Option<mpsc::UnboundedSender<UiCommand>>,
        wmclass_index: WmClassIndex,
    ) -> Result<Self> {
        let window_manager = WindowManager::new()?;

        Ok(Daemon {
            window_manager,
            config,
            shift_pressed: false,
            alt_pressed: false,
            switcher: None,
            ui_tx,
            wmclass_index,
        })
    }

    /// Returns true if currently in switching mode
    fn is_switching(&self) -> bool {
        self.switcher.is_some()
    }

    /// Main event loop
    pub async fn run(
        mut self,
        mut key_rx: mpsc::UnboundedReceiver<KeyEvent>,
    ) -> Result<()> {
        info!("Starting daemon event loop");

        // Create channel for window events
        let (window_tx, mut window_rx) = mpsc::unbounded_channel();

        // Create Sway IPC connection for event monitoring
        let sway_events = tokio::spawn(async move {
            match Self::monitor_sway_events(window_tx).await {
                Ok(_) => {}
                Err(e) => error!("Sway event monitoring error: {}", e),
            }
        });

        // Main event loop
        loop {
            tokio::select! {
                Some(key_event) = key_rx.recv() => {
                    debug!("Received key event: {:?}", key_event);
                    self.handle_key_event(key_event)?;
                }
                Some(window_event) = window_rx.recv() => {
                    debug!("Received window event: {:?}", window_event);
                    self.handle_window_event(window_event)?;
                }
                else => {
                    error!("All channels closed, shutting down");
                    break;
                }
            }
        }

        error!("Daemon event loop exited");
        sway_events.abort();
        Ok(())
    }

    fn handle_key_event(&mut self, event: KeyEvent) -> Result<()> {
        debug!(
            "Key event: {:?}, switching: {}",
            event,
            self.is_switching()
        );

        match event {
            KeyEvent::AltPressed => {
                self.alt_pressed = true;
            }
            KeyEvent::AltReleased => {
                self.alt_pressed = false;
                // If we're in switching mode, select the current window
                if self.is_switching() {
                    self.finalize_selection()?;
                }
            }
            KeyEvent::ShiftPressed => {
                self.shift_pressed = true;
            }
            KeyEvent::ShiftReleased => {
                self.shift_pressed = false;
            }
            KeyEvent::TabPressed => {
                if self.alt_pressed {
                    if self.is_switching() {
                        // Cycle through windows
                        self.cycle_windows(!self.shift_pressed)?;
                    } else {
                        // Start switching mode
                        self.start_switching()?;
                    }
                }
            }
        }

        Ok(())
    }

    fn handle_window_event(&mut self, event: WindowEvent) -> Result<()> {
        match event {
            WindowEvent::Focus(window_id) => {
                // Only update MRU order when not in switching mode
                // (during switching, we don't want Sway events to interfere)
                if !self.is_switching() {
                    debug!("Window {} focused, updating MRU order", window_id);
                    self.window_manager.on_focus(window_id);
                }
            }
        }
        Ok(())
    }

    fn start_switching(&mut self) -> Result<()> {
        info!("Starting window switching mode");

        // Refresh window list
        self.window_manager.refresh()?;

        // Get filtered windows
        let windows = self.window_manager.get_filtered_windows(self.config.mode);

        if windows.is_empty() {
            info!("No windows to switch to");
            return Ok(());
        }

        // Create the switcher, starting at the next window
        let switcher = WindowSwitcher::new(windows, true);

        // Print to stderr (keep console output)
        Self::print_switcher_static(&switcher);

        // Show UI if available
        if let Some(ref ui_tx) = self.ui_tx {
            info!("Sending UiCommand::Show to UI");
            if let Err(e) = ui_tx.send(UiCommand::Show {
                windows: switcher.windows().to_vec(),
                initial_index: switcher.current_index(),
                wmclass_index: self.wmclass_index.clone(),
            }) {
                error!("Failed to send UI command: {:?}", e);
            } else {
                info!("UI command sent successfully");
            }
        } else {
            info!("No UI channel available");
        }

        // Enter switching state
        self.switcher = Some(switcher);

        Ok(())
    }

    fn cycle_windows(&mut self, forward: bool) -> Result<()> {
        debug!("Cycling windows: forward={}", forward);

        if let Some(ref mut switcher) = self.switcher {
            if switcher.is_empty() {
                return Ok(());
            }
            switcher.cycle(forward);
        } else {
            return Ok(());
        }

        // Print to stderr (keep console output)
        if let Some(ref switcher) = self.switcher {
            Self::print_switcher_static(switcher);
        }

        // Update UI if available
        if let Some(ref ui_tx) = self.ui_tx {
            let command = if forward {
                UiCommand::CycleNext
            } else {
                UiCommand::CyclePrev
            };
            if let Err(e) = ui_tx.send(command) {
                debug!("Failed to send cycle command to UI (channel closed): {}", e);
            }
        }

        Ok(())
    }

    fn print_switcher_static(switcher: &WindowSwitcher) {
        eprintln!("\n=== Window Switcher ===");
        for (i, window) in switcher.windows().iter().enumerate() {
            let marker = if i == switcher.current_index() {
                ">>>"
            } else {
                "   "
            };
            let app_id = window.app_id.as_deref().unwrap_or("<unknown>");
            eprintln!("{} [{}] {} - {}", marker, window.id, app_id, window.title);
        }
        eprintln!("=======================\n");
    }

    fn finalize_selection(&mut self) -> Result<()> {
        info!("Finalizing window selection");

        // Take the switcher out, ending switching mode
        let switcher = match self.switcher.take() {
            Some(s) => s,
            None => return Ok(()),
        };

        // Focus the selected window
        if let Some(window) = switcher.current() {
            eprintln!("SELECTING: {} (ID: {})", window.title, window.id);
            let window_id = window.id;
            self.window_manager.focus_window(window_id)?;

            // Update MRU order immediately (don't wait for Sway event)
            self.window_manager.on_focus(window_id);
        }

        // Hide UI if available
        if let Some(ref ui_tx) = self.ui_tx {
            if let Err(e) = ui_tx.send(UiCommand::Hide) {
                debug!("Failed to send hide command to UI (channel closed): {}", e);
            }
        }

        Ok(())
    }

    /// Monitor Sway events for window changes
    async fn monitor_sway_events(window_tx: mpsc::UnboundedSender<WindowEvent>) -> Result<()> {
        let subs = [EventType::Window];
        let mut events = Connection::new().await?.subscribe(&subs).await?;

        info!("Subscribed to Sway window events");

        while let Some(event) = events.next().await {
            if let Event::Window(e) = event? {
                debug!(
                    "Sway window event: {:?} for container {:?}",
                    e.change, e.container.id
                );

                // Track window focus changes for MRU ordering
                if e.change == WindowChange::Focus {
                    if let Err(e) = window_tx.send(WindowEvent::Focus(e.container.id)) {
                        error!("Failed to send window focus event: {}", e);
                    }
                }
            }
        }

        Ok(())
    }
}
