# sway-alttab

Windows-style Alt-Tab window switcher for Sway (Wayland)

If you used [sagb/alttab](https://github.com/sagb/alttab) on X11, then this is for you.

## Features

- GTK4 visual window switcher with icons
- MRU (Most Recently Used) window ordering
- Alt+Tab to cycle forward, Shift+Tab to cycle backward
- Alt release to select window
- Can display windows from current workspace or all workspaces
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
