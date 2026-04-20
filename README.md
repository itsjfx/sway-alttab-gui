# sway-alttab-gui

Windows-style Alt-Tab window switcher for Sway (Wayland)

If you used [sagb/alttab](https://github.com/sagb/alttab) on X11, then this is for you.

## Features

* GTK4 visual window switcher with icons
* MRU (Most Recently Used) window ordering
* Customisable Alt key, use Ctrl or Super/Win instead
* Alt+Tab to cycle forward, Shift+Tab to cycle backward
* Alt release to select window
* Can display windows from current workspace or all workspaces
* No special permissions required (no udev rules or input group)

<img width="1037" height="229" alt="2026-01-26T21:46:08,249171018+11:00" src="https://github.com/user-attachments/assets/8019c303-343e-4369-b081-7ed81d3f4ef1" />

## Installation

### Arch Linux

If you're using Arch Linux, you can install from the AUR:

* [sway-alttab-gui-bin](https://aur.archlinux.org/packages/sway-alttab-gui-bin)
* No DIY build from source package yet (can be added and maintained on request)

### All distributions

1. Install required runtime dependencies
    1. `gtk4`
    2. `gtk4-layer-shell`
    3. Sway (duh)
2. Then choose one of the following methods to get the binary
    1. [Download the latest binary from GitHub releases](https://github.com/itsjfx/sway-alttab-gui/releases)
    2. Build and install from source with `cargo build --git=https://github.com/itsjfx/sway-alttab-gui`
    3. Or clone the repository yourself, and build the binary with `cargo build --release`

## Configuration

Add to your Sway configuration:

```bash
exec --no-startup-id sway-alttab-gui daemon
bindsym Mod1+Tab exec sway-alttab-gui show
```

For first time usage: reload your Sway configuration and run the daemon manually with `sway-alttab-gui daemon`

`sway-alttab-gui daemon` can optionally take:
* `--mode all`: to list windows across all workspaces
* `--verbose`: to enable verbose logging
* `--release-key <KEY>`: which modifier's release closes the switcher (default `alt`). Accepts `alt`/`mod1`, `super`/`mod4`/`win`, `ctrl`/`control`

`--release-key` will match the modifier key name in Sway, meaning to bind to Super/Win, you can do:

```bash
set $mod Mod4
exec --no-startup-id sway-alttab-gui daemon --release-key $mod
bindsym $mod+Tab exec sway-alttab-gui show
```

