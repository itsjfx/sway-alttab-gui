use clap::{Parser, ValueEnum};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
pub enum WorkspaceMode {
    /// Show windows from current workspace only
    Current,
    /// Show windows from all workspaces
    All,
}

#[derive(Debug, Clone, Parser)]
#[command(name = "sway-alttab")]
#[command(about = "Windows-style Alt-Tab window switcher for Sway", long_about = None)]
pub struct Config {
    /// Workspace filtering mode
    #[arg(short, long, value_enum, default_value = "current")]
    pub mode: WorkspaceMode,

    /// Enable verbose logging
    #[arg(short, long)]
    pub verbose: bool,
}

impl Config {
    pub fn parse() -> Self {
        Config::parse_from(std::env::args())
    }
}
