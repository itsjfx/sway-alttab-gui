use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{error, info};

/// Manages the daemon's PID file for single-instance enforcement
pub struct Pidfile {
    path: PathBuf,
}

impl Pidfile {
    /// Create a new Pidfile using the default path (XDG_RUNTIME_DIR or ~/.cache)
    pub fn new() -> Result<Self> {
        let path = dirs::runtime_dir()
            .or_else(dirs::cache_dir)
            .context("Could not determine runtime directory")?
            .join("sway-alttab-gui.pid");
        Ok(Self { path })
    }

    /// Create a Pidfile with a custom path (useful for testing)
    #[cfg(test)]
    pub fn with_path(path: PathBuf) -> Self {
        Self { path }
    }

    /// Get the path to the pidfile
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Read the PID from the pidfile if it exists.
    /// Returns Ok(None) if no pidfile exists, Ok(Some(pid)) if valid.
    pub fn read(&self) -> Result<Option<i32>> {
        if !self.path.exists() {
            return Ok(None);
        }

        let pid_str = fs::read_to_string(&self.path).context("Failed to read pidfile")?;
        let pid: i32 = pid_str.trim().parse().context("Invalid PID in pidfile")?;

        Ok(Some(pid))
    }

    /// Check if another instance is already running
    pub fn check(&self) -> Result<()> {
        let Some(pid) = self.read()? else {
            return Ok(());
        };

        if process_exists(pid) {
            anyhow::bail!(
                "Another instance of sway-alttab-gui is already running (PID: {}). \
                 If this is incorrect, remove the pidfile at: {}",
                pid,
                self.path.display()
            );
        }

        // Stale pidfile, remove it
        info!("Removing stale pidfile (PID {} not found)", pid);
        if let Err(e) = fs::remove_file(&self.path) {
            tracing::warn!("Failed to remove stale pidfile: {}", e);
        }

        Ok(())
    }

    /// Create the pidfile and return a guard that removes it on drop
    pub fn create(&self) -> Result<PidfileGuard> {
        let pid = std::process::id();

        fs::write(&self.path, pid.to_string()).context("Failed to write pidfile")?;

        info!("Created pidfile at {} with PID {}", self.path.display(), pid);

        Ok(PidfileGuard {
            path: self.path.clone(),
        })
    }
}

/// Check if a process with the given PID exists
fn process_exists(pid: i32) -> bool {
    // Check if /proc/<pid> exists (Linux-specific, but this is for Sway which is Linux-only)
    PathBuf::from(format!("/proc/{}", pid)).exists()
}

/// Guard that removes the pidfile when dropped
pub struct PidfileGuard {
    path: PathBuf,
}

impl Drop for PidfileGuard {
    fn drop(&mut self) {
        if let Err(e) = fs::remove_file(&self.path) {
            error!("Failed to remove pidfile: {}", e);
        } else {
            info!("Removed pidfile at {}", self.path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_pidfile(dir: &TempDir) -> Pidfile {
        Pidfile::with_path(dir.path().join("test.pid"))
    }

    #[test]
    fn test_read_nonexistent_returns_none() {
        let dir = TempDir::new().unwrap();
        let pidfile = create_test_pidfile(&dir);

        let result = pidfile.read().unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_read_existing_pidfile() {
        let dir = TempDir::new().unwrap();
        let pidfile = create_test_pidfile(&dir);

        fs::write(pidfile.path(), "12345").unwrap();

        let result = pidfile.read().unwrap();
        assert_eq!(result, Some(12345));
    }

    #[test]
    fn test_read_pidfile_with_whitespace() {
        let dir = TempDir::new().unwrap();
        let pidfile = create_test_pidfile(&dir);

        fs::write(pidfile.path(), "  12345\n").unwrap();

        let result = pidfile.read().unwrap();
        assert_eq!(result, Some(12345));
    }

    #[test]
    fn test_read_invalid_pid_returns_error() {
        let dir = TempDir::new().unwrap();
        let pidfile = create_test_pidfile(&dir);

        fs::write(pidfile.path(), "not_a_number").unwrap();

        let result = pidfile.read();
        assert!(result.is_err());
    }

    #[test]
    fn test_create_writes_current_pid() {
        let dir = TempDir::new().unwrap();
        let pidfile = create_test_pidfile(&dir);

        let _guard = pidfile.create().unwrap();

        let content = fs::read_to_string(pidfile.path()).unwrap();
        let written_pid: u32 = content.parse().unwrap();
        assert_eq!(written_pid, std::process::id());
    }

    #[test]
    fn test_guard_removes_pidfile_on_drop() {
        let dir = TempDir::new().unwrap();
        let pidfile = create_test_pidfile(&dir);
        let path = pidfile.path().to_path_buf();

        {
            let _guard = pidfile.create().unwrap();
            assert!(path.exists());
        }

        assert!(!path.exists());
    }

    #[test]
    fn test_check_passes_when_no_pidfile() {
        let dir = TempDir::new().unwrap();
        let pidfile = create_test_pidfile(&dir);

        let result = pidfile.check();
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_removes_stale_pidfile() {
        let dir = TempDir::new().unwrap();
        let pidfile = create_test_pidfile(&dir);

        // Write a PID that definitely doesn't exist (very high number)
        fs::write(pidfile.path(), "999999999").unwrap();

        let result = pidfile.check();
        assert!(result.is_ok());
        assert!(!pidfile.path().exists());
    }

    #[test]
    fn test_check_fails_when_process_running() {
        let dir = TempDir::new().unwrap();
        let pidfile = create_test_pidfile(&dir);

        // Write our own PID (which is definitely running)
        fs::write(pidfile.path(), std::process::id().to_string()).unwrap();

        let result = pidfile.check();
        assert!(result.is_err());

        // Clean up
        fs::remove_file(pidfile.path()).ok();
    }

    #[test]
    fn test_process_exists_for_current_process() {
        let pid = std::process::id() as i32;
        assert!(process_exists(pid));
    }

    #[test]
    fn test_process_exists_for_nonexistent_process() {
        // Very high PID that almost certainly doesn't exist
        assert!(!process_exists(999999999));
    }

    #[test]
    fn test_path_returns_correct_path() {
        let dir = TempDir::new().unwrap();
        let expected_path = dir.path().join("test.pid");
        let pidfile = Pidfile::with_path(expected_path.clone());

        assert_eq!(pidfile.path(), expected_path);
    }
}
