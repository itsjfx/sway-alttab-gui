use freedesktop_desktop_entry::DesktopEntry;
use gtk4::gdk_pixbuf::Pixbuf;
use gtk4::gio::prelude::FileExt;
use gtk4::IconLookupFlags;
use gtk4::IconTheme;
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{debug, warn};

pub struct IconResolver {
    icon_theme: IconTheme,
    desktop_file_cache: HashMap<String, Option<String>>, // app_id -> icon_name
    icon_size: i32,
}

impl IconResolver {
    pub fn new(icon_size: i32) -> Self {
        let icon_theme = IconTheme::new();

        IconResolver {
            icon_theme,
            desktop_file_cache: HashMap::new(),
            icon_size,
        }
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
        // Standard desktop file locations
        let search_dirs = vec![
            dirs::data_local_dir()?.join("applications"),
            PathBuf::from("/usr/share/applications"),
            PathBuf::from("/usr/local/share/applications"),
        ];

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

        warn!("No desktop file found for app_id: {}", app_id);
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

fn dirs() -> Option<()> {
    None
}

// Minimal implementation of dirs functions if the dirs crate is not available
mod dirs {
    use std::path::PathBuf;

    pub fn data_local_dir() -> Option<PathBuf> {
        std::env::var("XDG_DATA_HOME")
            .ok()
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var("HOME")
                    .ok()
                    .map(|h| PathBuf::from(h).join(".local/share"))
            })
    }
}
