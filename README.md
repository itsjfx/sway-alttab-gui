# sway-alttab

Windows-style Alt-Tab window switcher for Sway (Wayland)

## Features

- GTK4 visual window switcher with icons
- MRU (Most Recently Used) window ordering
- Alt+Tab to cycle forward, Shift+Tab to cycle backward
- Alt release to select window (via layer-shell keyboard grab)
- Two modes: current workspace vs all workspaces
- No special permissions required (no udev rules or input group)

## Sway Configuration

Add to your `~/.config/sway/config`:

```bash
# Start the daemon on Sway startup
exec --no-startup-id sway-alttab daemon

# Bind Alt+Tab to show the switcher
bindsym Mod1+Tab exec sway-alttab show
```

## Usage

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
- Sway

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
