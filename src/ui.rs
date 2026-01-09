use crate::icon_resolver::{IconResolver, WmClassIndex};
use crate::ipc::InputCommand;
use crate::window_manager::WindowInfo;
use gtk4::gdk::Key;
use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, Box as GtkBox, EventControllerKey, Image, Label, Orientation,
    Widget,
};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

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

/// Sender type for input commands to daemon
pub type InputSender = mpsc::UnboundedSender<InputCommand>;

impl SwitcherWindow {
    pub fn new(app: &Application, input_tx: InputSender) -> Self {
        let window = ApplicationWindow::builder()
            .application(app)
            .title("Window Switcher")
            .default_width(600)
            .default_height(150)
            .decorated(false)
            .resizable(false)
            .build();

        // Initialize layer shell
        window.init_layer_shell();
        window.set_layer(Layer::Overlay);
        window.set_keyboard_mode(KeyboardMode::Exclusive);

        // Center the window
        window.set_anchor(Edge::Top, false);
        window.set_anchor(Edge::Bottom, false);
        window.set_anchor(Edge::Left, false);
        window.set_anchor(Edge::Right, false);

        // Setup keyboard event controller
        let key_controller = EventControllerKey::new();
        let tx_pressed = input_tx.clone();
        key_controller.connect_key_pressed(move |_controller, keyval, _keycode, state| {
            debug!("Key pressed: {:?}, state: {:?}", keyval, state);

            match keyval {
                Key::Tab => {
                    // Check if Shift is held
                    if state.contains(gtk4::gdk::ModifierType::SHIFT_MASK) {
                        debug!("Shift+Tab pressed, sending prev");
                        send_input_command(&tx_pressed, InputCommand::Prev);
                    } else {
                        debug!("Tab pressed, sending next");
                        send_input_command(&tx_pressed, InputCommand::Next);
                    }
                    gtk4::glib::Propagation::Stop
                }
                Key::ISO_Left_Tab => {
                    // Shift+Tab often generates ISO_Left_Tab
                    debug!("ISO_Left_Tab (Shift+Tab) pressed, sending prev");
                    send_input_command(&tx_pressed, InputCommand::Prev);
                    gtk4::glib::Propagation::Stop
                }
                Key::Escape => {
                    debug!("Escape pressed, sending cancel");
                    send_input_command(&tx_pressed, InputCommand::Cancel);
                    gtk4::glib::Propagation::Stop
                }
                Key::Return | Key::KP_Enter => {
                    debug!("Enter pressed, sending select");
                    send_input_command(&tx_pressed, InputCommand::Select);
                    gtk4::glib::Propagation::Stop
                }
                _ => gtk4::glib::Propagation::Proceed,
            }
        });

        // Detect Alt release
        let tx_released = input_tx;
        key_controller.connect_key_released(move |_controller, keyval, _keycode, _state| {
            debug!("Key released: {:?}", keyval);

            match keyval {
                Key::Alt_L | Key::Alt_R => {
                    debug!("Alt released, sending select");
                    send_input_command(&tx_released, InputCommand::Select);
                }
                _ => {}
            }
        });

        window.add_controller(key_controller);

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

    /// Pre-realize the window to avoid slow first show.
    /// This creates the Wayland surface and layer shell setup without displaying anything.
    pub fn warm_up(&self) {
        // Realize creates the underlying GDK surface without showing
        gtk4::prelude::WidgetExt::realize(&self.window);
        info!("Window pre-realized for faster first show");
    }

    /// Show the window switcher with a list of windows
    pub fn show(
        &mut self,
        windows: Vec<WindowInfo>,
        initial_index: usize,
        wmclass_index: WmClassIndex,
    ) {
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

        // Add icon - try app_id first, then window_class, then fallback
        let pixbuf = icon_resolver
            .resolve_icon(window.app_id.as_deref())
            .or_else(|| icon_resolver.resolve_icon(window.window_class.as_deref()))
            .or_else(|| icon_resolver.get_fallback_icon());

        if let Some(pb) = pixbuf {
            let icon = Image::from_pixbuf(Some(&pb));
            icon.set_pixel_size(ICON_SIZE);
            vbox.append(&icon);
        } else {
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

    /// Set the selection to a specific index
    /// (daemon owns the authoritative selection state, UI just reflects it)
    pub fn set_selection(&mut self, new_index: usize) {
        if self.windows.is_empty() || new_index >= self.windows.len() {
            return;
        }

        // Remove highlight from current tile
        if let Some(current_tile) = self.tiles.get(self.current_index) {
            self.unhighlight_tile(current_tile);
        }

        // Update index
        self.current_index = new_index;

        // Highlight new selection
        if let Some(new_tile) = self.tiles.get(self.current_index) {
            self.highlight_tile(new_tile);
        }

        debug!(
            "Selection updated to window {}: {:?}",
            self.current_index, self.windows[self.current_index].title
        );
    }

    /// Close the window switcher
    pub fn close(&self) {
        info!("Hiding window (not closing, so GTK app stays alive)");
        self.window.set_visible(false);
    }
}

/// Send an input command to the daemon via channel
fn send_input_command(tx: &InputSender, cmd: InputCommand) {
    if let Err(e) = tx.send(cmd) {
        warn!("Failed to send input command: {}", e);
    }
}

fn truncate_string(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        s.to_string()
    } else {
        format!(
            "{}...",
            s.chars()
                .take(max_chars.saturating_sub(3))
                .collect::<String>()
        )
    }
}

/// Setup CSS styling for the window switcher
pub fn setup_css() {
    let provider = gtk4::CssProvider::new();
    // Minimal CSS - inherit colors from the user's GTK theme
    provider.load_from_data(
        r#"
        window {
            border-radius: 10px;
        }

        .selected {
            background-color: alpha(@theme_selected_bg_color, 0.7);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_string_short() {
        assert_eq!(truncate_string("hello", 20), "hello");
    }

    #[test]
    fn test_truncate_string_exact_length() {
        let s = "12345678901234567890"; // exactly 20 chars
        assert_eq!(truncate_string(s, 20), s);
    }

    #[test]
    fn test_truncate_string_long() {
        let s = "this is a very long window title";
        let result = truncate_string(s, 10);
        assert_eq!(result, "this is...");
        assert_eq!(result.chars().count(), 10);
    }

    #[test]
    fn test_truncate_string_unicode() {
        // Unicode characters should count as 1 char each
        let s = "日本語テストタイトル";
        let result = truncate_string(s, 5);
        assert_eq!(result, "日本...");
        assert_eq!(result.chars().count(), 5);
    }

    #[test]
    fn test_truncate_string_empty() {
        assert_eq!(truncate_string("", 20), "");
    }

    #[test]
    fn test_truncate_string_one_char() {
        assert_eq!(truncate_string("a", 20), "a");
    }

    #[test]
    fn test_truncate_string_exactly_max_plus_one() {
        let s = "123456789012345678901"; // 21 chars
        let result = truncate_string(s, 20);
        assert_eq!(result, "12345678901234567...");
        assert_eq!(result.chars().count(), 20);
    }

    #[test]
    fn test_truncate_string_very_short_max() {
        // Edge case: max is 3, so only ellipsis fits
        let result = truncate_string("hello", 3);
        assert_eq!(result, "...");
    }

    #[test]
    fn test_truncate_string_max_less_than_3() {
        // Edge case: max is less than ellipsis length
        // saturating_sub prevents underflow
        let result = truncate_string("hello", 2);
        assert_eq!(result, "...");
    }
}
