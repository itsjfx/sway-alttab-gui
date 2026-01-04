use freedesktop_desktop_entry::DesktopEntry;
use gtk4::gdk_pixbuf::Pixbuf;
use gtk4::gio::prelude::FileExt;
use gtk4::IconLookupFlags;
use gtk4::IconTheme;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// A pre-built index mapping StartupWMClass values to desktop file paths.
/// This allows resolving icons for apps like Signal where app_id ("signal")
/// doesn't match the desktop file name ("signal-desktop.desktop").
pub type WmClassIndex = Arc<HashMap<String, PathBuf>>;

pub struct IconResolver {
    icon_theme: IconTheme,
    desktop_file_cache: HashMap<String, Option<String>>, // app_id -> icon_name
    wmclass_index: WmClassIndex,                          // StartupWMClass -> desktop file path
    icon_size: i32,
}

impl IconResolver {
    /// Create an IconResolver with a pre-built WMClass index
    pub fn with_wmclass_index(icon_size: i32, wmclass_index: WmClassIndex) -> Self {
        let icon_theme = IconTheme::new();

        IconResolver {
            icon_theme,
            desktop_file_cache: HashMap::new(),
            wmclass_index,
            icon_size,
        }
    }

    /// Build an index mapping StartupWMClass values to desktop file paths.
    /// This scans all standard XDG application directories at startup.
    pub fn build_wmclass_index() -> WmClassIndex {
        let mut index = HashMap::new();

        // Standard XDG application directories plus flatpak
        let search_dirs: Vec<PathBuf> = [
            dirs::data_local_dir().map(|d| d.join("applications")),
            Some(PathBuf::from("/usr/share/applications")),
            Some(PathBuf::from("/usr/local/share/applications")),
            Some(PathBuf::from("/var/lib/flatpak/exports/share/applications")),
            dirs::home_dir().map(|d| d.join(".local/share/flatpak/exports/share/applications")),
        ]
        .into_iter()
        .flatten()
        .collect();

        for dir in &search_dirs {
            if !dir.exists() {
                continue;
            }

            let entries = match std::fs::read_dir(dir) {
                Ok(entries) => entries,
                Err(_) => continue,
            };

            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "desktop").unwrap_or(false) {
                    if let Some((wmclass, desktop_path)) = Self::extract_wmclass(&path) {
                        // Store lowercase key for case-insensitive matching
                        let key = wmclass.to_lowercase();
                        // First match wins (don't overwrite)
                        index.entry(key).or_insert(desktop_path);
                    }
                }
            }
        }

        info!("Built WMClass index with {} entries", index.len());
        Arc::new(index)
    }

    /// Extract StartupWMClass from a desktop file
    fn extract_wmclass(path: &PathBuf) -> Option<(String, PathBuf)> {
        let bytes = std::fs::read(path).ok()?;
        let content = String::from_utf8(bytes).ok()?;
        let entry = DesktopEntry::decode(path, &content).ok()?;

        entry.startup_wm_class().map(|wm| (wm.to_string(), path.clone()))
    }

    /// Resolve icon for an application ID
    pub fn resolve_icon(&mut self, app_id: Option<&str>) -> Option<Pixbuf> {
        let app_id = app_id?;

        // Check cache first
        if let Some(cached) = self.desktop_file_cache.get(app_id) {
            if let Some(icon_name) = cached {
                return self.load_icon_by_name(icon_name);
            } else {
                return None;
            }
        }

        // Try to find desktop file
        let icon_name = self.find_icon_from_desktop_file(app_id);

        // Cache the result
        self.desktop_file_cache.insert(app_id.to_string(), icon_name.clone());

        // Load icon if found
        icon_name.and_then(|name| self.load_icon_by_name(&name))
    }

    /// Find desktop file and extract icon name
    fn find_icon_from_desktop_file(&self, app_id: &str) -> Option<String> {
        // Check wmclass_index first (case-insensitive)
        let app_id_lower = app_id.to_lowercase();
        if let Some(desktop_path) = self.wmclass_index.get(&app_id_lower) {
            if let Some(icon) = self.parse_desktop_file(desktop_path) {
                debug!(
                    "Found icon '{}' for app_id '{}' via StartupWMClass in {:?}",
                    icon, app_id, desktop_path
                );
                return Some(icon);
            }
        }

        // Standard desktop file locations
        let search_dirs: Vec<PathBuf> = [
            dirs::data_local_dir().map(|d| d.join("applications")),
            Some(PathBuf::from("/usr/share/applications")),
            Some(PathBuf::from("/usr/local/share/applications")),
        ]
        .into_iter()
        .flatten()
        .collect();

        // Try exact match first: app_id.desktop
        for dir in &search_dirs {
            let desktop_file = dir.join(format!("{}.desktop", app_id));
            if let Some(icon) = self.parse_desktop_file(&desktop_file) {
                debug!("Found icon '{}' for app_id '{}' in {:?}", icon, app_id, desktop_file);
                return Some(icon);
            }
        }

        // Try case-insensitive match
        for dir in &search_dirs {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if let Some(filename) = path.file_name() {
                        let filename_str = filename.to_string_lossy();
                        if filename_str.to_lowercase() == format!("{}.desktop", app_id.to_lowercase()) {
                            if let Some(icon) = self.parse_desktop_file(&path) {
                                debug!("Found icon '{}' for app_id '{}' (case-insensitive) in {:?}", icon, app_id, path);
                                return Some(icon);
                            }
                        }
                    }
                }
            }
        }

        // Try common variations: remove spaces, convert to lowercase
        let variations = vec![
            app_id.replace(" ", "").to_lowercase(),
            app_id.replace(" ", "-").to_lowercase(),
            app_id.split_whitespace().next().unwrap_or("").to_lowercase(),
        ];

        for variation in variations {
            if variation.is_empty() {
                continue;
            }
            for dir in &search_dirs {
                let desktop_file = dir.join(format!("{}.desktop", variation));
                if let Some(icon) = self.parse_desktop_file(&desktop_file) {
                    debug!("Found icon '{}' for app_id '{}' using variation '{}' in {:?}", icon, app_id, variation, desktop_file);
                    return Some(icon);
                }
            }
        }

        debug!("No desktop file found for app_id: {}", app_id);
        None
    }

    /// Parse desktop file and extract Icon field
    fn parse_desktop_file(&self, path: &PathBuf) -> Option<String> {
        let bytes = std::fs::read(path).ok()?;
        let content = String::from_utf8(bytes).ok()?;
        let entry = DesktopEntry::decode(path, &content).ok()?;

        entry.icon().map(|s| s.to_string())
    }

    /// Load icon by name using GTK IconTheme
    fn load_icon_by_name(&self, icon_name: &str) -> Option<Pixbuf> {
        // Try to load from icon theme
        let paintable = self.icon_theme.lookup_icon(
            icon_name,
            &[], // No fallbacks
            self.icon_size,
            1, // scale
            gtk4::TextDirection::None,
            IconLookupFlags::empty(),
        );

        // Try to get the file and load as pixbuf
        if let Some(file) = paintable.file() {
            // In GTK4, get path from URI
            if let Some(path_str) = file.path() {
                if let Ok(pixbuf) = Pixbuf::from_file_at_scale(
                    &path_str,
                    self.icon_size,
                    self.icon_size,
                    true,
                ) {
                    return Some(pixbuf);
                }
            }
        }

        // Try loading directly as a file path (absolute icon paths)
        if let Ok(pixbuf) = Pixbuf::from_file_at_scale(
            icon_name,
            self.icon_size,
            self.icon_size,
            true,
        ) {
            return Some(pixbuf);
        }

        warn!("Failed to load icon: {}", icon_name);
        None
    }

    /// Get a fallback icon (generic application icon)
    pub fn get_fallback_icon(&self) -> Option<Pixbuf> {
        self.load_icon_by_name("application-x-executable")
            .or_else(|| self.load_icon_by_name("application-default-icon"))
            .or_else(|| self.load_icon_by_name("gtk-missing-image"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_wmclass_from_content() {
        // Simulate parsing a Signal-like desktop entry
        let content = r#"[Desktop Entry]
Type=Application
Name=Signal
Icon=signal-desktop
StartupWMClass=signal
Exec=signal-desktop
"#;
        let path = PathBuf::from("/tmp/signal-desktop.desktop");
        let entry = DesktopEntry::decode(&path, content).unwrap();

        assert_eq!(entry.startup_wm_class(), Some("signal"));
        assert_eq!(entry.icon(), Some("signal-desktop"));
    }

    #[test]
    fn test_wmclass_index_case_insensitive_lookup() {
        let mut index = HashMap::new();
        index.insert(
            "signal".to_string(),
            PathBuf::from("/usr/share/applications/signal-desktop.desktop"),
        );

        // All case variations should match when lowercased
        assert!(index.contains_key(&"signal".to_lowercase()));
        assert!(index.contains_key(&"SIGNAL".to_lowercase()));
        assert!(index.contains_key(&"Signal".to_lowercase()));
    }

    #[test]
    fn test_wmclass_index_multiple_entries() {
        let entries = vec![
            ("signal", "/usr/share/applications/signal-desktop.desktop"),
            ("discord", "/usr/share/applications/discord.desktop"),
            ("code", "/usr/share/applications/code.desktop"),
            ("Alacritty", "/usr/share/applications/Alacritty.desktop"),
        ];

        let index: HashMap<String, PathBuf> = entries
            .into_iter()
            .map(|(k, v)| (k.to_lowercase(), PathBuf::from(v)))
            .collect();

        assert_eq!(index.len(), 4);
        assert!(index.get("signal").unwrap().to_string_lossy().contains("signal-desktop"));
        assert!(index.get("discord").unwrap().to_string_lossy().contains("discord"));
        assert!(index.get("alacritty").unwrap().to_string_lossy().contains("Alacritty"));
    }

    #[test]
    fn test_empty_index_returns_none() {
        let index: HashMap<String, PathBuf> = HashMap::new();

        // With empty index, lookup returns None
        assert!(index.get("signal").is_none());
        assert!(index.get("firefox").is_none());
    }

    #[test]
    fn test_first_entry_wins_for_duplicate_wmclass() {
        let mut index = HashMap::new();

        // First entry wins
        index.entry("signal".to_string())
            .or_insert(PathBuf::from("/usr/share/applications/signal-desktop.desktop"));

        // Second entry with same key should not overwrite
        index.entry("signal".to_string())
            .or_insert(PathBuf::from("/some/other/path.desktop"));

        // Should still have the first path
        assert!(index.get("signal").unwrap().to_string_lossy().contains("signal-desktop"));
    }

    /// Integration test that actually scans the system's desktop files.
    /// Run with: cargo test -- --ignored
    #[test]
    #[ignore]
    fn test_build_wmclass_index_integration() {
        let index = IconResolver::build_wmclass_index();

        // Should find at least some entries on a typical Linux system
        assert!(!index.is_empty(), "Expected to find at least one desktop file with StartupWMClass");

        // Print first few entries for debugging
        println!("Found {} WMClass entries:", index.len());
        for (key, path) in index.iter().take(10) {
            println!("  '{}' -> {:?}", key, path);
        }
    }
}
