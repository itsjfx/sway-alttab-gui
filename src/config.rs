use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum WorkspaceMode {
    /// Show windows from current workspace only
    #[default]
    Current,
    /// Show windows from all workspaces
    All,
}

/// Arguments shared between root command and daemon subcommand
#[derive(Debug, Clone, Default, Args)]
pub struct DaemonArgs {
    /// Workspace filtering mode
    #[arg(short, long, value_enum, default_value_t)]
    pub mode: WorkspaceMode,

    /// Enable verbose logging
    #[arg(short, long)]
    pub verbose: bool,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    /// Run as daemon (default if no command specified)
    Daemon(DaemonArgs),
    /// Show the window switcher
    Show,
}

#[derive(Debug, Clone, Parser)]
#[command(name = "sway-alttab-gui")]
#[command(about = "Windows-style Alt-Tab window switcher for Sway", long_about = None)]
pub struct Config {
    /// Shared daemon arguments (used when no subcommand specified)
    #[command(flatten)]
    pub args: DaemonArgs,

    /// Command to execute
    #[command(subcommand)]
    pub command: Option<Command>,
}

impl Config {
    pub fn parse() -> Self {
        <Config as Parser>::parse()
    }

    /// Check if this is the show command
    #[must_use]
    pub fn is_show(&self) -> bool {
        matches!(self.command, Some(Command::Show))
    }

    /// Get the effective daemon args (from subcommand if specified, otherwise from root)
    #[must_use]
    pub fn daemon_args(&self) -> &DaemonArgs {
        match &self.command {
            Some(Command::Daemon(args)) => args,
            _ => &self.args,
        }
    }

    /// Get the workspace mode
    #[must_use]
    pub fn mode(&self) -> WorkspaceMode {
        self.daemon_args().mode
    }

    /// Get verbose flag
    #[must_use]
    pub fn verbose(&self) -> bool {
        self.daemon_args().verbose
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
            args: DaemonArgs::default(),
            command: None,
        };
        assert!(!config.is_show());
        assert!(!config.is_show());
    }

    #[test]
    fn test_command_show_when_specified() {
        let config = Config {
            args: DaemonArgs::default(),
            command: Some(Command::Show),
        };
        assert!(config.is_show());
        assert!(!!config.is_show());
    }

    #[test]
    fn test_command_daemon_when_specified() {
        let config = Config {
            args: DaemonArgs::default(),
            command: Some(Command::Daemon(DaemonArgs::default())),
        };
        assert!(!config.is_show());
        assert!(!config.is_show());
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
            args: DaemonArgs {
                mode: WorkspaceMode::Current,
                verbose: true,
            },
            command: None,
        };
        assert!(config.verbose());
    }

    #[test]
    fn test_daemon_args_from_subcommand() {
        let config = Config {
            args: DaemonArgs::default(),
            command: Some(Command::Daemon(DaemonArgs {
                mode: WorkspaceMode::All,
                verbose: true,
            })),
        };
        assert_eq!(config.mode(), WorkspaceMode::All);
        assert!(config.verbose());
    }

    #[test]
    fn test_daemon_args_from_root_when_no_subcommand() {
        let config = Config {
            args: DaemonArgs {
                mode: WorkspaceMode::All,
                verbose: true,
            },
            command: None,
        };
        assert_eq!(config.mode(), WorkspaceMode::All);
        assert!(config.verbose());
    }

    #[test]
    fn test_show_command_uses_root_args() {
        // When using show command, daemon_args() should return root args
        let config = Config {
            args: DaemonArgs {
                mode: WorkspaceMode::All,
                verbose: true,
            },
            command: Some(Command::Show),
        };
        assert_eq!(config.mode(), WorkspaceMode::All);
        assert!(config.verbose());
    }

    #[test]
    fn test_daemon_args_default_values() {
        let args = DaemonArgs::default();
        assert_eq!(args.mode, WorkspaceMode::Current);
        assert!(!args.verbose);
    }

    // CLI parsing tests
    #[test]
    fn test_parse_no_args() {
        let config = Config::try_parse_from(["sway-alttab-gui"]).unwrap();
        assert!(!config.is_show());
        assert_eq!(config.mode(), WorkspaceMode::Current);
        assert!(!config.verbose());
    }

    #[test]
    fn test_parse_root_verbose() {
        let config = Config::try_parse_from(["sway-alttab-gui", "--verbose"]).unwrap();
        assert!(!config.is_show());
        assert!(config.verbose());
    }

    #[test]
    fn test_parse_root_mode_all() {
        let config = Config::try_parse_from(["sway-alttab-gui", "--mode", "all"]).unwrap();
        assert!(!config.is_show());
        assert_eq!(config.mode(), WorkspaceMode::All);
    }

    #[test]
    fn test_parse_daemon_subcommand() {
        let config = Config::try_parse_from(["sway-alttab-gui", "daemon"]).unwrap();
        assert!(!config.is_show());
        assert_eq!(config.mode(), WorkspaceMode::Current);
        assert!(!config.verbose());
    }

    #[test]
    fn test_parse_daemon_with_verbose() {
        let config =
            Config::try_parse_from(["sway-alttab-gui", "daemon", "--verbose"]).unwrap();
        assert!(!config.is_show());
        assert!(config.verbose());
    }

    #[test]
    fn test_parse_daemon_with_mode_all() {
        let config =
            Config::try_parse_from(["sway-alttab-gui", "daemon", "--mode", "all"]).unwrap();
        assert!(!config.is_show());
        assert_eq!(config.mode(), WorkspaceMode::All);
    }

    #[test]
    fn test_parse_daemon_with_all_flags() {
        let config = Config::try_parse_from([
            "sway-alttab-gui",
            "daemon",
            "--verbose",
            "--mode",
            "all",
        ])
        .unwrap();
        assert!(!config.is_show());
        assert!(config.verbose());
        assert_eq!(config.mode(), WorkspaceMode::All);
    }

    #[test]
    fn test_parse_show_subcommand() {
        let config = Config::try_parse_from(["sway-alttab-gui", "show"]).unwrap();
        assert!(config.is_show());
        assert!(!!config.is_show());
    }

    #[test]
    fn test_parse_short_flags() {
        let config =
            Config::try_parse_from(["sway-alttab-gui", "daemon", "-v", "-m", "all"]).unwrap();
        assert!(config.verbose());
        assert_eq!(config.mode(), WorkspaceMode::All);
    }
}
