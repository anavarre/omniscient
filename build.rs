use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};

const SINGLETONS: &[&str] = &["Theme", "Store", "Icons"];

fn qml_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir).expect("qml dir").flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            qml_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "qml") {
            out.push(path);
        }
    }
}

fn main() {
    println!("cargo:rerun-if-changed=qml");
    let mut files = vec![];
    qml_files(std::path::Path::new("qml"), &mut files);
    let qml: Vec<QmlFile> = files
        .iter()
        .map(|p| {
            let stem = p.file_stem().unwrap().to_string_lossy();
            QmlFile::from(p.to_string_lossy().into_owned()).singleton(SINGLETONS.contains(&stem.as_ref()))
        })
        .collect();
    CxxQtBuilder::new_qml_module(QmlModule::new("Omniscient").qml_files(qml))
        .qt_module("Quick")
        .qt_module("QuickControls2")
        .qt_module("QuickShapes")
        .include_dir("src/bridge")
        .files(["src/bridge/app.rs", "src/bridge/theme.rs"])
        .build();
}
