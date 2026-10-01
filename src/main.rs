//! Omniscient — tasks, projects and collaborators, natively on Omarchy.
//!
//! Rust core (`core`), a thin cxx-qt bridge (`bridge`) and a Qt Quick UI
//! (`qml/`) styled entirely from the live Omarchy theme.

pub mod bridge;
pub mod core;

use std::os::unix::net::UnixListener;
use std::path::PathBuf;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString, QUrl};

pub const APP_ID: &str = "io.github.anavarre.Omniscient";

const HELP: &str = "\
Omniscient — tasks, projects and collaborators, natively on Omarchy

Usage: omniscient [--db <path>] [--import <rails.sqlite3>]

Options:
  --db <path>        Use this SQLite database
                     (default: $XDG_DATA_HOME/omniscient/omniscient.sqlite3)
  --import <path>    Copy a Taskestra (Rails) database into an empty one, then exit
  --gallery          Show every UI component (theme check)
  -h, --help         Show this help
  -V, --version      Show the version";

/// One window per database: a second launch focuses the first. Returns the
/// listener to hold for the app's lifetime, or None if another instance
/// owns the database.
fn claim_instance(db: &std::path::Path) -> Option<Option<UnixListener>> {
    let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR") else { return Some(None) };
    let tag: u64 = db.to_string_lossy().bytes().fold(1469598103934665603, |h, b| (h ^ b as u64).wrapping_mul(1099511628211));
    let sock = PathBuf::from(runtime).join(format!("omniscient-{tag:016x}.sock"));
    if std::os::unix::net::UnixStream::connect(&sock).is_ok() {
        return None;
    }
    let _ = std::fs::remove_file(&sock);
    Some(UnixListener::bind(&sock).ok())
}

fn main() {
    let mut db_path = core::db::default_path();
    let mut import: Option<PathBuf> = None;
    let mut gallery = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--db" => db_path = args.next().map(PathBuf::from).unwrap_or(db_path),
            "--import" => import = args.next().map(PathBuf::from),
            "--gallery" => gallery = true,
            "-h" | "--help" => {
                println!("{HELP}");
                return;
            }
            "-V" | "--version" => {
                println!("omniscient {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            other => {
                eprintln!("Unknown argument: {other}\n\n{HELP}");
                std::process::exit(2);
            }
        }
    }

    if let Some(src) = import {
        match core::db::Db::open(&db_path).and_then(|db| db.import_rails(&src)) {
            Ok(summary) => println!("{summary}\nDatabase: {}", db_path.display()),
            Err(e) => {
                eprintln!("Import failed: {e}");
                std::process::exit(1);
            }
        }
        return;
    }

    let Some(_instance) = claim_instance(&db_path) else {
        let _ = std::process::Command::new("hyprctl")
            .args(["dispatch", "focuswindow", &format!("class:^({APP_ID})$")])
            .stdout(std::process::Stdio::null())
            .status();
        return;
    };

    // OMNISCIENT_DEBUG_KEYS replays a key sequence then quits (smoke tests).
    // OMNISCIENT_SCREENSHOT saves the window as PNG when the replay ends.
    let replay = std::env::var("OMNISCIENT_DEBUG_KEYS").unwrap_or_default();
    let screenshot = std::env::var("OMNISCIENT_SCREENSHOT").unwrap_or_default();
    let replay = if replay.is_empty() && !screenshot.is_empty() { "wait wait wait".to_string() } else { replay };
    // OMNISCIENT_GEOMETRY=WxH sizes the window (layout checks).
    let geometry = std::env::var("OMNISCIENT_GEOMETRY").unwrap_or_default();
    let _ = bridge::app::LAUNCH.set(bridge::app::Launch { db_path, replay, screenshot, geometry, gallery });

    let mut app = QGuiApplication::new();
    QGuiApplication::set_desktop_file_name(&QString::from(APP_ID));
    if let Some(mut app) = app.as_mut() {
        app.as_mut().set_application_name(&QString::from("Omniscient"));
        app.as_mut().set_application_version(&QString::from(env!("CARGO_PKG_VERSION")));
    }
    let mut engine = QQmlApplicationEngine::new();
    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/Omniscient/qml/Main.qml"));
    }
    let code = app.as_mut().map(|a| a.exec()).unwrap_or(1);
    std::process::exit(code);
}
