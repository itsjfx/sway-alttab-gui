use clap::{Args, Parser, Subcommand, ValueEnum};
use gtk4::gdk::{Key, ModifierType};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum WorkspaceMode {
    /// Show windows from current workspace only
    #[default]
    Current,
    /// Show windows from all workspaces
    All,
}

/// Modifier key whose release finalizes the window selection.
///
/// Shift is deliberately unsupported: `Shift+Tab` is already the "cycle backward"
/// binding inside the switcher, so using Shift as the hold modifier would leave
/// the user unable to cycle forward.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ReleaseKey {
    #[default]
    Alt,
    Super,
    Ctrl,
}

impl ReleaseKey {
    pub fn mask(self) -> ModifierType {
        match self {
            Self::Alt => ModifierType::ALT_MASK,
            Self::Super => ModifierType::SUPER_MASK,
            Self::Ctrl => ModifierType::CONTROL_MASK,
        }
    }

    pub fn keys(self) -> [Key; 2] {
        match self {
            Self::Alt => [Key::Alt_L, Key::Alt_R],
            Self::Super => [Key::Super_L, Key::Super_R],
            Self::Ctrl => [Key::Control_L, Key::Control_R],
        }
    }
}

impl std::str::FromStr for ReleaseKey {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "alt" | "mod1" => Ok(Self::Alt),
            "super" | "mod4" | "win" => Ok(Self::Super),
            "ctrl" | "control" => Ok(Self::Ctrl),
            other => Err(format!(
                "unsupported release key '{other}' (expected one of: alt/mod1, super/mod4/win, ctrl/control)"
            )),
        }
    }
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

    /// Modifier whose release finalizes selection
    /// (accepts: alt/mod1, super/mod4/win, ctrl/control)
    #[arg(long, value_parser = <ReleaseKey as std::str::FromStr>::from_str, default_value = "alt")]
    pub release_key: ReleaseKey,
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

    /// Get the configured release key
    #[must_use]
    pub fn release_key(&self) -> ReleaseKey {
        self.daemon_args().release_key
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
                release_key: ReleaseKey::Alt,
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
                release_key: ReleaseKey::Alt,
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
                release_key: ReleaseKey::Alt,
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
                release_key: ReleaseKey::Alt,
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

    #[test]
    fn test_release_key_default_is_alt() {
        let config = Config::try_parse_from(["sway-alttab-gui"]).unwrap();
        assert_eq!(config.release_key(), ReleaseKey::Alt);
    }

    #[test]
    fn test_release_key_aliases() {
        use std::str::FromStr;
        assert_eq!(ReleaseKey::from_str("alt").unwrap(), ReleaseKey::Alt);
        assert_eq!(ReleaseKey::from_str("Mod1").unwrap(), ReleaseKey::Alt);
        assert_eq!(ReleaseKey::from_str("super").unwrap(), ReleaseKey::Super);
        assert_eq!(ReleaseKey::from_str("MOD4").unwrap(), ReleaseKey::Super);
        assert_eq!(ReleaseKey::from_str("win").unwrap(), ReleaseKey::Super);
        assert_eq!(ReleaseKey::from_str("ctrl").unwrap(), ReleaseKey::Ctrl);
        assert_eq!(ReleaseKey::from_str("Control").unwrap(), ReleaseKey::Ctrl);
    }

    #[test]
    fn test_release_key_rejects_unsupported() {
        use std::str::FromStr;
        // Shift is rejected because it conflicts with the Shift+Tab backward-cycle binding.
        assert!(ReleaseKey::from_str("shift").is_err());
        assert!(ReleaseKey::from_str("mod2").is_err());
        assert!(ReleaseKey::from_str("hyper").is_err());
        assert!(ReleaseKey::from_str("meta").is_err());
        assert!(ReleaseKey::from_str("mod5").is_err());
        assert!(ReleaseKey::from_str("").is_err());
    }

    #[test]
    fn test_parse_release_key_flag() {
        let config =
            Config::try_parse_from(["sway-alttab-gui", "daemon", "--release-key", "mod4"])
                .unwrap();
        assert_eq!(config.release_key(), ReleaseKey::Super);
    }

    #[test]
    fn test_parse_release_key_rejects_bad_value() {
        let result =
            Config::try_parse_from(["sway-alttab-gui", "daemon", "--release-key", "mod2"]);
        assert!(result.is_err());
    }
}
