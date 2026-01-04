use crate::icon_resolver::{IconResolver, WmClassIndex};
use crate::window_manager::WindowInfo;
use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, Box as GtkBox, Image, Label, Orientation,
    Widget,
};
use tracing::{debug, info};

const ICON_SIZE: i32 = 64;
const WINDOW_PADDING: i32 = 20;
const TILE_PADDING: i32 = 10;
const MAX_TITLE_LENGTH: usize = 20;

pub struct SwitcherWindow {
    window: ApplicationWindow,
    container: GtkBox,
    windows: Vec<WindowInfo>,
    current_index: usize,
    tiles: Vec<Widget>,
}

impl SwitcherWindow {
    pub fn new(app: &Application) -> Self {
        let window = ApplicationWindow::builder()
            .application(app)
            .title("Window Switcher")
            .default_width(600)
            .default_height(150)
            .decorated(false)
            .resizable(false)
            .build();

        // Create horizontal container for window tiles
        let container = GtkBox::new(Orientation::Horizontal, TILE_PADDING);
        container.set_margin_start(WINDOW_PADDING);
        container.set_margin_end(WINDOW_PADDING);
        container.set_margin_top(WINDOW_PADDING);
        container.set_margin_bottom(WINDOW_PADDING);
        container.set_halign(gtk4::Align::Center);
        container.set_valign(gtk4::Align::Center);

        window.set_child(Some(&container));

        SwitcherWindow {
            window,
            container,
            windows: Vec::new(),
            current_index: 0,
            tiles: Vec::new(),
        }
    }

    /// Show the window switcher with a list of windows
    pub fn show(&mut self, windows: Vec<WindowInfo>, initial_index: usize, wmclass_index: WmClassIndex) {
        self.windows = windows;
        self.current_index = initial_index.min(self.windows.len().saturating_sub(1));

        info!("Building UI for {} windows", self.windows.len());

        // Clear existing tiles
        while let Some(child) = self.container.first_child() {
            self.container.remove(&child);
        }
        self.tiles.clear();

        // Create icon resolver with the pre-built WMClass index
        let mut icon_resolver = IconResolver::with_wmclass_index(ICON_SIZE, wmclass_index);

        // Create tiles for each window
        for (i, window) in self.windows.iter().enumerate() {
            let tile = self.create_window_tile(window, &mut icon_resolver);

            // Highlight the selected tile
            if i == self.current_index {
                self.highlight_tile(&tile);
            }

            self.container.append(&tile);
            self.tiles.push(tile);
        }

        info!("Presenting window...");
        self.window.set_visible(true);
        self.window.present();
        info!("Window presented, is_visible={}", self.window.is_visible());
    }

    fn create_window_tile(&self, window: &WindowInfo, icon_resolver: &mut IconResolver) -> Widget {
        let vbox = GtkBox::new(Orientation::Vertical, 5);
        vbox.set_margin_start(TILE_PADDING);
        vbox.set_margin_end(TILE_PADDING);

        // Add icon - try app_id first, then window_class as fallback
        let icon_found = if let Some(pixbuf) = icon_resolver.resolve_icon(window.app_id.as_deref()) {
            let icon = Image::from_pixbuf(Some(&pixbuf));
            icon.set_pixel_size(ICON_SIZE);
            vbox.append(&icon);
            true
        } else if let Some(pixbuf) = icon_resolver.resolve_icon(window.window_class.as_deref()) {
            let icon = Image::from_pixbuf(Some(&pixbuf));
            icon.set_pixel_size(ICON_SIZE);
            vbox.append(&icon);
            true
        } else if let Some(fallback) = icon_resolver.get_fallback_icon() {
            let icon = Image::from_pixbuf(Some(&fallback));
            icon.set_pixel_size(ICON_SIZE);
            vbox.append(&icon);
            true
        } else {
            false
        };

        if !icon_found {
            // Absolute fallback: just a placeholder label
            let placeholder = Label::new(Some("□"));
            placeholder.set_width_request(ICON_SIZE);
            placeholder.set_height_request(ICON_SIZE);
            vbox.append(&placeholder);
        }

        // Add title
        let title = truncate_string(&window.title, MAX_TITLE_LENGTH);
        let label = Label::new(Some(&title));
        label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        label.set_max_width_chars(MAX_TITLE_LENGTH as i32);
        vbox.append(&label);

        vbox.upcast()
    }

    fn highlight_tile(&self, tile: &Widget) {
        // Add CSS class for highlighting
        tile.add_css_class("selected");
    }

    fn unhighlight_tile(&self, tile: &Widget) {
        tile.remove_css_class("selected");
    }

    /// Cycle through windows in the given direction
    /// forward=true cycles to the next window, forward=false cycles to the previous
    fn cycle(&mut self, forward: bool) {
        if self.windows.is_empty() {
            return;
        }

        // Remove highlight from current tile
        if let Some(current_tile) = self.tiles.get(self.current_index) {
            self.unhighlight_tile(current_tile);
        }

        // Update index with wraparound
        let len = self.windows.len();
        self.current_index = if forward {
            (self.current_index + 1) % len
        } else if self.current_index == 0 {
            len - 1
        } else {
            self.current_index - 1
        };

        // Highlight new selection
        if let Some(new_tile) = self.tiles.get(self.current_index) {
            self.highlight_tile(new_tile);
        }

        debug!("Cycled to window {}: {:?}", self.current_index, self.windows[self.current_index].title);
    }

    /// Cycle to next window
    pub fn cycle_next(&mut self) {
        self.cycle(true);
    }

    /// Cycle to previous window
    pub fn cycle_prev(&mut self) {
        self.cycle(false);
    }

    /// Close the window switcher
    pub fn close(&self) {
        info!("Hiding window (not closing, so GTK app stays alive)");
        self.window.set_visible(false);
    }
}

fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

/// Setup CSS styling for the window switcher
pub fn setup_css() {
    let provider = gtk4::CssProvider::new();
    // In GTK4, use load_from_data instead of load_from_string
    provider.load_from_data(
        r#"
        window {
            background-color: rgba(30, 30, 30, 0.95);
            border-radius: 10px;
            border: 2px solid rgba(100, 100, 100, 0.5);
        }

        box {
            background-color: transparent;
        }

        label {
            color: #ffffff;
            font-size: 12px;
        }

        .selected {
            background-color: rgba(70, 130, 180, 0.7);
            border-radius: 8px;
        }
        "#,
    );

    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().expect("Failed to get default display"),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}
