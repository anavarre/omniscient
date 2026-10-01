//! Everything that isn't UI: storage, list rules, date entry, theme tokens
//! and the controller the QML drives. No Qt types appear here, so all of it
//! is unit-tested with plain `cargo test`.

pub mod controller;
pub mod dates;
pub mod db;
pub mod json;
pub mod model;
pub mod rows;
pub mod theme;
