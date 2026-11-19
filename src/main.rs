//! Omniscient — tasks, projects and collaborators, natively on Omarchy.
//!
//! Rust core (`core`), a thin cxx-qt bridge (`bridge`) and a Qt Quick UI
//! (`qml/`) styled entirely from the live Omarchy theme.

pub mod bridge;
pub mod core;

use std::path::PathBuf;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString, QUrl};

pub const APP_ID: &str = "io.github.anavarre.Omniscient";

const HELP: &str = "\
Omniscient — tasks, projects and collaborators, natively on Omarchy

Usage: omniscient [--db <path>] [--import <rails.sqlite3>]

Options:
  --db <path>        Use this SQLite database for this launch
                     (else $OMNISCIENT_DB, else `db_path` in
                     $XDG_CONFIG_HOME/omniscient/config.toml, else
                     $XDG_DATA_HOME/omniscient/omniscient.sqlite3;
                     Settings → Database location writes the config)
  --import <path>    Copy a Taskestra (Rails) database into an empty one, then exit
  --gallery          Show every UI component (theme check)
  -h, --help         Show this help
  -V, --version      Show the version";

fn main() {
    let mut db_flag: Option<PathBuf> = None;
    let mut import: Option<PathBuf> = None;
    let mut gallery = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--db" => db_flag = args.next().map(PathBuf::from).or(db_flag),
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

    let location = core::location::resolve(db_flag);
    let db_path = location.path.clone();

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

    if !core::location::hold_instance(&db_path) {
        let _ = std::process::Command::new("hyprctl")
            .args(["dispatch", "focuswindow", &format!("class:^({APP_ID})$")])
            .stdout(std::process::Stdio::null())
            .status();
        return;
    }

    // OMNISCIENT_DEBUG_KEYS replays a key sequence then quits (smoke tests).
    // OMNISCIENT_SCREENSHOT saves the window as PNG when the replay ends.
    let replay = std::env::var("OMNISCIENT_DEBUG_KEYS").unwrap_or_default();
    let screenshot = std::env::var("OMNISCIENT_SCREENSHOT").unwrap_or_default();
    let replay = if replay.is_empty() && !screenshot.is_empty() { "wait wait wait".to_string() } else { replay };
    // OMNISCIENT_GEOMETRY=WxH sizes the window (layout checks).
    let geometry = std::env::var("OMNISCIENT_GEOMETRY").unwrap_or_default();
    let _ = bridge::app::LAUNCH.set(bridge::app::Launch { location, replay, screenshot, geometry, gallery });

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
