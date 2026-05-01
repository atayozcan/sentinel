# Screenshots

Screenshots referenced from the project README.

## Capturing

The helper renders a layer-shell overlay (full-screen translucent backdrop +
centered dialog), so a windowed screenshot tool won't capture the full effect.
Recommended:

- **Cosmic / GNOME / KDE**: full-screen screenshot via the keyboard shortcut
  while `just helper-test` is running.
- **Hyprland / Sway**: `grim` against the active output.

## Slots

Drop the captures here with these exact filenames so the README links resolve:

- `dialog-overlay.png` — the layer-shell overlay covering the full screen,
  dialog card centered, backdrop dimming the desktop behind it.
- `dialog-randomized.png` *(optional)* — same screenshot with `--randomize`
  active, showing Allow/Deny in the swapped position.
- `pam-fallback.png` *(optional)* — terminal showing the password fallback
  triggered when `WAYLAND_DISPLAY` is unset (e.g. SSH login).

PNG, ideally ≤1 MB each.
