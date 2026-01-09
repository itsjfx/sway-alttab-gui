use crate::config::Config;
use crate::icon_resolver::WmClassIndex;
use crate::ipc::InputCommand;
use crate::ui_commands::UiCommand;
use crate::window_manager::WindowManager;
use crate::window_switcher::WindowSwitcher;
use anyhow::Result;
use futures_lite::stream::StreamExt;
use swayipc_async::{Connection, Event, EventType, WindowChange};
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::mpsc;
use tracing::{debug, error, info};

/// Type alias for the optional UI command sender
type UiSender = Option<mpsc::UnboundedSender<UiCommand>>;

#[derive(Debug, Clone)]
enum WindowEvent {
    Focus(i64), // Window ID that received focus
}

/// Actions that can be taken by the daemon state machine.
/// This is a pure representation of what the daemon should do,
/// making state transitions testable without async/IO.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DaemonAction {
    /// Start the window switching UI
    StartSwitching,
    /// Cycle to the next window
    CycleForward,
    /// Cycle to the previous window
    CycleBackward,
    /// Finalize selection and focus the window
    FinalizeSelection,
    /// Cancel switching without selecting
    Cancel,
    /// Update MRU order for a window
    UpdateMru { window_id: i64 },
    /// No action needed
    None,
}

/// Determine what action to take based on input command and current state.
/// This is a pure function that encapsulates the state machine logic.
#[must_use]
pub fn determine_input_action(cmd: InputCommand, is_switching: bool) -> DaemonAction {
    match (cmd, is_switching) {
        (InputCommand::Next, true) => DaemonAction::CycleForward,
        (InputCommand::Prev, true) => DaemonAction::CycleBackward,
        (InputCommand::Select, true) => DaemonAction::FinalizeSelection,
        (InputCommand::Cancel, true) => DaemonAction::Cancel,
        // When not switching, input commands are ignored
        (_, false) => DaemonAction::None,
    }
}

/// Determine what action to take when show signal is received.
#[must_use]
pub fn determine_show_action(is_switching: bool) -> DaemonAction {
    if is_switching {
        // If already switching, show acts like next
        DaemonAction::CycleForward
    } else {
        DaemonAction::StartSwitching
    }
}

/// Determine what action to take for a window focus event.
#[must_use]
pub fn determine_focus_action(window_id: i64, is_switching: bool) -> DaemonAction {
    if is_switching {
        // During switching, ignore external focus events
        DaemonAction::None
    } else {
        DaemonAction::UpdateMru { window_id }
    }
}

pub struct Daemon {
    window_manager: WindowManager,
    config: Config,
    /// Active window switcher session, or None if idle
    switcher: Option<WindowSwitcher>,
    ui_tx: UiSender,
    wmclass_index: WmClassIndex,
}

impl Daemon {
    pub fn new(config: Config, ui_tx: UiSender, wmclass_index: WmClassIndex) -> Result<Self> {
        let window_manager = WindowManager::new()?;

        Ok(Daemon {
            window_manager,
            config,
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
        mut input_rx: mpsc::UnboundedReceiver<InputCommand>,
    ) -> Result<()> {
        info!("Starting daemon event loop");

        // Set up SIGUSR1 handler for show command
        let mut sigusr1 = signal(SignalKind::user_defined1())?;

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
                _ = sigusr1.recv() => {
                    debug!("Received SIGUSR1, triggering show");
                    self.handle_show()?;
                }
                Some(input_cmd) = input_rx.recv() => {
                    debug!("Received input command: {:?}", input_cmd);
                    self.handle_input_command(input_cmd)?;
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

        info!("Daemon shutting down gracefully");
        sway_events.abort();
        Ok(())
    }

    /// Handle SIGUSR1 show command
    fn handle_show(&mut self) -> Result<()> {
        match determine_show_action(self.is_switching()) {
            DaemonAction::StartSwitching => self.start_switching(),
            DaemonAction::CycleForward => self.cycle_windows(true),
            _ => Ok(()),
        }
    }

    /// Handle keyboard input commands from UI
    fn handle_input_command(&mut self, cmd: InputCommand) -> Result<()> {
        debug!("Input command: {:?}, switching: {}", cmd, self.is_switching());

        match determine_input_action(cmd, self.is_switching()) {
            DaemonAction::CycleForward => self.cycle_windows(true),
            DaemonAction::CycleBackward => self.cycle_windows(false),
            DaemonAction::FinalizeSelection => self.finalize_selection(),
            DaemonAction::Cancel => self.cancel_switching(),
            DaemonAction::None => Ok(()),
            _ => Ok(()),
        }
    }

    fn handle_window_event(&mut self, event: WindowEvent) -> Result<()> {
        match event {
            WindowEvent::Focus(window_id) => {
                if let DaemonAction::UpdateMru { window_id } =
                    determine_focus_action(window_id, self.is_switching())
                {
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
        if let Some(ref ui_tx) = self.ui_tx
            && let Some(ref switcher) = self.switcher {
                let command = UiCommand::UpdateSelection {
                    index: switcher.current_index(),
                };
                if let Err(e) = ui_tx.send(command) {
                    debug!("Failed to send selection update to UI (channel closed): {}", e);
                }
            }

        Ok(())
    }

    fn print_switcher_static(switcher: &WindowSwitcher) {
        debug!("=== Window Switcher ===");
        for (i, window) in switcher.windows().iter().enumerate() {
            let marker = if i == switcher.current_index() {
                ">>>"
            } else {
                "   "
            };
            let app_id = window.app_id.as_deref().unwrap_or("<unknown>");
            debug!("{} [{}] {} - {}", marker, window.id, app_id, window.title);
        }
        debug!("=======================");
    }

    fn finalize_selection(&mut self) -> Result<()> {
        info!("Finalizing window selection");

        // Take the switcher out, ending switching mode
        let Some(switcher) = self.switcher.take() else {
            return Ok(());
        };

        // Focus the selected window
        if let Some(window) = switcher.current() {
            debug!("Selecting window: {} (ID: {})", window.title, window.id);
            let window_id = window.id;
            self.window_manager.focus_window(window_id)?;

            // Update MRU order immediately (don't wait for Sway event)
            self.window_manager.on_focus(window_id);
        }

        // Hide UI if available
        if let Some(ref ui_tx) = self.ui_tx
            && let Err(e) = ui_tx.send(UiCommand::Hide) {
                debug!("Failed to send hide command to UI (channel closed): {}", e);
            }

        Ok(())
    }

    /// Cancel switching without selecting a window
    fn cancel_switching(&mut self) -> Result<()> {
        info!("Canceling window switching");

        self.switcher = None;

        // Hide UI if available
        if let Some(ref ui_tx) = self.ui_tx
            && let Err(e) = ui_tx.send(UiCommand::Hide) {
                debug!("Failed to send hide command to UI (channel closed): {}", e);
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
                if e.change == WindowChange::Focus
                    && let Err(e) = window_tx.send(WindowEvent::Focus(e.container.id)) {
                        error!("Failed to send window focus event: {}", e);
                    }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== determine_input_action tests ====================

    #[test]
    fn test_input_next_while_switching() {
        let action = determine_input_action(InputCommand::Next, true);
        assert_eq!(action, DaemonAction::CycleForward);
    }

    #[test]
    fn test_input_prev_while_switching() {
        let action = determine_input_action(InputCommand::Prev, true);
        assert_eq!(action, DaemonAction::CycleBackward);
    }

    #[test]
    fn test_input_select_while_switching() {
        let action = determine_input_action(InputCommand::Select, true);
        assert_eq!(action, DaemonAction::FinalizeSelection);
    }

    #[test]
    fn test_input_cancel_while_switching() {
        let action = determine_input_action(InputCommand::Cancel, true);
        assert_eq!(action, DaemonAction::Cancel);
    }

    #[test]
    fn test_input_next_while_not_switching() {
        let action = determine_input_action(InputCommand::Next, false);
        assert_eq!(action, DaemonAction::None);
    }

    #[test]
    fn test_input_prev_while_not_switching() {
        let action = determine_input_action(InputCommand::Prev, false);
        assert_eq!(action, DaemonAction::None);
    }

    #[test]
    fn test_input_select_while_not_switching() {
        let action = determine_input_action(InputCommand::Select, false);
        assert_eq!(action, DaemonAction::None);
    }

    #[test]
    fn test_input_cancel_while_not_switching() {
        let action = determine_input_action(InputCommand::Cancel, false);
        assert_eq!(action, DaemonAction::None);
    }

    // ==================== determine_show_action tests ====================

    #[test]
    fn test_show_while_not_switching_starts_switching() {
        let action = determine_show_action(false);
        assert_eq!(action, DaemonAction::StartSwitching);
    }

    #[test]
    fn test_show_while_switching_cycles_forward() {
        let action = determine_show_action(true);
        assert_eq!(action, DaemonAction::CycleForward);
    }

    // ==================== determine_focus_action tests ====================

    #[test]
    fn test_focus_while_not_switching_updates_mru() {
        let action = determine_focus_action(42, false);
        assert_eq!(action, DaemonAction::UpdateMru { window_id: 42 });
    }

    #[test]
    fn test_focus_while_switching_is_ignored() {
        let action = determine_focus_action(42, true);
        assert_eq!(action, DaemonAction::None);
    }

    #[test]
    fn test_focus_preserves_window_id() {
        let action = determine_focus_action(12345, false);
        assert_eq!(action, DaemonAction::UpdateMru { window_id: 12345 });
    }

    // ==================== DaemonAction enum tests ====================

    #[test]
    fn test_daemon_action_equality() {
        assert_eq!(DaemonAction::StartSwitching, DaemonAction::StartSwitching);
        assert_ne!(DaemonAction::StartSwitching, DaemonAction::CycleForward);
        assert_eq!(
            DaemonAction::UpdateMru { window_id: 1 },
            DaemonAction::UpdateMru { window_id: 1 }
        );
        assert_ne!(
            DaemonAction::UpdateMru { window_id: 1 },
            DaemonAction::UpdateMru { window_id: 2 }
        );
    }

    #[test]
    fn test_daemon_action_debug() {
        // Ensure Debug trait is properly derived
        let action = DaemonAction::CycleForward;
        let debug_str = format!("{:?}", action);
        assert!(debug_str.contains("CycleForward"));
    }

    #[test]
    fn test_daemon_action_clone() {
        let action = DaemonAction::UpdateMru { window_id: 99 };
        let cloned = action.clone();
        assert_eq!(action, cloned);
    }
}
