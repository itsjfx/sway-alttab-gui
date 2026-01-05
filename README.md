# sway-alttab

Windows-style Alt-Tab window switcher for Sway (Wayland compositor).

## Features

- GTK4 visual window switcher with icons
- MRU (Most Recently Used) window ordering
- Alt+Tab to cycle forward, Shift+Tab to cycle backward
- Alt release to select window (via layer-shell keyboard grab)
- Two modes: current workspace vs all workspaces
- No special permissions required (no udev rules or input group)

## Dependencies

### Build Dependencies

```bash
# Arch Linux
sudo pacman -S gtk4 gtk4-layer-shell

# Other distros: install gtk4 and gtk4-layer-shell development packages
```

### Runtime Dependencies

- `gtk4`
- `gtk4-layer-shell`
- Sway (or compatible Wayland compositor with layer-shell support)

## Building

```bash
cargo build --release
```

## Installation

```bash
# Copy binary to your PATH
cp target/release/sway-alttab ~/bin/

# Or install via cargo
cargo install --path .
```

## Sway Configuration

Add to your `~/.config/sway/config`:

```bash
# Start the daemon on Sway startup
exec --no-startup-id sway-alttab daemon

# Bind Alt+Tab to show the switcher
bindsym Mod1+Tab exec sway-alttab show
```

The layer-shell window grabs keyboard exclusively when visible, so:
- **Tab** cycles forward
- **Shift+Tab** cycles backward
- **Alt release** selects the window
- **Escape** cancels
- **Enter** also selects

## Usage

### CLI Commands

```bash
sway-alttab [OPTIONS] [COMMAND]

Commands:
  daemon    Run as daemon (default)
  show      Show the window switcher
  next      Cycle to next window
  prev      Cycle to previous window
  select    Select current window
  cancel    Cancel without selecting
  status    Query daemon status
  shutdown  Shutdown the daemon

Options:
  -m, --mode <MODE>  Workspace filter [default: current] [values: current, all]
  -v, --verbose      Enable verbose logging
```

### Examples

```bash
# Start daemon (usually via sway config)
sway-alttab daemon

# Start daemon showing all workspaces
sway-alttab --mode all daemon

# Manually trigger switcher (usually via keybinding)
sway-alttab show
```

## Architecture

```
[Sway keybinding] → [CLI: sway-alttab show] → [Unix socket] → [Daemon] → [GTK UI]
                                                    ↑
[GTK keyboard events] → [CLI: sway-alttab next/select] ─┘
```

- **Daemon**: Runs GTK application, listens on Unix socket for commands
- **CLI**: Sends commands to daemon via socket
- **Layer-shell**: GTK window grabs keyboard exclusively when visible
- **IPC**: Simple text protocol over Unix socket at `$XDG_RUNTIME_DIR/sway-alttab.sock`

### Key Files

- `main.rs` - Entry point, CLI dispatch
- `daemon.rs` - Event loop, window switching state machine
- `ui.rs` - GTK4 layer-shell window with keyboard handling
- `socket_server.rs` / `socket_client.rs` - Unix socket IPC
- `window_manager.rs` - Sway IPC, MRU tracking
- `icon_resolver.rs` - Desktop file icon resolution

## License

MIT
