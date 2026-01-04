//! Window switching logic extracted from the Daemon.
//!
//! This module encapsulates the state and logic for cycling through windows
//! during an Alt+Tab switching session.

use crate::window_manager::WindowInfo;

/// Manages the window list and current selection during an Alt+Tab session.
///
/// This struct is created when switching mode begins and destroyed when
/// the user finalizes their selection.
pub struct WindowSwitcher {
    windows: Vec<WindowInfo>,
    current_index: usize,
}

impl WindowSwitcher {
    /// Create a new window switcher with the given window list.
    ///
    /// If `start_at_next` is true and there are multiple windows,
    /// the initial selection will be the second window (index 1).
    pub fn new(windows: Vec<WindowInfo>, start_at_next: bool) -> Self {
        let current_index = if start_at_next && windows.len() > 1 {
            1
        } else {
            0
        };

        WindowSwitcher {
            windows,
            current_index,
        }
    }

    /// Get the current window list.
    pub fn windows(&self) -> &[WindowInfo] {
        &self.windows
    }

    /// Get the current selection index.
    pub fn current_index(&self) -> usize {
        self.current_index
    }

    /// Get the currently selected window, if any.
    pub fn current(&self) -> Option<&WindowInfo> {
        self.windows.get(self.current_index)
    }

    /// Check if there are any windows to switch between.
    pub fn is_empty(&self) -> bool {
        self.windows.is_empty()
    }

    /// Cycle to the next or previous window.
    ///
    /// Returns the new current index.
    pub fn cycle(&mut self, forward: bool) -> usize {
        if self.windows.is_empty() {
            return 0;
        }

        let len = self.windows.len();
        self.current_index = if forward {
            (self.current_index + 1) % len
        } else if self.current_index == 0 {
            len - 1
        } else {
            self.current_index - 1
        };

        self.current_index
    }

    /// Finalize the selection and return the selected window.
    ///
    /// This consumes the switcher and returns the window that was selected.
    #[allow(dead_code)]
    pub fn finalize(self) -> Option<WindowInfo> {
        if self.current_index < self.windows.len() {
            Some(self.windows.into_iter().nth(self.current_index).unwrap())
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_window(id: i64, title: &str) -> WindowInfo {
        WindowInfo {
            id,
            app_id: Some(format!("app-{}", id)),
            title: title.to_string(),
            workspace: "1".to_string(),
            window_class: None,
        }
    }

    #[test]
    fn test_new_empty() {
        let switcher = WindowSwitcher::new(vec![], false);
        assert!(switcher.is_empty());
        assert_eq!(switcher.current_index(), 0);
        assert!(switcher.current().is_none());
    }

    #[test]
    fn test_new_starts_at_first() {
        let windows = vec![make_window(1, "A"), make_window(2, "B")];
        let switcher = WindowSwitcher::new(windows, false);
        assert_eq!(switcher.current_index(), 0);
        assert_eq!(switcher.current().unwrap().id, 1);
    }

    #[test]
    fn test_new_starts_at_next() {
        let windows = vec![make_window(1, "A"), make_window(2, "B")];
        let switcher = WindowSwitcher::new(windows, true);
        assert_eq!(switcher.current_index(), 1);
        assert_eq!(switcher.current().unwrap().id, 2);
    }

    #[test]
    fn test_new_single_window_ignores_start_at_next() {
        let windows = vec![make_window(1, "A")];
        let switcher = WindowSwitcher::new(windows, true);
        assert_eq!(switcher.current_index(), 0);
    }

    #[test]
    fn test_cycle_forward() {
        let windows = vec![make_window(1, "A"), make_window(2, "B"), make_window(3, "C")];
        let mut switcher = WindowSwitcher::new(windows, false);

        assert_eq!(switcher.current_index(), 0);
        switcher.cycle(true);
        assert_eq!(switcher.current_index(), 1);
        switcher.cycle(true);
        assert_eq!(switcher.current_index(), 2);
        switcher.cycle(true); // Wrap around
        assert_eq!(switcher.current_index(), 0);
    }

    #[test]
    fn test_cycle_backward() {
        let windows = vec![make_window(1, "A"), make_window(2, "B"), make_window(3, "C")];
        let mut switcher = WindowSwitcher::new(windows, false);

        assert_eq!(switcher.current_index(), 0);
        switcher.cycle(false); // Wrap to end
        assert_eq!(switcher.current_index(), 2);
        switcher.cycle(false);
        assert_eq!(switcher.current_index(), 1);
        switcher.cycle(false);
        assert_eq!(switcher.current_index(), 0);
    }

    #[test]
    fn test_cycle_empty() {
        let mut switcher = WindowSwitcher::new(vec![], false);
        assert_eq!(switcher.cycle(true), 0);
        assert_eq!(switcher.cycle(false), 0);
    }

    #[test]
    fn test_finalize_returns_selected() {
        let windows = vec![make_window(1, "A"), make_window(2, "B"), make_window(3, "C")];
        let mut switcher = WindowSwitcher::new(windows, false);
        switcher.cycle(true);
        switcher.cycle(true);

        let selected = switcher.finalize();
        assert!(selected.is_some());
        assert_eq!(selected.unwrap().id, 3);
    }

    #[test]
    fn test_finalize_empty() {
        let switcher = WindowSwitcher::new(vec![], false);
        assert!(switcher.finalize().is_none());
    }
}
