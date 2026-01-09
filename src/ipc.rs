/// Commands sent from UI to daemon (keyboard input)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputCommand {
    /// Cycle to next window
    Next,
    /// Cycle to previous window
    Prev,
    /// Select current window and close switcher
    Select,
    /// Cancel switching without selecting
    Cancel,
}
