//! `App` QML singleton: the visible list as a Qt model, plus the
//! controller's `act`/`query` calls.
//!
//! After every action the new row list is diffed against the one QML has
//! and only the difference is announced (insert/remove/move/dataChanged),
//! so a `ListView` keeps its scroll position, current item and running
//! animations while rows update in place.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qmodelindex.h");
        type QModelIndex = cxx_qt_lib::QModelIndex;
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
        include!("cxx-qt-lib/qhash.h");
        type QHash_i32_QByteArray = cxx_qt_lib::QHash<cxx_qt_lib::QHashPair_i32_QByteArray>;
        include!("cxx-qt-lib/qlist.h");
        type QList_i32 = cxx_qt_lib::QList<i32>;
    }

    unsafe extern "C++" {
        include!(<QtCore/QAbstractListModel>);
        type QAbstractListModel;
        include!("keys.h");
        #[cxx_name = "omniscientPostKey"]
        fn post_key_event(key: i32, modifiers: i32, text: &QString);
        #[cxx_name = "omniscientGrab"]
        fn grab_window(path: &QString) -> bool;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[base = QAbstractListModel]
        #[qproperty(QString, state)]
        #[qproperty(QString, replay)]
        #[qproperty(QString, screenshot)]
        #[qproperty(QString, geometry)]
        #[qproperty(bool, gallery)]
        type App = super::AppRust;
    }

    impl cxx_qt::Threading for App {}
    impl cxx_qt::Initialize for App {}

    unsafe extern "RustQt" {
        /// Changes state; returns `{"ok": …}` or `{"error": "…"}`.
        #[qinvokable]
        fn act(self: Pin<&mut App>, name: &QString, args: &QString) -> QString;

        /// Read-only lookup; returns JSON (or `{"error": "…"}`).
        #[qinvokable]
        fn query(self: &App, name: &QString, args: &QString) -> QString;

        /// Row `index` as JSON ("" when out of range): lets QML inspect rows
        /// whose delegates aren't instantiated (keyboard selection).
        #[qinvokable]
        #[cxx_name = "rowAt"]
        fn row_at(self: &App, index: i32) -> QString;

        /// Smoke-test replay: one real key press/release to the window.
        #[qinvokable]
        #[cxx_name = "postKey"]
        fn post_key(self: &App, key: i32, modifiers: i32, text: &QString);

        /// Saves the window as an image (smoke screenshots).
        #[qinvokable]
        fn grab(self: &App, path: &QString) -> bool;

        /// SIGTERM/SIGINT arrived; countdowns are already committed.
        #[qsignal]
        #[cxx_name = "quitRequested"]
        fn quit_requested(self: Pin<&mut App>);

        #[inherit]
        #[cxx_name = "beginInsertRows"]
        unsafe fn begin_insert_rows(self: Pin<&mut App>, parent: &QModelIndex, first: i32, last: i32);
        #[inherit]
        #[cxx_name = "endInsertRows"]
        unsafe fn end_insert_rows(self: Pin<&mut App>);
        #[inherit]
        #[cxx_name = "beginRemoveRows"]
        unsafe fn begin_remove_rows(self: Pin<&mut App>, parent: &QModelIndex, first: i32, last: i32);
        #[inherit]
        #[cxx_name = "endRemoveRows"]
        unsafe fn end_remove_rows(self: Pin<&mut App>);
        #[inherit]
        #[cxx_name = "beginMoveRows"]
        unsafe fn begin_move_rows(
            self: Pin<&mut App>,
            source_parent: &QModelIndex,
            first: i32,
            last: i32,
            destination_parent: &QModelIndex,
            destination_child: i32,
        ) -> bool;
        #[inherit]
        #[cxx_name = "endMoveRows"]
        unsafe fn end_move_rows(self: Pin<&mut App>);
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut App>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut App>);
        #[inherit]
        fn index(self: &App, row: i32, column: i32, parent: &QModelIndex) -> QModelIndex;
        #[inherit]
        #[qsignal]
        #[cxx_name = "dataChanged"]
        fn data_changed(self: Pin<&mut App>, top_left: &QModelIndex, bottom_right: &QModelIndex, roles: &QList_i32);

        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &App, parent: &QModelIndex) -> i32;
        #[cxx_override]
        fn data(self: &App, index: &QModelIndex, role: i32) -> QVariant;
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &App) -> QHash_i32_QByteArray;
    }
}

use core::pin::Pin;
use std::sync::OnceLock;
use std::time::Instant;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QList, QModelIndex, QString, QVariant};
use serde_json::{Value, json};

use crate::core::controller::Controller;
use crate::core::db::Db;
use crate::core::rows::{Op, Row, diff};

const ROLE_ROW: i32 = 0x0101;
const ROLE_KIND: i32 = 0x0102;
const ROLE_KEY: i32 = 0x0103;

/// What `main` decided before the QML engine started.
pub struct Launch {
    pub db_path: std::path::PathBuf,
    pub replay: String,
    pub screenshot: String,
    pub geometry: String,
    pub gallery: bool,
}

pub static LAUNCH: OnceLock<Launch> = OnceLock::new();

struct Item {
    row: Row,
    kind: String,
    /// Converted once; `data()` is called for every visible delegate.
    qrow: QString,
}

impl Item {
    fn new(row: Row) -> Item {
        let kind = serde_json::from_str::<Value>(&row.json)
            .ok()
            .and_then(|v| v.get("kind").and_then(Value::as_str).map(str::to_string))
            .unwrap_or_default();
        let qrow = QString::from(row.json.as_str());
        Item { row, kind, qrow }
    }
}

#[derive(Default)]
pub struct AppRust {
    state: QString,
    replay: QString,
    screenshot: QString,
    geometry: QString,
    gallery: bool,
    ctl: Option<Controller>,
    items: Vec<Item>,
    view: String,
}

fn timing() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("OMNISCIENT_DEBUG_TIMING").is_some())
}

impl cxx_qt::Initialize for qobject::App {
    fn initialize(mut self: Pin<&mut Self>) {
        let launch = LAUNCH.get();
        if let Some(l) = launch {
            self.as_mut().set_replay(QString::from(l.replay.as_str()));
            self.as_mut().set_screenshot(QString::from(l.screenshot.as_str()));
            self.as_mut().set_geometry(QString::from(l.geometry.as_str()));
            self.as_mut().set_gallery(l.gallery);
        }
        let path = launch.map(|l| l.db_path.clone()).unwrap_or_else(crate::core::db::default_path);
        match Db::open(&path).and_then(Controller::new) {
            Ok(ctl) => {
                self.as_mut().rust_mut().ctl = Some(ctl);
                self.as_mut().sync();
            }
            Err(e) => {
                let state = json!({ "fatal": format!("Cannot open {}: {e}", path.display()) });
                self.as_mut().set_state(QString::from(state.to_string().as_str()));
            }
        }

        // SIGTERM (logout, `kill`) and Ctrl-C: commit countdowns, then let
        // QML close the window cleanly.
        let qt = self.qt_thread();
        std::thread::spawn(move || {
            use signal_hook::consts::{SIGINT, SIGTERM};
            let Ok(mut signals) = signal_hook::iterator::Signals::new([SIGTERM, SIGINT]) else { return };
            if signals.forever().next().is_some() {
                let _ = qt.queue(|mut app| {
                    if let Some(ctl) = app.as_mut().rust_mut().ctl.as_mut() {
                        ctl.flush();
                    }
                    app.as_mut().quit_requested();
                });
            }
        });
    }
}

impl qobject::App {
    fn act(mut self: Pin<&mut Self>, name: &QString, args: &QString) -> QString {
        let started = Instant::now();
        let name = name.to_string();
        let args: Value = serde_json::from_str(&args.to_string()).unwrap_or(Value::Null);
        let out = {
            let mut rust = self.as_mut().rust_mut();
            let Some(ctl) = rust.ctl.as_mut() else {
                return QString::from(r#"{"error":"The database could not be opened."}"#);
            };
            match ctl.act(&name, &args) {
                Ok(v) => json!({ "ok": v }),
                Err(e) => json!({ "error": e }),
            }
        };
        let acted = started.elapsed();
        self.as_mut().sync();
        if timing() && name != "tick" {
            eprintln!("act {name}: {acted:?} + sync {:?} ({} rows)", started.elapsed() - acted, self.items.len());
        }
        QString::from(out.to_string().as_str())
    }

    fn query(&self, name: &QString, args: &QString) -> QString {
        let args: Value = serde_json::from_str(&args.to_string()).unwrap_or(Value::Null);
        let out = match self.ctl.as_ref().map(|c| c.query(&name.to_string(), &args)) {
            Some(Ok(v)) => v,
            Some(Err(e)) => json!({ "error": e }),
            None => json!({ "error": "The database could not be opened." }),
        };
        QString::from(out.to_string().as_str())
    }

    fn row_at(&self, index: i32) -> QString {
        usize::try_from(index).ok().and_then(|i| self.items.get(i)).map(|i| i.qrow.clone()).unwrap_or_default()
    }

    fn grab(&self, path: &QString) -> bool {
        qobject::grab_window(path)
    }

    fn post_key(&self, key: i32, modifiers: i32, text: &QString) {
        qobject::post_key_event(key, modifiers, text);
    }

    /// Pushes the controller's current rows and state to QML.
    fn sync(mut self: Pin<&mut Self>) {
        let Some(ctl) = self.ctl.as_ref() else { return };
        let new_rows = ctl.rows();
        let state = QString::from(ctl.state().to_string().as_str());
        let view = ctl.view_key();
        let ops = if view != self.view {
            vec![Op::Reset { rows: new_rows }]
        } else {
            let old: Vec<Row> = self.items.iter().map(|i| i.row.clone()).collect();
            diff(&old, &new_rows)
        };
        self.as_mut().rust_mut().view = view;
        let root = QModelIndex::default();
        for op in ops {
            match op {
                Op::Remove { at, count } => unsafe {
                    self.as_mut().begin_remove_rows(&root, at as i32, (at + count - 1) as i32);
                    self.as_mut().rust_mut().items.drain(at..at + count);
                    self.as_mut().end_remove_rows();
                },
                Op::Insert { at, rows } => unsafe {
                    self.as_mut().begin_insert_rows(&root, at as i32, (at + rows.len() - 1) as i32);
                    let items: Vec<Item> = rows.into_iter().map(Item::new).collect();
                    self.as_mut().rust_mut().items.splice(at..at, items);
                    self.as_mut().end_insert_rows();
                },
                Op::Move { from, to } => unsafe {
                    // Qt's destination is "insert before this row of the
                    // list as it was"; moving up, that is `to`.
                    if self.as_mut().begin_move_rows(&root, from as i32, from as i32, &root, to as i32) {
                        let item = self.as_mut().rust_mut().items.remove(from);
                        self.as_mut().rust_mut().items.insert(to, item);
                        self.as_mut().end_move_rows();
                    }
                },
                Op::Change { at, row } => {
                    self.as_mut().rust_mut().items[at] = Item::new(row);
                    let idx = self.index(at as i32, 0, &root);
                    self.as_mut().data_changed(&idx, &idx, &QList::default());
                }
                Op::Reset { rows } => unsafe {
                    self.as_mut().begin_reset_model();
                    self.as_mut().rust_mut().items = rows.into_iter().map(Item::new).collect();
                    self.as_mut().end_reset_model();
                },
            }
        }
        self.as_mut().set_state(state);
    }

    fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.items.len() as i32
    }

    fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(item) = usize::try_from(index.row()).ok().and_then(|i| self.items.get(i)) else { return QVariant::default() };
        match role {
            ROLE_ROW => QVariant::from(&item.qrow),
            ROLE_KIND => QVariant::from(&QString::from(item.kind.as_str())),
            ROLE_KEY => QVariant::from(&QString::from(item.row.key.as_str())),
            _ => QVariant::default(),
        }
    }

    fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
        roles.insert(ROLE_ROW, QByteArray::from("row"));
        roles.insert(ROLE_KIND, QByteArray::from("kind"));
        roles.insert(ROLE_KEY, QByteArray::from("key"));
        roles
    }
}
