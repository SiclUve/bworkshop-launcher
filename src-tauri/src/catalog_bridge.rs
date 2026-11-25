//! Relie le service de catalogue du cœur à l'application.
//!
//! - Au démarrage : affichage immédiat depuis le cache, puis vérification en arrière-plan.
//! - Ensuite : une vérification toutes les 6 heures, sur un fil qui dort entre deux
//!   (aucune boucle active), ou sur « Actualiser ».

use crate::state::AppState;
use bworkshop_core::catalog::icons::IconCache;
use bworkshop_core::catalog::{CatalogService, CatalogSnapshot};
use std::collections::BTreeMap;
use bworkshop_core::net::HttpClient;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

pub const REFRESH_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
pub const EVENT_UPDATED: &str = "catalog-updated";
/// De nouvelles icônes sont arrivées dans le cache.
pub const EVENT_ICONS: &str = "icons-updated";

pub struct CatalogBridge {
    service: Option<CatalogService<HttpClient>>,
    refreshing: AtomicBool,
    icons: IconCache,
    /// Client séparé : les icônes se téléchargent après le catalogue.
    icon_http: Option<HttpClient>,
}

impl CatalogBridge {
    pub fn new(cache_dir: std::path::PathBuf) -> Self {
        let service = match HttpClient::new() {
            Ok(http) => Some(CatalogService::new(http, cache_dir.clone())),
            Err(e) => {
                log::error!("Client HTTP indisponible : {e}");
                None
            }
        };
        let icons = IconCache::new(cache_dir.join("icons"));
        let icon_http = HttpClient::new().ok();
        Self { service, refreshing: AtomicBool::new(false), icons, icon_http }
    }

    pub fn load_cached(&self) -> Option<CatalogSnapshot> {
        self.service.as_ref().map(CatalogService::load_cached)
    }

    /// Interroge le site. Renvoie `None` si une vérification est déjà en cours.
    fn refresh(&self) -> Option<CatalogSnapshot> {
        let service = self.service.as_ref()?;
        if self.refreshing.swap(true, Ordering::SeqCst) {
            return None;
        }
        let snap = service.refresh();
        self.refreshing.store(false, Ordering::SeqCst);
        Some(snap)
    }
}

impl CatalogBridge {
    /// Icônes déjà en cache, par identifiant d'outil (adresses `data:`).
    pub fn icons_for(&self, snap: &CatalogSnapshot) -> BTreeMap<String, String> {
        snap.tools
            .iter()
            .filter_map(|t| {
                let url = t.icon_url.as_deref()?;
                self.icons.data_url(url).map(|d| (t.id.clone(), d))
            })
            .collect()
    }

    /// Télécharge les icônes manquantes. Renvoie vrai si au moins une est arrivée.
    fn fetch_icons(&self, snap: &CatalogSnapshot) -> bool {
        let Some(http) = self.icon_http.as_ref() else { return false };
        let mut any = false;
        for tool in &snap.tools {
            let Some(url) = tool.icon_url.as_deref() else { continue };
            match self.icons.fetch(http, url) {
                Ok(new) => any |= new,
                Err(e) => log::warn!("Icône de « {} » non récupérée : {e}", tool.id),
            }
        }
        any
    }
}

/// Met l'état à jour et prévient les fenêtres.
fn publish(app: &AppHandle, snap: CatalogSnapshot) {
    let state = app.state::<AppState>();
    if let Ok(mut current) = state.catalog.lock() {
        *current = Some(snap.clone());
    }
    let _ = app.emit(EVENT_UPDATED, &snap);
}

/// Vérification immédiate (bloquante) : à appeler hors du fil de l'interface.
pub fn refresh_now(app: &AppHandle) -> Option<CatalogSnapshot> {
    let bridge = app.state::<Arc<CatalogBridge>>().inner().clone();
    let snap = bridge.refresh()?;
    publish(app, snap.clone());
    if bridge.fetch_icons(&snap) {
        let _ = app.emit(EVENT_ICONS, bridge.icons_for(&snap));
    }
    Some(snap)
}

/// Lance le fil de vérification périodique.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("catalogue".into())
        .spawn(move || loop {
            refresh_now(&app);
            std::thread::sleep(REFRESH_INTERVAL);
        });
    if let Err(e) = spawned {
        log::error!("Fil du catalogue non démarré : {e}");
    }
}
