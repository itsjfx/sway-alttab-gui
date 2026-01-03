# sway-alttab

Windows-style Alt-Tab window switcher for Sway (Wayland compositor).

## Overview

This daemon provides a familiar Alt+Tab window switching experience for Sway, similar to Windows. It runs in the background, monitors keyboard input, tracks window focus history, and allows you to cycle through windows in MRU (Most Recently Used) order.

## Current Status

**Phase 1: Core Functionality (No UI)**

The current implementation focuses on the core Alt+Tab behavior without a graphical UI. Instead, it prints the window list to stderr, allowing you to verify the behavior before adding the GTK UI.

## Features

- ✅ Background daemon with async Tokio runtime
- ✅ Keyboard input monitoring via evdev (Alt, Tab, Shift detection)
- ✅ Window tracking via Sway IPC
- ✅ MRU (Most Recently Used) window ordering
- ✅ Two modes: current workspace vs all workspaces
- ✅ Alt+Tab to cycle forward, Alt+Shift+Tab to cycle backward
- ✅ Alt release to select window
- ⏳ GTK4 UI (planned for Phase 2)

## Building

```bash
cargo build --release
```

## Running

### Prerequisites

The daemon needs permission to read keyboard events from `/dev/input/event*` devices. Add your user to the `input` group:

```bash
sudo usermod -aG input $USER
```

Then **log out and log back in** for the group change to take effect.

### Start the Daemon

```bash
# Show windows from current workspace only (default)
./target/release/sway-alttab

# Show windows from all workspaces
./target/release/sway-alttab --mode all

# Enable verbose logging
./target/release/sway-alttab --verbose
```

### Usage

1. Press **Alt+Tab** to start window switching
   - The window list will be printed to stderr
   - The second window (index 1) will be selected by default

2. While holding **Alt**:
   - Press **Tab** to cycle forward through windows
   - Press **Shift+Tab** to cycle backward through windows

3. Release **Alt** to focus the selected window

### Example Output

```
=== Window Switcher ===
    [94489317269056] Alacritty - ~/projects
>>> [94489320495616] firefox - Mozilla Firefox
    [94489318582592] Code - ~/projects/sway-alttab
=======================

SELECTING: Mozilla Firefox (ID: 94489320495616)
```

## Configuration

All configuration is done via CLI flags (no config file):

```bash
sway-alttab [OPTIONS]

Options:
  -m, --mode <MODE>  Workspace filtering mode [default: current] [possible values: current, all]
  -v, --verbose      Enable verbose logging
  -h, --help         Print help
```

## Architecture

- **main.rs**: Entry point, initializes Tokio runtime
- **daemon.rs**: Main event loop, state machine (Idle/Switching)
- **window_manager.rs**: Tracks windows via Sway IPC, maintains MRU list
- **keyboard_monitor.rs**: Reads raw keyboard events via evdev
- **config.rs**: CLI configuration with clap

### Threading Model

- Tokio async runtime for daemon event loop
- Separate async task for keyboard monitoring
- Separate async task for Sway IPC event monitoring

## Next Steps (Phase 2)

1. Add GTK4 UI to display window switcher visually
2. Implement icon resolution from desktop files
3. Add proper window previews/thumbnails
4. Create systemd user service for auto-start

## Troubleshooting

### Permission Denied for /dev/input

```
ERROR: Cannot access keyboard devices.
This daemon needs permission to read /dev/input/event* devices.

To fix this, add your user to the 'input' group:
  sudo usermod -aG input $USER
  (then log out and log back in)
```

### No Windows Found

Make sure you're running the daemon inside a Sway session and have some windows open.

## License

MIT
