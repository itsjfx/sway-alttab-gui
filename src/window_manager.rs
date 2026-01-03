use anyhow::Result;
use swayipc::{Connection, Node, NodeType};
use crate::config::WorkspaceMode;

#[derive(Debug, Clone)]
pub struct WindowInfo {
    pub id: i64,
    pub app_id: Option<String>,
    pub title: String,
    pub workspace: String,
    pub window_class: Option<String>, // WM_CLASS for X11 windows
}

impl WindowInfo {
    fn from_node(node: &Node, workspace: String) -> Option<Self> {
        // Only include actual windows (views), not containers
        // Windows have a pid, containers don't
        if node.node_type == NodeType::Con && node.pid.is_some() {
            // Extract WM_CLASS from X11/XWayland window properties
            let window_class = node.window_properties.as_ref()
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

pub struct WindowManager {
    windows: Vec<WindowInfo>,
    current_workspace: Option<String>,
}

impl WindowManager {
    pub fn new() -> Result<Self> {
        let mut manager = WindowManager {
            windows: Vec::new(),
            current_workspace: None,
        };
        manager.refresh()?;
        Ok(manager)
    }

    /// Refresh the window list from Sway
    /// This preserves the MRU order for existing windows
    pub fn refresh(&mut self) -> Result<()> {
        use tracing::debug;

        let mut connection = Connection::new()?;
        let tree = connection.get_tree()?;

        // Save the current MRU order
        let old_windows = std::mem::take(&mut self.windows);

        // Collect current windows from Sway
        self.collect_windows(&tree, String::new());

        // Build a set of current window IDs for quick lookup
        let current_ids: std::collections::HashSet<i64> =
            self.windows.iter().map(|w| w.id).collect();

        // Rebuild the list preserving MRU order:
        // 1. Keep existing windows in their MRU order
        // 2. Add new windows at the end
        let mut new_windows = Vec::new();

        // First, add windows that existed before, in their MRU order
        for old_win in old_windows {
            if current_ids.contains(&old_win.id) {
                // Window still exists, keep it in MRU order
                new_windows.push(old_win);
            }
            // Windows that no longer exist are dropped
        }

        // Then add any new windows that weren't in the old list
        for new_win in self.windows.drain(..) {
            if !new_windows.iter().any(|w| w.id == new_win.id) {
                new_windows.push(new_win);
            }
        }

        self.windows = new_windows;
        debug!("Refreshed to {} windows (preserved MRU order)", self.windows.len());

        // Get current workspace
        if let Ok(workspaces) = connection.get_workspaces() {
            self.current_workspace = workspaces
                .iter()
                .find(|w| w.focused)
                .map(|w| w.name.clone());
        }

        Ok(())
    }

    fn collect_windows(&mut self, node: &Node, current_workspace: String) {
        // Update workspace name if we encounter a workspace node
        let workspace = if node.node_type == NodeType::Workspace {
            node.name.clone().unwrap_or(current_workspace.clone())
        } else {
            current_workspace.clone()
        };

        // Add window if it's an actual window (has a pid)
        if let Some(window) = WindowInfo::from_node(node, workspace.clone()) {
            self.windows.push(window);
        }

        // Recurse into children
        for child in &node.nodes {
            self.collect_windows(child, workspace.clone());
        }
        for child in &node.floating_nodes {
            self.collect_windows(child, workspace.clone());
        }
    }

    fn find_focused_window(&self, node: &Node) -> Option<i64> {
        // Check if this node is a focused window (not just a focused container)
        // Windows have a pid, containers don't
        if node.node_type == NodeType::Con && node.focused && node.pid.is_some() {
            return Some(node.id);
        }

        // Recurse into children
        for child in &node.nodes {
            if let Some(id) = self.find_focused_window(child) {
                return Some(id);
            }
        }
        for child in &node.floating_nodes {
            if let Some(id) = self.find_focused_window(child) {
                return Some(id);
            }
        }

        None
    }

    /// Move window to front of MRU list
    pub fn on_focus(&mut self, window_id: i64) {
        if let Some(pos) = self.windows.iter().position(|w| w.id == window_id) {
            let window = self.windows.remove(pos);
            self.windows.insert(0, window);
        }
    }

    /// Remove window from list
    pub fn on_window_close(&mut self, window_id: i64) {
        self.windows.retain(|w| w.id != window_id);
    }

    /// Add new window to list
    pub fn on_window_open(&mut self, window: WindowInfo) {
        self.windows.insert(0, window);
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
    pub fn focus_window(&self, window_id: i64) -> Result<()> {
        let mut connection = Connection::new()?;
        connection.run_command(format!("[con_id={}] focus", window_id))?;
        Ok(())
    }

    /// Get current workspace name
    pub fn get_current_workspace(&self) -> Option<&str> {
        self.current_workspace.as_deref()
    }
}
