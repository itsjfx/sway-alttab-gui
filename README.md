# sway-alttab-gui

Windows-style Alt-Tab window switcher for Sway (Wayland)

If you used [sagb/alttab](https://github.com/sagb/alttab) on X11, then this is for you.

## Features

* GTK4 visual window switcher with icons
* MRU (Most Recently Used) window ordering
* Alt+Tab to cycle forward, Shift+Tab to cycle backward
* Alt release to select window
* Can display windows from current workspace or all workspaces
* No special permissions required (no udev rules or input group)

## Quick Start

### Install

If you're using Arch Linux, you can install the packages from the AUR... eventually

1. Install required runtime dependencies
    1. `gtk4`
    2. `gtk4-layer-shell`
    3. Sway (duh)
2. Download the binary from GitHub releases or build from source with `cargo build --release`

### Configuration

```bash
exec --no-startup-id sway-alttab-gui daemon
bindsym Mod1+Tab exec sway-alttab-gui show
```

For first time usage: reload your Sway configuration and run the daemon manually with `sway-alttab-gui daemon`

`sway-alttab-gui daemon` can optionally take:
* `--mode all`: to list windows across all workspaces
* `--verbose`: to enable verbose logging
