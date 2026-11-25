//! Cœur du Bworkshop Launcher.
//!
//! Aucun module ici ne dépend de Tauri ni de l'interface : tout est testable
//! avec `cargo test -p bworkshop-core`.

pub mod catalog;
pub mod dofus_tracker;
pub mod geometry;
pub mod hotkeys;
pub mod installer;
pub mod layouts;
pub mod logging;
pub mod net;
pub mod paths;
pub mod security;
pub mod settings;
pub mod storage;
pub mod widgets_store;
