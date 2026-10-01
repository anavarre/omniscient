//! `Omarchy` QML singleton: the live theme tokens as JSON.
//!
//! A background thread watches the places `omarchy-theme-set`,
//! `omarchy-font-set` and `omarchy display text size` write to, debounces
//! the burst of events a switch produces, re-resolves the tokens (running
//! `fc-match` and `hyprctl` off the GUI thread) and queues the result onto
//! the Qt thread. `Theme.qml` turns the JSON into animated properties.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, tokens)]
        /// A new theme waiting to be applied: QML snapshots the window,
        /// copies this into `tokens`, then fades the snapshot out.
        #[qproperty(QString, pending)]
        type Omarchy = super::OmarchyRust;
    }

    impl cxx_qt::Threading for Omarchy {}
    impl cxx_qt::Initialize for Omarchy {}
}

use core::pin::Pin;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use notify::{RecursiveMode, Watcher};

use crate::core::theme;

#[derive(Default)]
pub struct OmarchyRust {
    tokens: QString,
    pending: QString,
    watcher: Option<notify::RecommendedWatcher>,
}

/// Resolves everything, running the two helper processes in parallel.
fn resolve() -> String {
    let rounding = std::thread::spawn(theme::hyprland_rounding);
    let family = theme::monospace_family().unwrap_or_default();
    let radius = rounding.join().ok().flatten().unwrap_or(0);
    theme::load(radius, &family).json()
}

fn relevant(path: &Path, state: &Path) -> bool {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if path.parent() == Some(state) {
        // theme.name is rewritten last on every switch; `theme` is the dir
        // moved into place just before.
        return name == "theme.name" || name == "theme";
    }
    name == "shell.toml" || name.ends_with(".conf") || name == "fontconfig"
}

impl cxx_qt::Initialize for qobject::Omarchy {
    fn initialize(mut self: Pin<&mut Self>) {
        self.as_mut().set_tokens(QString::from(resolve().as_str()));

        let (tx, rx) = mpsc::channel::<PathBuf>();
        let state = theme::state_dir();
        let config = theme::user_shell().parent().map(Path::to_path_buf).unwrap_or_default();
        let fontconfig = theme::fontconfig_dir();
        let watched_state = state.clone();
        let watcher = notify::recommended_watcher(move |ev: notify::Result<notify::Event>| {
            if let Ok(ev) = ev {
                for p in ev.paths {
                    if relevant(&p, &watched_state) {
                        let _ = tx.send(p);
                    }
                }
            }
        });
        let Ok(mut watcher) = watcher else { return };
        let _ = watcher.watch(&state, RecursiveMode::NonRecursive);
        let _ = watcher.watch(&config, RecursiveMode::NonRecursive);
        // fontconfig/ may not exist until the first `omarchy-font-set`;
        // watching ~/.config catches it being created.
        if watcher.watch(&fontconfig, RecursiveMode::NonRecursive).is_err()
            && let Some(parent) = fontconfig.parent()
        {
            let _ = watcher.watch(parent, RecursiveMode::NonRecursive);
        }
        self.as_mut().rust_mut().watcher = Some(watcher);

        let qt = self.qt_thread();
        std::thread::spawn(move || {
            while rx.recv().is_ok() {
                // A switch writes many files in a burst: settle first.
                while rx.recv_timeout(Duration::from_millis(120)).is_ok() {}
                let json = resolve();
                if qt.queue(move |mut o| o.as_mut().set_pending(QString::from(json.as_str()))).is_err() {
                    break;
                }
                // Hyprland re-reads its config asynchronously after
                // `hyprctl reload`; pick up a new rounding a beat later.
                std::thread::sleep(Duration::from_millis(400));
                let json = resolve();
                if qt.queue(move |mut o| o.as_mut().set_pending(QString::from(json.as_str()))).is_err() {
                    break;
                }
            }
        });
    }
}
