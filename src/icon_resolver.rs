use freedesktop_desktop_entry::DesktopEntry;
use gtk4::gdk_pixbuf::Pixbuf;
use gtk4::gio::prelude::FileExt;
use gtk4::IconLookupFlags;
use gtk4::IconTheme;
use lru::LruCache;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use tracing::{debug, info, warn};

/// Maximum number of entries in the desktop file cache.
/// This prevents unbounded memory growth if many different apps are used.
const DESKTOP_FILE_CACHE_SIZE: usize = 256;

/// Cached XDG application directories plus flatpak locations.
/// Computed once at first access.
static APPLICATION_DIRS: LazyLock<Vec<PathBuf>> = LazyLock::new(|| {
    [
        dirs::data_local_dir().map(|d| d.join("applications")),
        Some(PathBuf::from("/usr/share/applications")),
        Some(PathBuf::from("/usr/local/share/applications")),
        Some(PathBuf::from("/var/lib/flatpak/exports/share/applications")),
        dirs::home_dir().map(|d| d.join(".local/share/flatpak/exports/share/applications")),
    ]
    .into_iter()
    .flatten()
    .collect()
});

/// A pre-built index mapping StartupWMClass values to desktop file paths.
/// This allows resolving icons for apps like Signal where app_id ("signal")
/// doesn't match the desktop file name ("signal-desktop.desktop").
pub type WmClassIndex = Arc<HashMap<String, PathBuf>>;

pub struct IconResolver {
    icon_theme: IconTheme,
    /// LRU cache for desktop file lookups: app_id -> icon_name
    /// Bounded to prevent unbounded memory growth
    desktop_file_cache: LruCache<String, Option<String>>,
    wmclass_index: WmClassIndex, // StartupWMClass -> desktop file path
    icon_size: i32,
}

impl IconResolver {
    /// Create an IconResolver with a pre-built WMClass index
    pub fn with_wmclass_index(icon_size: i32, wmclass_index: WmClassIndex) -> Self {
        let icon_theme = IconTheme::new();
        let cache_size =
            NonZeroUsize::new(DESKTOP_FILE_CACHE_SIZE).expect("cache size must be non-zero");

        IconResolver {
            icon_theme,
            desktop_file_cache: LruCache::new(cache_size),
            wmclass_index,
            icon_size,
        }
    }

    /// Build an index mapping StartupWMClass values to desktop file paths.
    /// This scans all standard XDG application directories at startup.
    pub fn build_wmclass_index() -> WmClassIndex {
        let mut index = HashMap::new();

        for dir in APPLICATION_DIRS.iter() {
            if !dir.exists() {
                continue;
            }

            let entries = match std::fs::read_dir(dir) {
                Ok(entries) => entries,
                Err(_) => continue,
            };

            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "desktop")
                    && let Some((wmclass, desktop_path)) = Self::extract_wmclass(&path) {
                        // Store lowercase key for case-insensitive matching
                        let key = wmclass.to_lowercase();
                        // First match wins (don't overwrite)
                        index.entry(key).or_insert(desktop_path);
                    }
            }
        }

        info!("Built WMClass index with {} entries", index.len());
        Arc::new(index)
    }

    /// Extract StartupWMClass from a desktop file
    fn extract_wmclass(path: &Path) -> Option<(String, PathBuf)> {
        let bytes = std::fs::read(path).ok()?;
        let content = String::from_utf8(bytes).ok()?;
        let entry = DesktopEntry::decode(path, &content).ok()?;

        entry.startup_wm_class().map(|wm| (wm.to_string(), path.to_path_buf()))
    }

    /// Resolve icon for an application ID
    pub fn resolve_icon(&mut self, app_id: Option<&str>) -> Option<Pixbuf> {
        let app_id = app_id?;

        // Check LRU cache first (also promotes to most-recently-used)
        // Clone the cached value to release the mutable borrow before calling load_icon_by_name
        if let Some(cached) = self.desktop_file_cache.get(app_id).cloned() {
            return cached.and_then(|name| self.load_icon_by_name(&name));
        }

        // Try to find desktop file
        let icon_name = self.find_icon_from_desktop_file(app_id);

        // Cache the result in LRU cache (evicts oldest if at capacity)
        self.desktop_file_cache
            .put(app_id.to_string(), icon_name.clone());

        // Load icon if found
        icon_name.and_then(|name| self.load_icon_by_name(&name))
    }

    /// Find desktop file and extract icon name using multiple search strategies
    fn find_icon_from_desktop_file(&self, app_id: &str) -> Option<String> {
        // Try strategies in order of likelihood
        self.try_wmclass_index_lookup(app_id)
            .or_else(|| self.try_exact_desktop_match(app_id))
            .or_else(|| self.try_case_insensitive_match(app_id))
            .or_else(|| self.try_reverse_domain_match(app_id))
            .or_else(|| self.try_common_variations(app_id))
            .or_else(|| {
                debug!("No desktop file found for app_id: {}", app_id);
                None
            })
    }

    /// Try to match reverse-domain app_ids like "org.speedcrunch.speedcrunch"
    /// by extracting the last segment and looking for that desktop file
    fn try_reverse_domain_match(&self, app_id: &str) -> Option<String> {
        // Only try if app_id contains dots (reverse-domain style)
        if !app_id.contains('.') {
            return None;
        }

        // Try the last segment (e.g., "speedcrunch" from "org.speedcrunch.speedcrunch")
        if let Some(last_segment) = app_id.rsplit('.').next() {
            let last_segment_lower = last_segment.to_lowercase();
            for dir in APPLICATION_DIRS.iter() {
                let desktop_file = dir.join(format!("{}.desktop", last_segment_lower));
                if let Some(icon) = self.parse_desktop_file(&desktop_file) {
                    debug!(
                        "Found icon '{}' for app_id '{}' via reverse-domain last segment '{}' in {:?}",
                        icon, app_id, last_segment_lower, desktop_file
                    );
                    return Some(icon);
                }
            }
        }

        None
    }

    /// Try to find icon via the pre-built WMClass index
    fn try_wmclass_index_lookup(&self, app_id: &str) -> Option<String> {
        let app_id_lower = app_id.to_lowercase();
        let desktop_path = self.wmclass_index.get(&app_id_lower)?;
        let icon = self.parse_desktop_file(desktop_path)?;
        debug!(
            "Found icon '{}' for app_id '{}' via StartupWMClass in {:?}",
            icon, app_id, desktop_path
        );
        Some(icon)
    }

    /// Try exact match: app_id.desktop
    fn try_exact_desktop_match(&self, app_id: &str) -> Option<String> {
        for dir in APPLICATION_DIRS.iter() {
            let desktop_file = dir.join(format!("{}.desktop", app_id));
            if let Some(icon) = self.parse_desktop_file(&desktop_file) {
                debug!("Found icon '{}' for app_id '{}' in {:?}", icon, app_id, desktop_file);
                return Some(icon);
            }
        }
        None
    }

    /// Try case-insensitive match by scanning directories
    fn try_case_insensitive_match(&self, app_id: &str) -> Option<String> {
        let target = format!("{}.desktop", app_id.to_lowercase());
        for dir in APPLICATION_DIRS.iter() {
            let entries = match std::fs::read_dir(dir) {
                Ok(entries) => entries,
                Err(_) => continue,
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(filename) = path.file_name()
                    && filename.to_string_lossy().to_lowercase() == target
                        && let Some(icon) = self.parse_desktop_file(&path) {
                            debug!(
                                "Found icon '{}' for app_id '{}' (case-insensitive) in {:?}",
                                icon, app_id, path
                            );
                            return Some(icon);
                        }
            }
        }
        None
    }

    /// Try common variations: remove spaces, replace with dashes, first word only
    fn try_common_variations(&self, app_id: &str) -> Option<String> {
        let variations = [
            app_id.replace(' ', "").to_lowercase(),
            app_id.replace(' ', "-").to_lowercase(),
            app_id.split_whitespace().next().unwrap_or("").to_lowercase(),
        ];

        for variation in variations {
            if variation.is_empty() {
                continue;
            }
            for dir in APPLICATION_DIRS.iter() {
                let desktop_file = dir.join(format!("{}.desktop", variation));
                if let Some(icon) = self.parse_desktop_file(&desktop_file) {
                    debug!(
                        "Found icon '{}' for app_id '{}' using variation '{}' in {:?}",
                        icon, app_id, variation, desktop_file
                    );
                    return Some(icon);
                }
            }
        }
        None
    }

    /// Parse desktop file and extract Icon field
    fn parse_desktop_file(&self, path: &Path) -> Option<String> {
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
            if let Some(path_str) = file.path()
                && let Ok(pixbuf) = Pixbuf::from_file_at_scale(
                    &path_str,
                    self.icon_size,
                    self.icon_size,
                    true,
                ) {
                    return Some(pixbuf);
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

        // Try /usr/share/pixmaps as fallback (many apps install icons here)
        let pixmaps_path = format!("/usr/share/pixmaps/{}.png", icon_name);
        if let Ok(pixbuf) = Pixbuf::from_file_at_scale(
            &pixmaps_path,
            self.icon_size,
            self.icon_size,
            true,
        ) {
            debug!("Found icon in pixmaps: {}", pixmaps_path);
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

    #[test]
    fn test_pixmaps_fallback_path_format() {
        // Test that the pixmaps path is correctly formatted
        let icon_name = "speedcrunch";
        let expected_path = "/usr/share/pixmaps/speedcrunch.png";
        let actual_path = format!("/usr/share/pixmaps/{}.png", icon_name);
        assert_eq!(actual_path, expected_path);
    }

    #[test]
    fn test_reverse_domain_extracts_last_segment() {
        // Test that reverse-domain app_ids extract the correct last segment
        let test_cases = [
            ("org.speedcrunch.speedcrunch", "speedcrunch"),
            ("org.gnome.Calculator", "calculator"),
            ("com.github.user.app", "app"),
            ("io.elementary.code", "code"),
        ];

        for (app_id, expected) in test_cases {
            let last_segment = app_id.rsplit('.').next().unwrap().to_lowercase();
            assert_eq!(last_segment, expected, "Failed for app_id: {}", app_id);
        }
    }

    #[test]
    fn test_reverse_domain_skips_non_dotted_ids() {
        // Non-dotted app_ids should not be processed as reverse-domain
        let simple_ids = ["firefox", "alacritty", "code"];

        for app_id in simple_ids {
            assert!(!app_id.contains('.'), "Test data error: {} contains a dot", app_id);
        }
    }

    /// Integration test that verifies icons in /usr/share/pixmaps can be loaded.
    /// Run with: cargo test -- --ignored
    #[test]
    #[ignore]
    fn test_pixmaps_fallback_integration() {
        use std::path::Path;

        // Find an icon that exists in pixmaps but not in icon theme
        let pixmaps_dir = Path::new("/usr/share/pixmaps");
        if !pixmaps_dir.exists() {
            println!("Skipping test: /usr/share/pixmaps does not exist");
            return;
        }

        // Look for any .png file in pixmaps
        let test_icon = std::fs::read_dir(pixmaps_dir)
            .ok()
            .and_then(|entries| {
                entries
                    .flatten()
                    .find(|e| e.path().extension().map(|ext| ext == "png").unwrap_or(false))
                    .map(|e| e.path().file_stem().unwrap().to_string_lossy().to_string())
            });

        if let Some(icon_name) = test_icon {
            println!("Testing pixmaps fallback with icon: {}", icon_name);

            let wmclass_index = IconResolver::build_wmclass_index();
            let resolver = IconResolver::with_wmclass_index(48, wmclass_index);

            // The icon should be loadable via the pixmaps fallback
            let pixbuf = resolver.load_icon_by_name(&icon_name);
            assert!(
                pixbuf.is_some(),
                "Expected to load icon '{}' from /usr/share/pixmaps/{}.png",
                icon_name,
                icon_name
            );
        } else {
            println!("Skipping test: no PNG icons found in /usr/share/pixmaps");
        }
    }

    #[test]
    fn test_lru_cache_size_constant() {
        // Verify the cache size constant is reasonable
        assert!(DESKTOP_FILE_CACHE_SIZE > 0);
        assert!(DESKTOP_FILE_CACHE_SIZE >= 64); // At least handle a typical number of apps
        assert!(DESKTOP_FILE_CACHE_SIZE <= 1024); // But not excessively large
    }

    #[test]
    fn test_lru_cache_eviction() {
        use std::num::NonZeroUsize;

        // Create a small LRU cache to test eviction
        let mut cache: LruCache<String, Option<String>> =
            LruCache::new(NonZeroUsize::new(2).unwrap());

        // Add two entries
        cache.put("app1".to_string(), Some("icon1".to_string()));
        cache.put("app2".to_string(), Some("icon2".to_string()));

        // Both should be present
        assert!(cache.get("app1").is_some());
        assert!(cache.get("app2").is_some());

        // Add a third entry - should evict the least recently used (app1, since we just accessed app2)
        cache.put("app3".to_string(), Some("icon3".to_string()));

        // app1 should be evicted (it was accessed before app2)
        assert!(cache.get("app1").is_none());
        assert!(cache.get("app2").is_some());
        assert!(cache.get("app3").is_some());
    }

    #[test]
    fn test_lru_cache_get_promotes_entry() {
        use std::num::NonZeroUsize;

        let mut cache: LruCache<String, Option<String>> =
            LruCache::new(NonZeroUsize::new(2).unwrap());

        cache.put("app1".to_string(), Some("icon1".to_string()));
        cache.put("app2".to_string(), Some("icon2".to_string()));

        // Access app1 to promote it to most recently used
        let _ = cache.get("app1");

        // Add app3 - should evict app2 (now least recently used)
        cache.put("app3".to_string(), Some("icon3".to_string()));

        // app2 should be evicted, app1 should still be present
        assert!(cache.get("app1").is_some());
        assert!(cache.get("app2").is_none());
        assert!(cache.get("app3").is_some());
    }
}
