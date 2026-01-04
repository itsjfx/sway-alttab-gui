use crate::icon_resolver::WmClassIndex;
use crate::window_manager::WindowInfo;

/// Commands sent from daemon to UI
#[derive(Debug, Clone)]
pub enum UiCommand {
    /// Show the window switcher with a list of windows
    Show {
        windows: Vec<WindowInfo>,
        initial_index: usize,
        wmclass_index: WmClassIndex,
    },
    /// Cycle to next window
    CycleNext,
    /// Cycle to previous window
    CyclePrev,
    /// Hide the window switcher
    Hide,
}
