use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

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

/// Error returned when parsing an invalid IpcCommand string
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseIpcCommandError;

impl fmt::Display for ParseIpcCommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid IPC command")
    }
}

impl std::error::Error for ParseIpcCommandError {}

impl FromStr for IpcCommand {
    type Err = ParseIpcCommandError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "show" => Ok(IpcCommand::Show),
            "next" => Ok(IpcCommand::Next),
            "prev" => Ok(IpcCommand::Prev),
            "select" => Ok(IpcCommand::Select),
            "cancel" => Ok(IpcCommand::Cancel),
            "status" => Ok(IpcCommand::Status),
            "shutdown" => Ok(IpcCommand::Shutdown),
            _ => Err(ParseIpcCommandError),
        }
    }
}

impl fmt::Display for IpcCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            IpcCommand::Show => "show",
            IpcCommand::Next => "next",
            IpcCommand::Prev => "prev",
            IpcCommand::Select => "select",
            IpcCommand::Cancel => "cancel",
            IpcCommand::Status => "status",
            IpcCommand::Shutdown => "shutdown",
        };
        write!(f, "{}", s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipc_command_from_str() {
        assert_eq!("show".parse(), Ok(IpcCommand::Show));
        assert_eq!("next".parse(), Ok(IpcCommand::Next));
        assert_eq!("prev".parse(), Ok(IpcCommand::Prev));
        assert_eq!("select".parse(), Ok(IpcCommand::Select));
        assert_eq!("cancel".parse(), Ok(IpcCommand::Cancel));
        assert_eq!("status".parse(), Ok(IpcCommand::Status));
        assert_eq!("shutdown".parse(), Ok(IpcCommand::Shutdown));
        assert_eq!("invalid".parse::<IpcCommand>(), Err(ParseIpcCommandError));
    }

    #[test]
    fn test_ipc_command_from_str_case_insensitive() {
        assert_eq!("SHOW".parse(), Ok(IpcCommand::Show));
        assert_eq!("Show".parse(), Ok(IpcCommand::Show));
        assert_eq!("  show  ".parse(), Ok(IpcCommand::Show));
    }

    #[test]
    fn test_ipc_command_display() {
        assert_eq!(IpcCommand::Show.to_string(), "show");
        assert_eq!(IpcCommand::Next.to_string(), "next");
        assert_eq!(IpcCommand::Prev.to_string(), "prev");
        assert_eq!(IpcCommand::Select.to_string(), "select");
        assert_eq!(IpcCommand::Cancel.to_string(), "cancel");
        assert_eq!(IpcCommand::Status.to_string(), "status");
        assert_eq!(IpcCommand::Shutdown.to_string(), "shutdown");
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
            let s = cmd.to_string();
            let parsed: IpcCommand = s.parse().unwrap();
            assert_eq!(parsed, cmd);
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
