# Sentinel

A Windows UAC-like confirmation dialog for Linux, implemented as a PAM module with a libadwaita GUI. Written in modern C++23 with Wayland-only support and secure session locking.

## Features

- **Graphical confirmation dialog** for privilege escalation
- **Wayland session lock** (`ext-session-lock-v1`) for true secure desktop
  - Screen blanks completely before dialog appears
  - Exclusive input delivery - nothing else can receive input
  - Cannot be hidden by other windows
  - Crash-safe - screen stays locked if helper dies
- **Randomized button positions** to prevent automated clicking
- **Configurable timeout** with auto-deny
- **Minimum display time** to prevent instant automated clicks
- **Per-service configuration** (sudo, su, polkit, etc.)
- **Headless fallback** to standard password authentication
- **Process information display** showing what's requesting privileges
- **Syslog logging** of all confirmation attempts

## Requirements

- **Wayland compositor** with `ext-session-lock-v1` support (Hyprland, Sway, GNOME 44+, KDE Plasma 6+)
- **No X11 support** - this is a Wayland-only application

## Dependencies

### Build Dependencies

```bash
# Debian/Ubuntu
sudo apt install meson ninja-build g++ libpam0g-dev libgtk-4-dev libadwaita-1-dev libgtk4-layer-shell-dev

# Fedora
sudo dnf install meson ninja-build gcc-c++ pam-devel gtk4-devel libadwaita-devel gtk4-layer-shell-devel

# Arch
sudo pacman -S meson ninja gcc pam gtk4 libadwaita gtk4-layer-shell
```

## Building

```bash
meson setup build --prefix=/usr --sysconfdir=/etc --libexecdir=lib
meson compile -C build
```

## Installation

```bash
sudo meson install -C build
```

This installs:
- `/usr/lib/security/pam_sentinel.so` - The PAM module
- `/usr/lib/sentinel-helper` - The GUI helper
- `/etc/security/sentinel.conf` - Configuration file
- `/etc/pam.d/polkit-1` - PAM configuration for polkit (graphical privilege prompts)

## Configuration

### PAM Configuration

Add the module to the services you want to protect. Edit the appropriate file in `/etc/pam.d/`:

#### For sudo (`/etc/pam.d/sudo`)

```
# Add BEFORE the existing auth lines
auth    sufficient  pam_sentinel.so

# Existing lines follow
auth    include     system-auth
...
```

#### For su (`/etc/pam.d/su`)

```
auth    sufficient  pam_sentinel.so
auth    sufficient  pam_rootok.so
...
```

#### For polkit (`/etc/pam.d/polkit-1`)

This file is installed automatically. If you need to customize it:

```
auth    sufficient  pam_sentinel.so
auth    include     system-auth
account include     system-auth
password include    system-auth
session include     system-auth
```

### Module Configuration

Edit `/etc/security/sentinel.conf`:

```ini
[general]
# Enable globally
enabled = yes

# Timeout before auto-deny (seconds)
timeout = 30

# Randomize Allow/Deny button positions
randomize_buttons = yes

# What to do when no display is available
# Options: allow, deny, password
headless_action = password

# Show the executable path of the requesting process
show_process_info = yes

# Log all attempts to syslog
log_attempts = yes

# Minimum time dialog must be shown (milliseconds)
min_display_time = 500

[services]
# Per-service overrides: enabled,timeout,randomize
# Use 'default' to inherit from [general]
sudo = yes,30,yes
su = yes,30,yes
polkit-1 = yes,30,yes
login = no,default,default
```

## How It Works

1. When a PAM-enabled application (sudo, su, etc.) requests authentication, the PAM module is invoked
2. The module checks if a graphical display is available
3. If yes, it spawns the helper application which locks the session and shows the confirmation dialog
4. The user must:
   - Wait for the minimum display time
   - Click the "Allow" button (which may be in a random position)
5. The result is passed back to the PAM module
6. If no display is available, it falls back to standard password authentication

## Security Considerations

### Session Lock

The session lock protocol (`ext-session-lock-v1`) ensures:
- Only the confirmation dialog can receive input
- Other applications cannot overlay or intercept the dialog
- The screen stays locked even if the helper crashes

### Button Randomization

By randomizing button positions, automated clicking scripts cannot reliably click "Allow" without human interaction.

### Minimum Display Time

Prevents scripts from instantly clicking the button before a human can read the dialog.

### Headless Fallback

In SSH sessions or other non-graphical environments, the module can be configured to:
- `allow` - Skip confirmation (NOT RECOMMENDED)
- `deny` - Always deny (may lock you out of SSH sudo)
- `password` - Fall back to password authentication (RECOMMENDED)

## Troubleshooting

### "No Wayland display available"

The helper requires a Wayland compositor. Make sure `WAYLAND_DISPLAY` is set in the PAM environment. X11/XWayland is not supported.

### Locked out of sudo

Boot into single-user mode or use a live USB to edit `/etc/pam.d/sudo` and comment out or remove the `pam_sentinel.so` line.

### Debug logging

Check syslog/journal for sentinel messages:

```bash
journalctl -t pam_sentinel
```

## Testing

Before adding to your actual PAM configuration, test the helper directly:

```bash
# With session lock (recommended - provides full security)
/usr/lib/sentinel-helper --timeout 10 --randomize

# Without session lock (for testing in windowed mode)
/usr/lib/sentinel-helper --timeout 10 --randomize --no-session-lock
```

This will show the dialog and print the result (ALLOW/DENY/TIMEOUT) to stdout.

**Note**: When using session lock, your screen will briefly blank and the dialog will have exclusive input. Press Escape or click Deny to exit.

## License

GPL-3.0-or-later

## Acknowledgments

AI was used partly for development of this project.
