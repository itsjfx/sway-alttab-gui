use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Commands sent from CLI client to daemon
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IpcCommand {
    /// Show the window switcher (start switching mode)
    Show,
    /// Cycle to next window
    Next,
    /// Cycle to previous window
    Prev,
    /// Select current window and close switcher
    Select,
    /// Cancel switching without selecting
    Cancel,
    /// Query daemon status (for debugging)
    Status,
    /// Shutdown the daemon gracefully
    Shutdown,
}

/// Response from daemon to CLI client
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IpcResponse {
    /// Command executed successfully
    Ok,
    /// Error occurred
    Error(String),
    /// Status response
    Status {
        switching: bool,
        window_count: usize,
        current_index: Option<usize>,
    },
}

/// Get the path to the Unix socket
pub fn get_socket_path() -> Result<PathBuf> {
    let runtime_dir = dirs::runtime_dir()
        .or_else(dirs::cache_dir)
        .context("Could not determine runtime directory")?;

    Ok(runtime_dir.join("sway-alttab.sock"))
}

impl IpcCommand {
    /// Parse from simple string format (for text protocol)
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "show" => Some(IpcCommand::Show),
            "next" => Some(IpcCommand::Next),
            "prev" => Some(IpcCommand::Prev),
            "select" => Some(IpcCommand::Select),
            "cancel" => Some(IpcCommand::Cancel),
            "status" => Some(IpcCommand::Status),
            "shutdown" => Some(IpcCommand::Shutdown),
            _ => None,
        }
    }

    /// Convert to string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            IpcCommand::Show => "show",
            IpcCommand::Next => "next",
            IpcCommand::Prev => "prev",
            IpcCommand::Select => "select",
            IpcCommand::Cancel => "cancel",
            IpcCommand::Status => "status",
            IpcCommand::Shutdown => "shutdown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipc_command_from_str() {
        assert_eq!(IpcCommand::from_str("show"), Some(IpcCommand::Show));
        assert_eq!(IpcCommand::from_str("next"), Some(IpcCommand::Next));
        assert_eq!(IpcCommand::from_str("prev"), Some(IpcCommand::Prev));
        assert_eq!(IpcCommand::from_str("select"), Some(IpcCommand::Select));
        assert_eq!(IpcCommand::from_str("cancel"), Some(IpcCommand::Cancel));
        assert_eq!(IpcCommand::from_str("status"), Some(IpcCommand::Status));
        assert_eq!(IpcCommand::from_str("shutdown"), Some(IpcCommand::Shutdown));
        assert_eq!(IpcCommand::from_str("invalid"), None);
    }

    #[test]
    fn test_ipc_command_from_str_case_insensitive() {
        assert_eq!(IpcCommand::from_str("SHOW"), Some(IpcCommand::Show));
        assert_eq!(IpcCommand::from_str("Show"), Some(IpcCommand::Show));
        assert_eq!(IpcCommand::from_str("  show  "), Some(IpcCommand::Show));
    }

    #[test]
    fn test_ipc_command_as_str() {
        assert_eq!(IpcCommand::Show.as_str(), "show");
        assert_eq!(IpcCommand::Next.as_str(), "next");
        assert_eq!(IpcCommand::Prev.as_str(), "prev");
        assert_eq!(IpcCommand::Select.as_str(), "select");
        assert_eq!(IpcCommand::Cancel.as_str(), "cancel");
        assert_eq!(IpcCommand::Status.as_str(), "status");
        assert_eq!(IpcCommand::Shutdown.as_str(), "shutdown");
    }

    #[test]
    fn test_ipc_command_roundtrip() {
        let commands = [
            IpcCommand::Show,
            IpcCommand::Next,
            IpcCommand::Prev,
            IpcCommand::Select,
            IpcCommand::Cancel,
            IpcCommand::Status,
            IpcCommand::Shutdown,
        ];

        for cmd in commands {
            let s = cmd.as_str();
            let parsed = IpcCommand::from_str(s);
            assert_eq!(parsed, Some(cmd));
        }
    }

    #[test]
    fn test_ipc_response_serialization() {
        let ok_response = IpcResponse::Ok;
        let json = serde_json::to_string(&ok_response).unwrap();
        assert!(json.contains("ok"));

        let error_response = IpcResponse::Error("test error".to_string());
        let json = serde_json::to_string(&error_response).unwrap();
        assert!(json.contains("error"));
        assert!(json.contains("test error"));

        let status_response = IpcResponse::Status {
            switching: true,
            window_count: 5,
            current_index: Some(2),
        };
        let json = serde_json::to_string(&status_response).unwrap();
        assert!(json.contains("switching"));
        assert!(json.contains("true"));
    }

    #[test]
    fn test_get_socket_path() {
        let path = get_socket_path().unwrap();
        assert!(path.ends_with("sway-alttab.sock"));
    }
}
