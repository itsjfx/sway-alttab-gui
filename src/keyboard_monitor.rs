use anyhow::{Context, Result};
use evdev::{Device, InputEventKind, Key};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyEvent {
    AltPressed,
    AltReleased,
    TabPressed,
    ShiftPressed,
    ShiftReleased,
}

pub struct KeyboardMonitor {
    device: Device,
}

impl KeyboardMonitor {
    /// Find and open a keyboard device
    pub fn new() -> Result<Self> {
        let device = Self::find_keyboard_device()
            .context("Failed to find keyboard device")?;

        info!("Using keyboard device: {:?}", device.name());

        Ok(KeyboardMonitor { device })
    }

    /// Find a suitable keyboard device from /dev/input/event*
    fn find_keyboard_device() -> Result<Device> {
        let devices = evdev::enumerate();

        // Look for a device that supports the keys we need
        for (_, mut device) in devices {
            if let Some(keys) = device.supported_keys() {
                // Check if device supports Alt, Shift, and Tab
                if keys.contains(Key::KEY_LEFTALT)
                    && keys.contains(Key::KEY_TAB)
                    && keys.contains(Key::KEY_LEFTSHIFT) {
                    debug!("Found suitable keyboard: {:?}", device.name());
                    return Ok(device);
                }
            }
        }

        anyhow::bail!("No suitable keyboard device found. Make sure you have permission to read /dev/input/event* devices.")
    }

    /// Start monitoring keyboard events and send them through the channel
    /// This runs in a blocking thread and communicates via the channel
    pub fn monitor_blocking(mut self, tx: mpsc::UnboundedSender<KeyEvent>) -> Result<()> {
        info!("Starting keyboard monitoring");

        loop {
            match self.device.fetch_events() {
                Ok(events) => {
                    for event in events {
                        if let InputEventKind::Key(key) = event.kind() {
                            let key_event = match (key, event.value()) {
                                (Key::KEY_LEFTALT | Key::KEY_RIGHTALT, 1) => Some(KeyEvent::AltPressed),
                                (Key::KEY_LEFTALT | Key::KEY_RIGHTALT, 0) => Some(KeyEvent::AltReleased),
                                (Key::KEY_TAB, 1) => Some(KeyEvent::TabPressed),
                                (Key::KEY_LEFTSHIFT | Key::KEY_RIGHTSHIFT, 1) => Some(KeyEvent::ShiftPressed),
                                (Key::KEY_LEFTSHIFT | Key::KEY_RIGHTSHIFT, 0) => Some(KeyEvent::ShiftReleased),
                                _ => None,
                            };

                            if let Some(ke) = key_event {
                                debug!("Key event: {:?}", ke);
                                if tx.send(ke).is_err() {
                                    warn!("Failed to send key event, receiver dropped");
                                    return Ok(());
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    if e.kind() == std::io::ErrorKind::WouldBlock {
                        // No events available, sleep briefly
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    } else {
                        return Err(e.into());
                    }
                }
            }
        }
    }
}

/// Check if the current user has permission to read keyboard devices
pub fn check_permissions() -> Result<()> {
    let test_device = KeyboardMonitor::find_keyboard_device();

    match test_device {
        Ok(_) => {
            info!("Keyboard device access OK");
            Ok(())
        }
        Err(e) => {
            eprintln!("ERROR: Cannot access keyboard devices.");
            eprintln!("This daemon needs permission to read /dev/input/event* devices.");
            eprintln!("\nTo fix this, add your user to the 'input' group:");
            eprintln!("  sudo usermod -aG input $USER");
            eprintln!("  (then log out and log back in)");
            eprintln!("\nOr run with elevated permissions (not recommended):");
            eprintln!("  sudo {}", std::env::current_exe().unwrap_or_default().display());
            Err(e)
        }
    }
}
