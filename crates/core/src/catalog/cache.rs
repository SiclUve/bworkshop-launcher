//! Cache du dernier catalogue valide, avec son ETag.
//!
//! On garde le corps brut : il est revalidé à chaque lecture, avec les règles de
//! la version du launcher qui le relit.

use crate::storage::{self, StorageError};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedCatalog {
    pub etag: Option<String>,
    /// Dernière vérification réussie (secondes depuis 1970).
    pub fetched_at: u64,
    pub body: String,
}

pub fn read(path: &Path) -> Option<CachedCatalog> {
    let bytes = fs::read(path).ok()?;
    match serde_json::from_slice(&bytes) {
        Ok(c) => Some(c),
        Err(e) => {
            log::warn!("Cache de catalogue illisible, ignoré : {e}");
            None
        }
    }
}

pub fn write(path: &Path, cached: &CachedCatalog) -> Result<(), StorageError> {
    storage::write_json_atomic(path, cached)
}
