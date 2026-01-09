use clap::{Parser, Subcommand, ValueEnum};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum WorkspaceMode {
    /// Show windows from current workspace only
    #[default]
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
}

#[derive(Debug, Clone, Parser)]
#[command(name = "sway-alttab")]
#[command(about = "Windows-style Alt-Tab window switcher for Sway", long_about = None)]
pub struct Config {
    /// Workspace filtering mode (only applies to daemon mode)
    #[arg(short, long, value_enum, default_value_t)]
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
    #[must_use]
    pub fn command(&self) -> Command {
        self.command.clone().unwrap_or(Command::Daemon)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workspace_mode_default() {
        assert_eq!(WorkspaceMode::default(), WorkspaceMode::Current);
    }

    #[test]
    fn test_command_default_is_daemon() {
        let config = Config {
            mode: WorkspaceMode::default(),
            verbose: false,
            command: None,
        };
        assert!(matches!(config.command(), Command::Daemon));
    }

    #[test]
    fn test_command_show_when_specified() {
        let config = Config {
            mode: WorkspaceMode::default(),
            verbose: false,
            command: Some(Command::Show),
        };
        assert!(matches!(config.command(), Command::Show));
    }

    #[test]
    fn test_command_daemon_when_specified() {
        let config = Config {
            mode: WorkspaceMode::default(),
            verbose: false,
            command: Some(Command::Daemon),
        };
        assert!(matches!(config.command(), Command::Daemon));
    }

    #[test]
    fn test_workspace_mode_all() {
        let mode = WorkspaceMode::All;
        assert_eq!(mode, WorkspaceMode::All);
        assert_ne!(mode, WorkspaceMode::Current);
    }

    #[test]
    fn test_config_verbose_flag() {
        let config = Config {
            mode: WorkspaceMode::Current,
            verbose: true,
            command: None,
        };
        assert!(config.verbose);
    }
}
