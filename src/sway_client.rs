//! Sway IPC abstraction for testability.
//!
//! This module provides a trait that abstracts Sway IPC operations,
//! allowing for mock implementations in tests.

use anyhow::Result;
use swayipc::{Connection, Node, Workspace};

/// Trait for Sway IPC operations.
///
/// This abstraction allows for mock implementations in tests.
pub trait SwayClient {
    /// Get the full window tree from Sway
    fn get_tree(&mut self) -> Result<Node>;

    /// Get the list of workspaces
    fn get_workspaces(&mut self) -> Result<Vec<Workspace>>;

    /// Focus a window by its container ID
    fn focus_window(&mut self, window_id: i64) -> Result<()>;
}

/// Real implementation using swayipc
pub struct RealSwayClient {
    connection: Connection,
}

impl RealSwayClient {
    /// Create a new connection to Sway
    pub fn new() -> Result<Self> {
        let connection = Connection::new()?;
        Ok(RealSwayClient { connection })
    }
}

impl SwayClient for RealSwayClient {
    fn get_tree(&mut self) -> Result<Node> {
        Ok(self.connection.get_tree()?)
    }

    fn get_workspaces(&mut self) -> Result<Vec<Workspace>> {
        Ok(self.connection.get_workspaces()?)
    }

    fn focus_window(&mut self, window_id: i64) -> Result<()> {
        self.connection
            .run_command(format!("[con_id={}] focus", window_id))?;
        Ok(())
    }
}

// Note: A full mock implementation would require creating swayipc Node structs,
// but these are non-exhaustive and complex. For unit testing, consider:
// 1. Testing the pure functions (preserve_mru_order, collect_windows, etc.) directly
// 2. Using integration tests that connect to a real Sway instance
//
// The SwayClient trait is provided for future extensibility and to document
// the interface, but the tests for window_manager use the extracted pure functions.
