use anyhow::Result;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use swayipc::{Node, NodeType};
use tracing::debug;

use crate::config::WorkspaceMode;
use crate::sway_client::{RealSwayClient, SwayClient};

#[derive(Debug, Clone)]
pub struct WindowInfo {
    pub id: i64,
    pub app_id: Option<String>,
    pub title: String,
    pub workspace: String,
    pub window_class: Option<String>, // WM_CLASS for X11 windows
}

impl WindowInfo {
    pub fn from_node(node: &Node, workspace: String) -> Option<Self> {
        // Only include actual windows (views), not containers
        // Windows have a pid, containers don't
        if node.node_type == NodeType::Con && node.pid.is_some() {
            // Extract WM_CLASS from X11/XWayland window properties
            let window_class = node
                .window_properties
                .as_ref()
                .and_then(|props| props.class.clone());

            Some(WindowInfo {
                id: node.id,
                app_id: node.app_id.clone(),
                title: node.name.clone().unwrap_or_default(),
                workspace,
                window_class,
            })
        } else {
            None
        }
    }
}

/// Manages window list and MRU ordering using Sway IPC.
pub struct WindowManager<C: SwayClient = RealSwayClient> {
    client: C,
    windows: Vec<WindowInfo>,
    current_workspace: Option<String>,
}

impl WindowManager<RealSwayClient> {
    /// Create a new WindowManager with a real Sway connection
    pub fn new() -> Result<Self> {
        let client = RealSwayClient::new()?;
        Self::with_client(client)
    }
}

impl<C: SwayClient> WindowManager<C> {
    /// Create a WindowManager with a custom SwayClient (for testing)
    pub fn with_client(client: C) -> Result<Self> {
        let mut manager = WindowManager {
            client,
            windows: Vec::new(),
            current_workspace: None,
        };
        manager.refresh()?;
        Ok(manager)
    }

    /// Refresh the window list from Sway
    /// This preserves the MRU order for existing windows
    pub fn refresh(&mut self) -> Result<()> {
        let tree = self.client.get_tree()?;

        // Find the currently focused window ID
        let focused_id = find_focused_window(&tree);

        // Save the current MRU order and collect new windows
        let old_windows = std::mem::take(&mut self.windows);
        let current_windows = collect_windows(&tree, Cow::Borrowed(""));

        // Preserve MRU order while merging old and new window lists
        self.windows = preserve_mru_order(old_windows, current_windows, focused_id);
        debug!(
            "Refreshed to {} windows (preserved MRU order, focused: {:?})",
            self.windows.len(),
            focused_id
        );

        // Get current workspace
        if let Ok(workspaces) = self.client.get_workspaces() {
            self.current_workspace = workspaces.iter().find(|w| w.focused).map(|w| w.name.clone());
        }

        Ok(())
    }

    /// Move window to front of MRU list
    pub fn on_focus(&mut self, window_id: i64) {
        if let Some(pos) = self.windows.iter().position(|w| w.id == window_id) {
            let window = self.windows.remove(pos);
            self.windows.insert(0, window);
        }
    }

    /// Get filtered windows based on workspace mode
    pub fn get_filtered_windows(&self, mode: WorkspaceMode) -> Vec<WindowInfo> {
        match mode {
            WorkspaceMode::Current => {
                if let Some(ref current_ws) = self.current_workspace {
                    self.windows
                        .iter()
                        .filter(|w| &w.workspace == current_ws)
                        .cloned()
                        .collect()
                } else {
                    self.windows.clone()
                }
            }
            WorkspaceMode::All => self.windows.clone(),
        }
    }

    /// Focus a window by ID
    pub fn focus_window(&mut self, window_id: i64) -> Result<()> {
        self.client.focus_window(window_id)
    }
}

/// Preserve MRU order while merging old and new window lists.
///
/// The resulting list has:
/// 1. The focused window first (if any)
/// 2. Previously known windows in their MRU order (if still present), with fresh data
/// 3. Newly discovered windows at the end
#[must_use]
fn preserve_mru_order(
    old_windows: Vec<WindowInfo>,
    current_windows: Vec<WindowInfo>,
    focused_id: Option<i64>,
) -> Vec<WindowInfo> {
    // Build a map of current windows by ID for O(1) lookup with fresh data
    let current_by_id: HashMap<i64, WindowInfo> =
        current_windows.into_iter().map(|w| (w.id, w)).collect();
    let mut result = Vec::with_capacity(current_by_id.len());
    let mut added_ids = HashSet::new();

    // 1. Add focused window first if it exists in current windows
    if let Some(fid) = focused_id
        && let Some(focused_win) = current_by_id.get(&fid).cloned() {
            added_ids.insert(fid);
            result.push(focused_win);
        }

    // 2. Add windows from old list that still exist (preserving MRU order)
    //    Use fresh data from current_by_id (single lookup pattern)
    for old_win in old_windows {
        if !added_ids.contains(&old_win.id)
            && let Some(fresh_win) = current_by_id.get(&old_win.id).cloned() {
                added_ids.insert(old_win.id);
                result.push(fresh_win);
            }
    }

    // 3. Add any new windows not in the old list
    for (id, new_win) in current_by_id {
        if !added_ids.contains(&id) {
            result.push(new_win);
        }
    }

    result
}

/// Recursively collect all windows from a Sway node tree.
/// Returns a flat list of WindowInfo structs.
///
/// Uses `Cow<str>` to avoid cloning workspace names during traversal.
/// The string is only cloned when a window is actually found.
#[must_use]
fn collect_windows<'a>(node: &'a Node, current_workspace: Cow<'a, str>) -> Vec<WindowInfo> {
    let mut windows = Vec::new();

    // Update workspace name if we encounter a workspace node
    // Use Cow to avoid cloning unless necessary
    let workspace: Cow<'a, str> = if node.node_type == NodeType::Workspace {
        node.name
            .as_deref()
            .map(Cow::Borrowed)
            .unwrap_or(current_workspace)
    } else {
        current_workspace
    };

    // Add window if it's an actual window (has a pid)
    // Only clone the workspace string when we actually create a WindowInfo
    if let Some(window) = WindowInfo::from_node(node, workspace.clone().into_owned()) {
        windows.push(window);
    }

    // Recurse into children - borrow the workspace string
    for child in &node.nodes {
        windows.extend(collect_windows(child, Cow::Borrowed(&workspace)));
    }
    for child in &node.floating_nodes {
        windows.extend(collect_windows(child, Cow::Borrowed(&workspace)));
    }

    windows
}

/// Find the currently focused window in a Sway node tree.
#[must_use]
fn find_focused_window(node: &Node) -> Option<i64> {
    // Check if this node is a focused window (not just a focused container)
    // Windows have a pid, containers don't
    if node.node_type == NodeType::Con && node.focused && node.pid.is_some() {
        return Some(node.id);
    }

    // Recurse into children
    for child in &node.nodes {
        if let Some(id) = find_focused_window(child) {
            return Some(id);
        }
    }
    for child in &node.floating_nodes {
        if let Some(id) = find_focused_window(child) {
            return Some(id);
        }
    }

    None
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

    fn make_window_in_workspace(id: i64, title: &str, workspace: &str) -> WindowInfo {
        WindowInfo {
            id,
            app_id: Some(format!("app-{}", id)),
            title: title.to_string(),
            workspace: workspace.to_string(),
            window_class: None,
        }
    }

    // ==================== preserve_mru_order tests ====================

    #[test]
    fn test_preserve_mru_order_empty_lists() {
        let result = preserve_mru_order(vec![], vec![], None);
        assert!(result.is_empty());
    }

    #[test]
    fn test_preserve_mru_order_focused_window_first() {
        let old = vec![make_window(1, "A"), make_window(2, "B")];
        let current = vec![make_window(1, "A"), make_window(2, "B"), make_window(3, "C")];

        let result = preserve_mru_order(old, current, Some(3));

        assert_eq!(result.len(), 3);
        assert_eq!(result[0].id, 3); // Focused window first
        assert_eq!(result[1].id, 1); // Then MRU order
        assert_eq!(result[2].id, 2);
    }

    #[test]
    fn test_preserve_mru_order_removes_closed_windows() {
        let old = vec![make_window(1, "A"), make_window(2, "B"), make_window(3, "C")];
        let current = vec![make_window(1, "A"), make_window(3, "C")]; // Window 2 closed

        let result = preserve_mru_order(old, current, None);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].id, 1);
        assert_eq!(result[1].id, 3);
    }

    #[test]
    fn test_preserve_mru_order_new_windows_at_end() {
        let old = vec![make_window(1, "A")];
        let current = vec![make_window(1, "A"), make_window(2, "B"), make_window(3, "C")];

        let result = preserve_mru_order(old, current, None);

        assert_eq!(result.len(), 3);
        assert_eq!(result[0].id, 1); // Existing window keeps position
        // New windows at end (order may vary)
        let new_ids: HashSet<_> = result[1..].iter().map(|w| w.id).collect();
        assert!(new_ids.contains(&2));
        assert!(new_ids.contains(&3));
    }

    #[test]
    fn test_preserve_mru_order_focused_already_first() {
        // When focused window is already first in old list
        let old = vec![make_window(1, "A"), make_window(2, "B")];
        let current = vec![make_window(1, "A"), make_window(2, "B")];

        let result = preserve_mru_order(old, current, Some(1));

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].id, 1); // Focused window first
        assert_eq!(result[1].id, 2);
    }

    #[test]
    fn test_preserve_mru_order_focused_window_not_in_current() {
        // Focused window was closed - shouldn't appear in result
        let old = vec![make_window(1, "A"), make_window(2, "B")];
        let current = vec![make_window(1, "A")]; // Window 2 is gone

        let result = preserve_mru_order(old, current, Some(2)); // 2 is focused but gone

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, 1);
    }

    #[test]
    fn test_preserve_mru_order_uses_fresh_data() {
        // Ensure titles are updated from current windows
        let old = vec![WindowInfo {
            id: 1,
            app_id: Some("app".to_string()),
            title: "Old Title".to_string(),
            workspace: "1".to_string(),
            window_class: None,
        }];
        let current = vec![WindowInfo {
            id: 1,
            app_id: Some("app".to_string()),
            title: "New Title".to_string(),
            workspace: "1".to_string(),
            window_class: None,
        }];

        let result = preserve_mru_order(old, current, None);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].title, "New Title"); // Should use fresh data
    }

    // ==================== WindowInfo tests ====================
    // Note: swayipc::Node is #[non_exhaustive] so we cannot construct it directly in tests.
    // WindowInfo::from_node is tested via integration tests with a real Sway connection.
    // The logic is simple: check node_type == Con && pid.is_some()

    #[test]
    fn test_window_info_fields() {
        let info = WindowInfo {
            id: 42,
            app_id: Some("alacritty".to_string()),
            title: "Terminal".to_string(),
            workspace: "1".to_string(),
            window_class: Some("Alacritty".to_string()),
        };

        assert_eq!(info.id, 42);
        assert_eq!(info.app_id, Some("alacritty".to_string()));
        assert_eq!(info.title, "Terminal");
        assert_eq!(info.workspace, "1");
        assert_eq!(info.window_class, Some("Alacritty".to_string()));
    }

    #[test]
    fn test_window_info_optional_fields() {
        let info = WindowInfo {
            id: 1,
            app_id: None,
            title: String::new(),
            workspace: "2".to_string(),
            window_class: None,
        };

        assert!(info.app_id.is_none());
        assert!(info.window_class.is_none());
        assert!(info.title.is_empty());
    }

    // ==================== get_filtered_windows tests ====================
    // Note: Full WindowManager tests would require mocking SwayClient.
    // These tests focus on the pure helper functions and filtering logic.

    #[test]
    fn test_window_info_workspace_filter_logic() {
        // Test the filtering logic directly
        let windows = vec![
            make_window_in_workspace(1, "A", "1"),
            make_window_in_workspace(2, "B", "2"),
            make_window_in_workspace(3, "C", "1"),
        ];

        let current_workspace = "1";

        // Filter for current workspace
        let filtered: Vec<_> = windows
            .iter()
            .filter(|w| &w.workspace == current_workspace)
            .collect();

        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().any(|w| w.id == 1));
        assert!(filtered.iter().any(|w| w.id == 3));
    }

    #[test]
    fn test_window_info_no_filter_all_workspaces() {
        let windows = vec![
            make_window_in_workspace(1, "A", "1"),
            make_window_in_workspace(2, "B", "2"),
            make_window_in_workspace(3, "C", "3"),
        ];

        // No filtering = all windows
        assert_eq!(windows.len(), 3);
    }
}
