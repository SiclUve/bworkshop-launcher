//! Lecture des deux catalogues avec cache et requêtes conditionnelles (ETag).
//!
//! - Réseau OK, contenu valide → cache mis à jour, état « à jour ».
//! - 304 Not Modified → le cache est réutilisé, état « à jour ».
//! - Hors ligne, contenu invalide → dernier catalogue valide (état « en cache »).
//! - schemaVersion inconnu → dernier catalogue valide + « nouvelle version du launcher nécessaire ».
//!
//! La cadence (démarrage, toutes les 6 h, bouton « Actualiser ») est décidée par l'application.

use super::cache::{self, CachedCatalog};
use super::parse::{self, ParseError, Parsed};
use super::{merge, Tool, COMMUNITY_CATALOG_URL, OFFICIAL_CATALOG_URL};
use serde::Serialize;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Réponse HTTP minimale, pour pouvoir tester sans réseau.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpResponse {
    NotModified,
    Ok { body: Vec<u8>, etag: Option<String> },
}

pub trait HttpGet: Send + Sync {
    /// GET avec `If-None-Match` si `etag` est fourni. Erreur = message lisible.
    fn get(&self, url: &str, etag: Option<&str>) -> Result<HttpResponse, String>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum SourceState {
    /// Vérifié auprès du site à l'instant.
    Fresh,
    /// Dernier catalogue valide connu, utilisé faute de mieux.
    Cached { reason: String },
    /// Rien d'utilisable (premier lancement hors ligne, par exemple).
    Unavailable { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogSnapshot {
    pub tools: Vec<Tool>,
    pub official: SourceState,
    pub community: SourceState,
    /// Un catalogue a un schemaVersion plus récent que celui connu.
    pub launcher_update_required: bool,
    /// Plus ancienne date de vérification réussie des deux sources.
    pub fetched_at: Option<u64>,
    pub ignored_entries: usize,
}

#[derive(Clone, Copy)]
enum Kind {
    Official,
    Community,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Kind::Official => "officiel",
            Kind::Community => "communautaire",
        }
    }
    fn file(self) -> &'static str {
        match self {
            Kind::Official => "catalog-official.json",
            Kind::Community => "catalog-community.json",
        }
    }
    fn parse(self, bytes: &[u8]) -> Result<Parsed, ParseError> {
        match self {
            Kind::Official => parse::parse_official(bytes),
            Kind::Community => parse::parse_community(bytes),
        }
    }
}

struct Outcome {
    tools: Vec<Tool>,
    state: SourceState,
    update_required: bool,
    fetched_at: Option<u64>,
    ignored: usize,
}

pub struct CatalogService<H> {
    http: H,
    cache_dir: PathBuf,
    official_url: String,
    community_url: String,
}

fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl<H: HttpGet> CatalogService<H> {
    pub fn new(http: H, cache_dir: PathBuf) -> Self {
        Self::with_urls(http, cache_dir, OFFICIAL_CATALOG_URL, COMMUNITY_CATALOG_URL)
    }

    pub fn with_urls(http: H, cache_dir: PathBuf, official: &str, community: &str) -> Self {
        Self { http, cache_dir, official_url: official.into(), community_url: community.into() }
    }

    fn url(&self, kind: Kind) -> &str {
        match kind {
            Kind::Official => &self.official_url,
            Kind::Community => &self.community_url,
        }
    }

    /// Catalogue tiré du cache seulement, sans réseau (affichage immédiat au démarrage).
    pub fn load_cached(&self) -> CatalogSnapshot {
        let reason = "pas encore vérifié".to_string();
        self.snapshot(self.cached_outcome(Kind::Official, reason.clone()), self.cached_outcome(Kind::Community, reason))
    }

    /// Interroge le site (requêtes conditionnelles) et met le cache à jour.
    pub fn refresh(&self) -> CatalogSnapshot {
        self.snapshot(self.refresh_source(Kind::Official), self.refresh_source(Kind::Community))
    }

    fn snapshot(&self, official: Outcome, community: Outcome) -> CatalogSnapshot {
        let fetched_at = match (official.fetched_at, community.fetched_at) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        CatalogSnapshot {
            launcher_update_required: official.update_required || community.update_required,
            ignored_entries: official.ignored + community.ignored,
            tools: merge(official.tools, community.tools),
            official: official.state,
            community: community.state,
            fetched_at,
        }
    }

    fn cached_outcome(&self, kind: Kind, reason: String) -> Outcome {
        let cached = cache::read(&self.cache_dir.join(kind.file()));
        let parsed = cached.as_ref().map(|c| (c.fetched_at, kind.parse(c.body.as_bytes())));
        match parsed {
            Some((fetched_at, Ok(p))) => Outcome {
                tools: p.tools,
                state: SourceState::Cached { reason },
                update_required: false,
                fetched_at: Some(fetched_at),
                ignored: p.ignored,
            },
            Some((_, Err(e))) => {
                log::warn!("Cache du catalogue {} inutilisable : {e}", kind.label());
                Self::unavailable(reason)
            }
            None => Self::unavailable(reason),
        }
    }

    fn unavailable(reason: String) -> Outcome {
        Outcome {
            tools: Vec::new(),
            state: SourceState::Unavailable { reason },
            update_required: false,
            fetched_at: None,
            ignored: 0,
        }
    }

    fn refresh_source(&self, kind: Kind) -> Outcome {
        let path = self.cache_dir.join(kind.file());
        let cached = cache::read(&path);
        let etag = cached.as_ref().and_then(|c| c.etag.clone());

        let mut response = self.http.get(self.url(kind), etag.as_deref());
        // 304 sans cache exploitable : on redemande sans ETag.
        if matches!(response, Ok(HttpResponse::NotModified)) && cached.is_none() {
            response = self.http.get(self.url(kind), None);
        }

        match response {
            Err(e) => {
                log::warn!("Catalogue {} injoignable : {e}", kind.label());
                self.cached_outcome(kind, format!("hors ligne : {e}"))
            }
            Ok(HttpResponse::NotModified) => {
                let Some(mut c) = cached else {
                    return Self::unavailable("réponse 304 sans cache".into());
                };
                match kind.parse(c.body.as_bytes()) {
                    Ok(p) => {
                        c.fetched_at = now_unix();
                        if let Err(e) = cache::write(&path, &c) {
                            log::warn!("Cache du catalogue {} non mis à jour : {e}", kind.label());
                        }
                        log::info!("Catalogue {} inchangé (304)", kind.label());
                        Outcome {
                            tools: p.tools,
                            state: SourceState::Fresh,
                            update_required: false,
                            fetched_at: Some(c.fetched_at),
                            ignored: p.ignored,
                        }
                    }
                    Err(e) => {
                        log::warn!("Cache du catalogue {} invalide : {e}", kind.label());
                        Self::unavailable(format!("cache invalide : {e}"))
                    }
                }
            }
            Ok(HttpResponse::Ok { body, etag }) => match kind.parse(&body) {
                Ok(p) => {
                    let fetched_at = now_unix();
                    match String::from_utf8(body) {
                        Ok(body) => {
                            let entry = CachedCatalog { etag, fetched_at, body };
                            if let Err(e) = cache::write(&path, &entry) {
                                log::warn!("Cache du catalogue {} non écrit : {e}", kind.label());
                            }
                        }
                        Err(_) => log::warn!("Catalogue {} non UTF-8 : pas mis en cache", kind.label()),
                    }
                    log::info!(
                        "Catalogue {} à jour : {} outil(s), {} entrée(s) ignorée(s)",
                        kind.label(),
                        p.tools.len(),
                        p.ignored
                    );
                    Outcome {
                        tools: p.tools,
                        state: SourceState::Fresh,
                        update_required: false,
                        fetched_at: Some(fetched_at),
                        ignored: p.ignored,
                    }
                }
                Err(ParseError::NewerSchema { found }) => {
                    log::warn!(
                        "Catalogue {} au format {found} : nouvelle version du launcher nécessaire",
                        kind.label()
                    );
                    let mut out =
                        self.cached_outcome(kind, "une nouvelle version du launcher est nécessaire".into());
                    out.update_required = true;
                    out
                }
                Err(e) => {
                    log::warn!("Catalogue {} rejeté : {e}", kind.label());
                    self.cached_outcome(kind, format!("catalogue reçu invalide : {e}"))
                }
            },
        }
    }
}
