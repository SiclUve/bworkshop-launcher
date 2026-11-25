//! Tests du service de catalogue avec un faux client HTTP (aucun accès réseau).

use bworkshop_core::catalog::parse::{parse_community, parse_official};
use bworkshop_core::catalog::{CatalogService, HttpGet, HttpResponse, Source, SourceState};
use serde_json::json;
use std::collections::VecDeque;
use std::sync::Mutex;

const OFFICIAL: &str = "https://bworkshop.app/catalog.json";
const COMMUNITY: &str = "https://bworkshop.app/api/catalog.json";

type Queue = VecDeque<Result<HttpResponse, String>>;

/// Faux client : renvoie des réponses programmées par adresse et note les ETag reçus.
#[derive(Default)]
struct FakeHttp {
    responses: Mutex<Vec<(String, Queue)>>,
    etags_seen: Mutex<Vec<(String, Option<String>)>>,
}

impl FakeHttp {
    fn push(&self, url: &str, r: Result<HttpResponse, String>) {
        let mut all = self.responses.lock().unwrap();
        match all.iter_mut().find(|(u, _)| u == url) {
            Some((_, q)) => q.push_back(r),
            None => all.push((url.into(), VecDeque::from([r]))),
        }
    }
}

impl HttpGet for &FakeHttp {
    fn get(&self, url: &str, etag: Option<&str>) -> Result<HttpResponse, String> {
        self.etags_seen.lock().unwrap().push((url.into(), etag.map(String::from)));
        let mut all = self.responses.lock().unwrap();
        all.iter_mut()
            .find(|(u, _)| u == url)
            .and_then(|(_, q)| q.pop_front())
            .unwrap_or_else(|| Err("pas de réponse programmée".into()))
    }
}

fn ok(body: serde_json::Value, etag: Option<&str>) -> Result<HttpResponse, String> {
    Ok(HttpResponse::Ok { body: serde_json::to_vec(&body).unwrap(), etag: etag.map(String::from) })
}

fn official(ids: &[&str]) -> serde_json::Value {
    let apps: Vec<_> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| {
            json!({"id": id, "name": id, "author": "Bworkshop", "kinds": ["web"], "platforms": ["web"],
                   "status": "stable", "version": "1.0.0", "category": "c", "order": i})
        })
        .collect();
    json!({"schemaVersion": 2, "apps": apps})
}

fn community(slugs: &[&str]) -> serde_json::Value {
    let plugins: Vec<_> = slugs
        .iter()
        .map(|s| json!({"slug": s, "name": s, "version": "0.1.0", "category": "c", "author": "X"}))
        .collect();
    json!({"schemaVersion": 2, "communityPlugins": plugins})
}

fn ids(tools: &[bworkshop_core::catalog::Tool]) -> Vec<&str> {
    tools.iter().map(|t| t.id.as_str()).collect()
}

#[test]
fn catalogues_valides_fusionnes_et_mis_en_cache() {
    let dir = tempfile::tempdir().unwrap();
    let http = FakeHttp::default();
    http.push(OFFICIAL, ok(official(&["a", "b"]), Some("\"v1\"")));
    // « a » existe aussi côté communauté : l'officiel l'emporte.
    http.push(COMMUNITY, ok(community(&["a", "c"]), None));
    let service = CatalogService::new(&http, dir.path().to_path_buf());

    let snap = service.refresh();
    assert_eq!(ids(&snap.tools), vec!["a", "b", "c"]);
    assert_eq!(snap.tools[0].source, Source::Official);
    assert_eq!(snap.official, SourceState::Fresh);
    assert_eq!(snap.community, SourceState::Fresh);
    assert!(!snap.launcher_update_required);

    // Le cache est relu sans réseau.
    let cached = service.load_cached();
    assert_eq!(ids(&cached.tools), vec!["a", "b", "c"]);
    assert!(matches!(cached.official, SourceState::Cached { .. }));
}

#[test]
fn etag_envoye_et_304_reutilise_le_cache() {
    let dir = tempfile::tempdir().unwrap();
    let http = FakeHttp::default();
    http.push(OFFICIAL, ok(official(&["a"]), Some("\"v1\"")));
    http.push(COMMUNITY, ok(community(&[]), None));
    http.push(OFFICIAL, Ok(HttpResponse::NotModified));
    http.push(COMMUNITY, ok(community(&["c"]), None));
    let service = CatalogService::new(&http, dir.path().to_path_buf());

    service.refresh();
    let snap = service.refresh();
    assert_eq!(ids(&snap.tools), vec!["a", "c"]);
    assert_eq!(snap.official, SourceState::Fresh);
    let seen = http.etags_seen.lock().unwrap();
    let official_etags: Vec<_> = seen.iter().filter(|(u, _)| u == OFFICIAL).map(|(_, e)| e.clone()).collect();
    assert_eq!(official_etags, vec![None, Some("\"v1\"".to_string())]);
}

#[test]
fn hors_ligne_avec_cache() {
    let dir = tempfile::tempdir().unwrap();
    let http = FakeHttp::default();
    http.push(OFFICIAL, ok(official(&["a"]), None));
    http.push(COMMUNITY, ok(community(&["c"]), None));
    http.push(OFFICIAL, Err("réseau : délai dépassé".into()));
    http.push(COMMUNITY, Err("réseau : délai dépassé".into()));
    let service = CatalogService::new(&http, dir.path().to_path_buf());

    service.refresh();
    let snap = service.refresh();
    assert_eq!(ids(&snap.tools), vec!["a", "c"]);
    assert!(matches!(snap.official, SourceState::Cached { .. }));
    assert!(snap.fetched_at.is_some());
}

#[test]
fn hors_ligne_sans_cache() {
    let dir = tempfile::tempdir().unwrap();
    let http = FakeHttp::default();
    let service = CatalogService::new(&http, dir.path().to_path_buf());
    let snap = service.refresh();
    assert!(snap.tools.is_empty());
    assert!(matches!(snap.official, SourceState::Unavailable { .. }));
}

#[test]
fn schema_inconnu_garde_le_dernier_catalogue_valide() {
    let dir = tempfile::tempdir().unwrap();
    let http = FakeHttp::default();
    http.push(OFFICIAL, ok(official(&["a"]), Some("\"v1\"")));
    http.push(COMMUNITY, ok(community(&[]), None));
    http.push(OFFICIAL, ok(json!({"schemaVersion": 3, "apps": [], "autre": true}), Some("\"v2\"")));
    http.push(COMMUNITY, ok(community(&[]), None));
    let service = CatalogService::new(&http, dir.path().to_path_buf());

    service.refresh();
    let snap = service.refresh();
    assert!(snap.launcher_update_required);
    assert_eq!(ids(&snap.tools), vec!["a"]);
    // Le cache n'a pas été écrasé par le catalogue inconnu.
    let again = service.load_cached();
    assert_eq!(ids(&again.tools), vec!["a"]);
}

#[test]
fn catalogue_invalide_garde_le_cache() {
    let dir = tempfile::tempdir().unwrap();
    let http = FakeHttp::default();
    http.push(OFFICIAL, ok(official(&["a"]), None));
    http.push(COMMUNITY, ok(community(&[]), None));
    http.push(OFFICIAL, Ok(HttpResponse::Ok { body: b"<html>erreur</html>".to_vec(), etag: None }));
    http.push(COMMUNITY, ok(community(&[]), None));
    let service = CatalogService::new(&http, dir.path().to_path_buf());
    service.refresh();
    let snap = service.refresh();
    assert_eq!(ids(&snap.tools), vec!["a"]);
    assert!(matches!(snap.official, SourceState::Cached { .. }));
}

/// Les vrais catalogues publiés le 24/11/2026 passent la validation sans perte.
#[test]
fn vrais_catalogues_du_site() {
    let off = parse_official(include_bytes!("fixtures/official-2026-11-24.json")).unwrap();
    assert_eq!(off.ignored, 0);
    assert_eq!(
        ids(&off.tools),
        vec!["assistant-forge", "simulateur-harebourg", "bworklingo", "skin-helper", "dofus-photobooth"]
    );
    let forge = &off.tools[0];
    assert_eq!(forge.web_url.as_deref(), Some("https://bworkshop.app/forge-app/"));
    assert_eq!(forge.overlay_url.as_deref(), Some("https://bworkshop.app/forge-app/?overlay=1"));
    assert_eq!(forge.download.as_ref().unwrap().size_bytes, Some(141_743_088));
    let photobooth = off.tools.iter().find(|t| t.id == "dofus-photobooth").unwrap();
    assert!(!photobooth.available, "en développement et sans download : « Bientôt »");

    let com = parse_community(include_bytes!("fixtures/community-2026-11-24.json")).unwrap();
    assert_eq!(com.ignored, 0);
    assert_eq!(ids(&com.tools), vec!["grimoire-boss", "grimoire-compagnon", "grimoire"]);
    assert_eq!(com.tools[0].web_url.as_deref(), Some("https://bworkshop.app/tacle-retrait-app/"));
    assert_eq!(com.tools[0].overlay_url.as_deref(), Some("https://bworkshop.app/tacle-retrait-app/?widget=1"));
    // « Grimoire » n'a pas de version web (pas de kind « web ») : pas de widget.
    assert_eq!(com.tools[2].web_url, None);
}
