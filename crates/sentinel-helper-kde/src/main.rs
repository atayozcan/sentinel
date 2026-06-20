// SPDX-FileCopyrightText: 2025 Atay Özcan <atay@oezcan.me>
// SPDX-License-Identifier: GPL-3.0-or-later
//! KDE Plasma-native confirmation helper for Sentinel.
//!
//! A drop-in alternative to the COSMIC `sentinel-helper`: same CLI flags
//! (parsed via `sentinel_shared::cli`), same `ALLOW`/`DENY`/`TIMEOUT`
//! stdout contract. Renders a Breeze/Kirigami dialog as a
//! `zwlr-layer-shell-v1` overlay (fullscreen, exclusive keyboard) on
//! Plasma/wlroots compositors, falling back to a normal window on
//! Mutter-based desktops.

mod bridge;

use cxx_qt_lib::{QQmlApplicationEngine, QQuickStyle, QString, QUrl};
use cxx_qt_lib_extras::QApplication;
use sentinel_shared::cli::{self, RenderMode};
use std::sync::OnceLock;

/// Parsed CLI args, cached process-wide. Read here for the render-mode
/// decision and by the QObject's `Default` impl when QML instantiates the
/// controller. `get_or_init` makes the ordering robust either way.
static ARGS: OnceLock<cli::Args> = OnceLock::new();

pub fn args() -> &'static cli::Args {
    ARGS.get_or_init(cli::parse)
}

fn main() {
    let a = args();
    let mode = a.effective_render_mode(std::env::var("XDG_CURRENT_DESKTOP").ok().as_deref());

    // Fail safe: this helper is Wayland-only. With no display we can't paint
    // the confirmation, so deny rather than proceed blindly or hang.
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        eprintln!("sentinel-helper-kde: WAYLAND_DISPLAY not set; Wayland-only — denying");
        bridge::finish_deny();
    }

    // Fire the UAC-style audio cue before the GUI spins up.
    play_sound(&a.sound_name);

    if mode == RenderMode::LayerShell {
        // `LayerShellQt::Shell::useLayerShell()` is exactly this qputenv,
        // and the `liblayer-shell.so` Wayland integration ships with Plasma
        // — so no `layer-shell-qt6-devel` and no C++ shim are needed. Must
        // be set before QApplication initializes the Wayland platform.
        //
        // SAFETY: process start, single-threaded, before any Qt setup.
        unsafe { std::env::set_var("QT_WAYLAND_SHELL_INTEGRATION", "layer-shell") };
    }

    // SAFETY: still single-threaded, before Qt init. Two quality-of-life
    // env tweaks for the spawned-helper context:
    //  - Mute KDE's icon-theme chatter ("Icon theme \"X\" not found"): the
    //    user's theme isn't on the helper's search path, but Kirigami.Icon
    //    falls back fine — the warning is just noise on the caller's stderr.
    //  - Render on the GUI thread (`basic`): we exit the process directly
    //    after the verdict, which otherwise tears down the scene-graph render
    //    thread mid-flight and spews "QThreadStorage destroyed" warnings.
    unsafe {
        if std::env::var_os("QT_LOGGING_RULES").is_none() {
            std::env::set_var("QT_LOGGING_RULES", "kf.iconthemes.warning=false");
        }
        if std::env::var_os("QSG_RENDER_LOOP").is_none() {
            std::env::set_var("QSG_RENDER_LOOP", "basic");
        }
    }

    let mut app = QApplication::new();

    // Native Breeze styling for QtQuick.Controls. qqc2-desktop-style needs
    // the QApplication created above; the explicit style keeps the look
    // correct even when the helper is spawned under a minimal env.
    QQuickStyle::set_style(&QString::from("org.kde.desktop"));

    let mut engine = QQmlApplicationEngine::new();

    // Fail safe: if the QML scene fails to instantiate, deny instead of
    // running a windowless event loop until PAM SIGKILLs us. Connected
    // before load() so it fires during the synchronous instantiation.
    if let Some(engine) = engine.as_mut() {
        engine
            .on_object_creation_failed(|_engine, _url| {
                eprintln!("sentinel-helper-kde: QML failed to load — denying");
                bridge::finish_deny();
            })
            .release();
    }

    let entry = match mode {
        RenderMode::LayerShell => "Main.qml",
        RenderMode::Windowed => "Windowed.qml",
    };
    // QML is embedded in the binary (qrc) — tamper-proof and self-contained.
    let url = format!("qrc:/qt/qml/org/sentinel/kde/qml/{entry}");
    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from(url.as_str()));
    }

    if let Some(app) = app.as_mut() {
        app.exec();
    }

    // Reached only if the event loop quit without a verdict (e.g. the
    // surface was torn down). Fail safe: deny.
    bridge::finish_deny();
}

/// UAC-style audio cue. Best-effort and non-blocking — never delays or
/// fails the dialog. Tries libcanberra (theme-aware) first; if it isn't
/// installed, resolves the freedesktop sound *name* to a file ourselves and
/// plays it with whatever PipeWire / PulseAudio / ALSA player is present
/// (so the cue still fires on a stock system without libcanberra).
fn play_sound(name: &str) {
    if name.is_empty() {
        return;
    }
    // 1. canberra-gtk-play resolves the name through the user's sound theme.
    if spawn_detached("canberra-gtk-play", &["-i", name]) {
        return;
    }
    // 2. Fallback: resolve the name to a file and play it directly.
    let Some(file) = resolve_sound_file(name) else {
        return;
    };
    for (player, args) in [
        ("pw-play", &[file.as_str()][..]),
        ("paplay", &[file.as_str()][..]),
        (
            "ffplay",
            &["-nodisp", "-autoexit", "-loglevel", "quiet", file.as_str()][..],
        ),
        ("aplay", &["-q", file.as_str()][..]),
    ] {
        if spawn_detached(player, args) {
            return;
        }
    }
}

/// Resolve a freedesktop sound *name* (e.g. `dialog-warning`) to a playable
/// file. Honors an absolute path verbatim. Searches the freedesktop and
/// Oxygen themes — `dialog-warning.oga` ships with both KDE and GNOME.
fn resolve_sound_file(name: &str) -> Option<String> {
    if name.starts_with('/') {
        return std::path::Path::new(name)
            .is_file()
            .then(|| name.to_string());
    }
    const DIRS: &[&str] = &[
        "/usr/share/sounds/freedesktop/stereo",
        "/usr/local/share/sounds/freedesktop/stereo",
        "/usr/share/sounds/Oxygen/stereo",
    ];
    for dir in DIRS {
        for ext in ["oga", "ogg", "wav"] {
            let path = format!("{dir}/{name}.{ext}");
            if std::path::Path::new(&path).is_file() {
                return Some(path);
            }
        }
    }
    None
}

/// Spawn a silenced audio player **detached** so the cue keeps playing after
/// the dialog exits (the user often clicks Allow before the sound finishes):
/// its own process group, so the caller's terminal/session tearing down can't
/// take it with it. On our exit it's reparented to init, which reaps it — no
/// wait-thread needed (which also kept a live thread around at process::exit).
/// Tries an absolute path first because the helper is spawned with a minimal
/// PATH; returns false if the binary isn't found so the caller tries the next.
fn spawn_detached(bin: &str, args: &[&str]) -> bool {
    use std::os::unix::process::CommandExt;
    for prog in [format!("/usr/bin/{bin}"), bin.to_string()] {
        let spawned = std::process::Command::new(&prog)
            .args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .process_group(0) // own group → survives the dialog's exit + terminal
            .spawn()
            .is_ok();
        if spawned {
            return true;
        }
    }
    false
}
