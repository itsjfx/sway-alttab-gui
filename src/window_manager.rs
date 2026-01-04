use anyhow::Result;
use std::collections::HashSet;
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
        let current_windows = collect_windows(&tree, String::new());

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
/// 2. Previously known windows in their MRU order (if still present)
/// 3. Newly discovered windows at the end
fn preserve_mru_order(
    old_windows: Vec<WindowInfo>,
    current_windows: Vec<WindowInfo>,
    focused_id: Option<i64>,
) -> Vec<WindowInfo> {
    let current_ids: HashSet<i64> = current_windows.iter().map(|w| w.id).collect();
    let mut result = Vec::with_capacity(current_windows.len());
    let mut added_ids = HashSet::new();

    // 1. Add focused window first if it exists in current windows
    if let Some(fid) = focused_id {
        if let Some(focused_win) = current_windows.iter().find(|w| w.id == fid).cloned() {
            added_ids.insert(fid);
            result.push(focused_win);
        }
    }

    // 2. Add windows from old list that still exist (preserving MRU order)
    for old_win in old_windows {
        if current_ids.contains(&old_win.id) && !added_ids.contains(&old_win.id) {
            added_ids.insert(old_win.id);
            result.push(old_win);
        }
    }

    // 3. Add any new windows not in the old list
    for new_win in current_windows {
        if !added_ids.contains(&new_win.id) {
            result.push(new_win);
        }
    }

    result
}

/// Recursively collect all windows from a Sway node tree.
/// Returns a flat list of WindowInfo structs.
fn collect_windows(node: &Node, current_workspace: String) -> Vec<WindowInfo> {
    let mut windows = Vec::new();

    // Update workspace name if we encounter a workspace node
    let workspace = if node.node_type == NodeType::Workspace {
        node.name.clone().unwrap_or(current_workspace.clone())
    } else {
        current_workspace.clone()
    };

    // Add window if it's an actual window (has a pid)
    if let Some(window) = WindowInfo::from_node(node, workspace.clone()) {
        windows.push(window);
    }

    // Recurse into children
    for child in &node.nodes {
        windows.extend(collect_windows(child, workspace.clone()));
    }
    for child in &node.floating_nodes {
        windows.extend(collect_windows(child, workspace.clone()));
    }

    windows
}

/// Find the currently focused window in a Sway node tree.
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
}
