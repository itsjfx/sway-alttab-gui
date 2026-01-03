use crate::config::Config;
use crate::keyboard_monitor::KeyEvent;
use crate::window_manager::WindowManager;
use anyhow::Result;
use futures_lite::stream::StreamExt;
use swayipc_async::{Connection, Event, EventType, WindowChange};
use tokio::sync::mpsc;
use tracing::{debug, error, info};

#[derive(Debug, Clone, PartialEq, Eq)]
enum DaemonState {
    Idle,
    Switching,
}

#[derive(Debug, Clone)]
enum WindowEvent {
    Focus(i64), // Window ID that received focus
}

pub struct Daemon {
    window_manager: WindowManager,
    config: Config,
    state: DaemonState,
    shift_pressed: bool,
    alt_pressed: bool,
    current_index: usize,
    current_windows: Vec<crate::window_manager::WindowInfo>,
}

impl Daemon {
    pub fn new(config: Config) -> Result<Self> {
        let window_manager = WindowManager::new()?;

        Ok(Daemon {
            window_manager,
            config,
            state: DaemonState::Idle,
            shift_pressed: false,
            alt_pressed: false,
            current_index: 0,
            current_windows: Vec::new(),
        })
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
                    self.handle_key_event(key_event)?;
                }
                Some(window_event) = window_rx.recv() => {
                    self.handle_window_event(window_event)?;
                }
                else => {
                    info!("All channels closed, shutting down");
                    break;
                }
            }
        }

        sway_events.abort();
        Ok(())
    }

    fn handle_key_event(
        &mut self,
        event: KeyEvent,
    ) -> Result<()> {
        debug!("Key event: {:?}, State: {:?}", event, self.state);

        match event {
            KeyEvent::AltPressed => {
                self.alt_pressed = true;
            }
            KeyEvent::AltReleased => {
                self.alt_pressed = false;

                // If we're in switching mode, select the current window
                if self.state == DaemonState::Switching {
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
                    match self.state {
                        DaemonState::Idle => {
                            // Start switching mode
                            self.start_switching()?;
                        }
                        DaemonState::Switching => {
                            // Cycle through windows
                            self.cycle_windows(!self.shift_pressed)?;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn handle_window_event(
        &mut self,
        event: WindowEvent,
    ) -> Result<()> {
        match event {
            WindowEvent::Focus(window_id) => {
                // Only update MRU order when not in switching mode
                // (during switching, we don't want Sway events to interfere)
                if self.state == DaemonState::Idle {
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
        self.current_windows = self.window_manager.get_filtered_windows(self.config.mode);

        if self.current_windows.is_empty() {
            info!("No windows to switch to");
            return Ok(());
        }

        // Enter switching state
        self.state = DaemonState::Switching;

        // Start at index 1 (next window), or 0 if only one window
        self.current_index = if self.current_windows.len() > 1 { 1 } else { 0 };

        // Print to stderr
        self.print_switcher();

        Ok(())
    }

    fn cycle_windows(&mut self, forward: bool) -> Result<()> {
        debug!("Cycling windows: forward={}", forward);

        if self.current_windows.is_empty() {
            return Ok(());
        }

        if forward {
            self.current_index = (self.current_index + 1) % self.current_windows.len();
        } else {
            if self.current_index == 0 {
                self.current_index = self.current_windows.len() - 1;
            } else {
                self.current_index -= 1;
            }
        }

        self.print_switcher();

        Ok(())
    }

    fn print_switcher(&self) {
        eprintln!("\n=== Window Switcher ===");
        for (i, window) in self.current_windows.iter().enumerate() {
            let marker = if i == self.current_index { ">>>" } else { "   " };
            let app_id = window.app_id.as_deref().unwrap_or("<unknown>");
            eprintln!("{} [{}] {} - {}", marker, window.id, app_id, window.title);
        }
        eprintln!("=======================\n");
    }

    fn finalize_selection(&mut self) -> Result<()> {
        info!("Finalizing window selection");

        // Focus the selected window
        if let Some(window) = self.current_windows.get(self.current_index) {
            eprintln!("SELECTING: {} (ID: {})", window.title, window.id);
            let window_id = window.id;
            self.window_manager.focus_window(window_id)?;

            // Update MRU order immediately (don't wait for Sway event)
            self.window_manager.on_focus(window_id);
        }

        // Return to idle state
        self.state = DaemonState::Idle;
        self.current_windows.clear();
        self.current_index = 0;

        Ok(())
    }

    /// Monitor Sway events for window changes
    async fn monitor_sway_events(window_tx: mpsc::UnboundedSender<WindowEvent>) -> Result<()> {
        let subs = [EventType::Window];
        let mut events = Connection::new().await?.subscribe(&subs).await?;

        info!("Subscribed to Sway window events");

        while let Some(event) = events.next().await {
            match event? {
                Event::Window(e) => {
                    debug!("Sway window event: {:?} for container {:?}", e.change, e.container.id);

                    // Track window focus changes for MRU ordering
                    if e.change == WindowChange::Focus {
                        if let Err(e) = window_tx.send(WindowEvent::Focus(e.container.id)) {
                            error!("Failed to send window focus event: {}", e);
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }
}
