//! Catalogue des outils : téléchargement, validation, cache, fusion officiels + communauté.
//!
//! Sources (et rien d'autre) :
//! - <https://bworkshop.app/catalog.json> (outils officiels)
//! - <https://bworkshop.app/api/catalog.json> (plugins communautaires)

pub mod cache;
pub mod icons;
pub mod model;
pub mod parse;
pub mod service;
pub mod web_urls;

pub use model::{Download, Localized, Source, Status, Tool};
pub use service::{CatalogService, CatalogSnapshot, HttpGet, HttpResponse, SourceState};

pub const OFFICIAL_CATALOG_URL: &str = "https://bworkshop.app/catalog.json";
pub const COMMUNITY_CATALOG_URL: &str = "https://bworkshop.app/api/catalog.json";

/// Fusionne les deux listes. En cas d'identifiant commun, l'outil officiel l'emporte.
pub fn merge(official: Vec<Tool>, community: Vec<Tool>) -> Vec<Tool> {
    let mut out = official;
    for tool in community {
        if out.iter().any(|t| t.id == tool.id) {
            log::warn!(
                "Plugin communautaire « {} » ignoré : même identifiant qu'un outil officiel",
                tool.id
            );
        } else {
            out.push(tool);
        }
    }
    out
}
