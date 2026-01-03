use crate::window_manager::WindowInfo;

/// Commands sent from daemon to UI
#[derive(Debug, Clone)]
pub enum UiCommand {
    /// Show the window switcher with a list of windows
    Show {
        windows: Vec<WindowInfo>,
        initial_index: usize,
    },
    /// Cycle to next window
    CycleNext,
    /// Cycle to previous window
    CyclePrev,
    /// Hide the window switcher
    Hide,
}

/// Responses sent from UI back to daemon
#[derive(Debug, Clone)]
pub enum UiResponse {
    /// UI was shown successfully
    Shown,
    /// UI cycled to a new window
    Cycled { new_index: usize },
    /// UI was dismissed (window closed or hidden)
    Dismissed {
        selected_window: Option<WindowInfo>,
    },
}
