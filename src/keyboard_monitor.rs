use anyhow::{Context, Result};
use evdev::{Device, InputEventKind, Key};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// Device names that should be excluded from keyboard detection.
/// These devices may report supporting keyboard keys but aren't real keyboards.
const EXCLUDED_DEVICE_NAMES: &[&str] = &[
    "Yubico",
    "YubiKey",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyEvent {
    AltPressed,
    AltReleased,
    TabPressed,
    ShiftPressed,
    ShiftReleased,
}

/// Trait for keyboard event sources.
///
/// This abstraction allows for mock implementations in tests.
#[allow(dead_code)]
pub trait KeyboardEventSource: Send {
    /// Block and wait for the next keyboard event.
    /// Returns None when the source is exhausted or an error occurs.
    fn next_event(&mut self) -> Option<KeyEvent>;
}

pub struct KeyboardMonitor {
    device: Device,
}

impl KeyboardMonitor {
    /// Find and open a keyboard device
    pub fn new(device_name: Option<&str>) -> Result<Self> {
        let device = Self::find_keyboard_device(device_name)
            .context("Failed to find keyboard device")?;

        info!("Using keyboard device: {:?}", device.name());

        Ok(KeyboardMonitor { device })
    }

    /// Check if a device name should be excluded
    fn is_excluded(name: &str) -> bool {
        EXCLUDED_DEVICE_NAMES.iter().any(|excluded| name.contains(excluded))
    }

    /// Find a suitable keyboard device from /dev/input/event*
    fn find_keyboard_device(device_name: Option<&str>) -> Result<Device> {
        let devices = evdev::enumerate();

        // If user specified a device name, find exact match
        if let Some(requested_name) = device_name {
            for (_, device) in devices {
                if let Some(name) = device.name() {
                    if name == requested_name {
                        info!("Found requested keyboard device: {:?}", name);
                        return Ok(device);
                    }
                }
            }
            anyhow::bail!(
                "Keyboard device '{}' not found. Check available devices with: cat /proc/bus/input/devices",
                requested_name
            );
        }

        // Otherwise, look for a device that supports the keys we need
        for (_, device) in evdev::enumerate() {
            if let Some(name) = device.name() {
                if Self::is_excluded(name) {
                    debug!("Skipping excluded device: {:?}", name);
                    continue;
                }
            }

            if let Some(keys) = device.supported_keys() {
                // Check if device supports Alt, Shift, and Tab
                if keys.contains(Key::KEY_LEFTALT)
                    && keys.contains(Key::KEY_TAB)
                    && keys.contains(Key::KEY_LEFTSHIFT)
                {
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
pub fn check_permissions(device_name: Option<&str>) -> Result<()> {
    let test_device = KeyboardMonitor::find_keyboard_device(device_name);

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

#[cfg(test)]
pub mod mock {
    use super::*;
    use std::collections::VecDeque;

    /// Mock implementation for testing
    pub struct MockKeyboardSource {
        events: VecDeque<KeyEvent>,
    }

    impl MockKeyboardSource {
        pub fn new() -> Self {
            MockKeyboardSource {
                events: VecDeque::new(),
            }
        }

        /// Add events to be returned by next_event
        pub fn add_events(&mut self, events: impl IntoIterator<Item = KeyEvent>) {
            self.events.extend(events);
        }
    }

    impl KeyboardEventSource for MockKeyboardSource {
        fn next_event(&mut self) -> Option<KeyEvent> {
            self.events.pop_front()
        }
    }

    #[test]
    fn test_mock_keyboard_source() {
        let mut source = MockKeyboardSource::new();
        source.add_events([
            KeyEvent::AltPressed,
            KeyEvent::TabPressed,
            KeyEvent::AltReleased,
        ]);

        assert_eq!(source.next_event(), Some(KeyEvent::AltPressed));
        assert_eq!(source.next_event(), Some(KeyEvent::TabPressed));
        assert_eq!(source.next_event(), Some(KeyEvent::AltReleased));
        assert_eq!(source.next_event(), None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_excluded_yubikey_variants() {
        // Should exclude various Yubikey device names
        assert!(KeyboardMonitor::is_excluded("Yubico YubiKey OTP+FIDO"));
        assert!(KeyboardMonitor::is_excluded("Yubico YubiKey OTP+FIDO+CCID"));
        assert!(KeyboardMonitor::is_excluded("YubiKey NEO"));
        assert!(KeyboardMonitor::is_excluded("Yubico Yubikey"));
    }

    #[test]
    fn test_is_excluded_real_keyboards() {
        // Should not exclude real keyboards
        assert!(!KeyboardMonitor::is_excluded("AT Translated Set 2 keyboard"));
        assert!(!KeyboardMonitor::is_excluded("Logitech USB Keyboard"));
        assert!(!KeyboardMonitor::is_excluded("Dell KB216 Wired Keyboard"));
        assert!(!KeyboardMonitor::is_excluded("Microsoft Natural Ergonomic Keyboard"));
    }

    #[test]
    fn test_is_excluded_case_sensitive() {
        // Exclusion is case-sensitive (matching the device names exactly)
        assert!(!KeyboardMonitor::is_excluded("yubico"));
        assert!(!KeyboardMonitor::is_excluded("yubikey"));
        assert!(!KeyboardMonitor::is_excluded("YUBICO"));
    }

    #[test]
    fn test_is_excluded_empty_string() {
        assert!(!KeyboardMonitor::is_excluded(""));
    }
}
