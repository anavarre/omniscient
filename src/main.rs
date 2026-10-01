//! Taskestra — the Taskestra task app, natively on Omarchy.

mod dates;
mod db;
mod model;
mod theme;
mod ui;

use std::path::PathBuf;

use gtk::{glib, prelude::*};

const APP_ID: &str = "io.github.anavarre.Taskestra";

const HELP: &str = "\
Taskestra — tasks, natively on Omarchy

Usage: taskestra [--db <path>] [--import <rails.sqlite3>]

Options:
  --db <path>        Use this SQLite database
                     (default: $XDG_DATA_HOME/taskestra/taskestra.sqlite3)
  --import <path>    Copy a Taskestra (Rails) database into an empty one, then exit
  -h, --help         Show this help
  -V, --version      Show the version";

fn main() -> glib::ExitCode {
    let mut db_path = db::default_path();
    let mut import: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--db" => db_path = args.next().map(PathBuf::from).unwrap_or(db_path),
            "--import" => import = args.next().map(PathBuf::from),
            "-h" | "--help" => {
                println!("{HELP}");
                return glib::ExitCode::SUCCESS;
            }
            "-V" | "--version" => {
                println!("taskestra {}", env!("CARGO_PKG_VERSION"));
                return glib::ExitCode::SUCCESS;
            }
            other => {
                eprintln!("Unknown argument: {other}\n\n{HELP}");
                return glib::ExitCode::FAILURE;
            }
        }
    }

    if let Some(src) = import {
        return match db::Db::open(&db_path).and_then(|db| db.import_rails(&src)) {
            Ok(summary) => {
                println!("{summary}\nDatabase: {}", db_path.display());
                glib::ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("Import failed: {e}");
                glib::ExitCode::FAILURE
            }
        };
    }

    // Omarchy sets QT/GTK platform themes for other toolkits; this app paints
    // itself from the Omarchy theme, so skip loading a foreign GTK theme.
    unsafe { std::env::set_var("GTK_THEME", "Default") };

    // TASKESTRA_APP_ID lets a test instance run beside the real one (dev aid).
    let app_id = std::env::var("TASKESTRA_APP_ID").unwrap_or_else(|_| APP_ID.to_string());
    let app = gtk::Application::builder().application_id(app_id).build();
    app.connect_activate(move |gapp| {
        // A second launch just focuses the running window.
        if let Some(w) = gapp.active_window() {
            w.present();
            return;
        }
        match db::Db::open(&db_path) {
            Ok(db) => ui::build(gapp, db),
            Err(e) => {
                eprintln!("Cannot open {}: {e}", db_path.display());
                gapp.quit();
            }
        }
    });
    // Arguments were handled above; don't let GApplication re-parse them.
    app.run_with_args::<&str>(&[])
}
