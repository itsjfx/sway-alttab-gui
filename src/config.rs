use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
pub enum WorkspaceMode {
    /// Show windows from current workspace only
    Current,
    /// Show windows from all workspaces
    All,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    /// Run as daemon (default if no command specified)
    Daemon,
    /// Show the window switcher
    Show,
    /// Cycle to next window
    Next,
    /// Cycle to previous window
    Prev,
    /// Select current window and close switcher
    Select,
    /// Cancel switching without selecting
    Cancel,
    /// Query daemon status
    Status,
    /// Shutdown the daemon
    Shutdown,
}

#[derive(Debug, Clone, Parser)]
#[command(name = "sway-alttab")]
#[command(about = "Windows-style Alt-Tab window switcher for Sway", long_about = None)]
pub struct Config {
    /// Workspace filtering mode (only applies to daemon mode)
    #[arg(short, long, value_enum, default_value = "current")]
    pub mode: WorkspaceMode,

    /// Enable verbose logging
    #[arg(short, long)]
    pub verbose: bool,

    /// Command to execute
    #[command(subcommand)]
    pub command: Option<Command>,
}

impl Config {
    pub fn parse() -> Self {
        <Config as Parser>::parse()
    }

    /// Get the command, defaulting to Daemon if none specified
    pub fn command(&self) -> Command {
        self.command.clone().unwrap_or(Command::Daemon)
    }
}
