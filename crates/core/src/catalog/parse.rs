//! Lecture et validation stricte des deux catalogues.
//!
//! - Champ obligatoire absent ou de mauvais type → l'entrée est ignorée et journalisée.
//! - Lien facultatif invalide → ce lien seul est retiré (journalisé).
//! - Téléchargement invalide (adresse non HTTPS, empreinte mal formée…) → retiré :
//!   l'outil s'affiche « Bientôt » plutôt que de proposer un fichier douteux.
//! - Champs inconnus : ignorés (le site en ajoute régulièrement).

use super::model::{Download, Localized, Source, Status, Tool};
use super::web_urls;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;
use url::Url;

/// Dernière version du format de catalogue connue par ce launcher.
pub const KNOWN_SCHEMA_VERSION: u64 = 2;
/// Base des adresses relatives (`pageUrl: "/apps/…"`).
pub const SITE_ORIGIN: &str = "https://bworkshop.app";
/// Launcher Mac : les téléchargements lus sont les versions Mac (`downloadMac`…).
const FOR_MAC: bool = cfg!(target_os = "macos");
/// Rang des plugins communautaires (après les officiels, dans l'ordre du catalogue).
const COMMUNITY_ORDER_BASE: i64 = 10_000;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("JSON illisible : {0}")]
    Json(String),
    #[error("schemaVersion {found} inconnu : une nouvelle version du launcher est nécessaire")]
    NewerSchema { found: u64 },
    #[error("format invalide : {0}")]
    Invalid(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed {
    pub tools: Vec<Tool>,
    /// Nombre d'entrées ignorées car invalides.
    pub ignored: usize,
}

// ---------- Formats bruts ----------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawDownload {
    url: String,
    size_bytes: Option<u64>,
    sha256: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawOfficialApp {
    id: String,
    name: String,
    author: String,
    #[serde(default)]
    kinds: Vec<String>,
    #[serde(default)]
    platforms: Vec<String>,
    status: Status,
    version: Option<String>,
    category: String,
    order: i64,
    tagline: Option<Localized>,
    description: Option<Localized>,
    download: Option<RawDownload>,
    /// Version Mac (.dmg), lue seulement par le launcher Mac. Les launchers plus anciens
    /// ignorent ce champ : `download` reste le fichier Windows.
    download_mac: Option<RawDownload>,
    page_url: Option<String>,
    source_url: Option<String>,
    web_url: Option<String>,
    overlay_url: Option<String>,
    icon_url: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCommunityPlugin {
    slug: String,
    name: String,
    version: String,
    category: String,
    #[serde(default)]
    kinds: Vec<String>,
    #[serde(default)]
    platforms: Vec<String>,
    author: String,
    download_url: Option<String>,
    /// Version Mac (.dmg) et son empreinte, lues seulement par le launcher Mac.
    download_url_mac: Option<String>,
    sha256_mac: Option<String>,
    web_url: Option<String>,
    overlay_url: Option<String>,
    source_url: Option<String>,
    sha256: Option<String>,
    virustotal_url: Option<String>,
    page_url: Option<String>,
    updated_at: Option<String>,
    icon_url: Option<String>,
}

// ---------- Validation ----------

fn check_id(id: &str) -> Result<(), String> {
    let ok = !id.is_empty()
        && id.len() <= 64
        && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    if ok { Ok(()) } else { Err(format!("identifiant invalide « {id} »")) }
}

fn check_text(field: &str, value: &str) -> Result<String, String> {
    let v = value.trim();
    if v.is_empty() { Err(format!("champ « {field} » vide")) } else { Ok(v.to_string()) }
}

fn check_version(v: &str) -> Result<String, String> {
    semver::Version::parse(v.trim())
        .map(|p| p.to_string())
        .map_err(|_| format!("version non semver « {v} »"))
}

/// Adresse HTTPS absolue, sans identifiants.
fn https_url(raw: &str) -> Result<String, String> {
    let url = Url::parse(raw.trim()).map_err(|_| format!("adresse invalide « {raw} »"))?;
    if url.scheme() != "https" || url.host_str().is_none() {
        return Err(format!("adresse non HTTPS « {raw} »"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("adresse avec identifiants".into());
    }
    Ok(url.to_string())
}

/// Accepte aussi un chemin relatif au site (`/apps/x/`).
fn page_url(raw: &str) -> Result<String, String> {
    let raw = raw.trim();
    if raw.starts_with('/') && !raw.starts_with("//") {
        https_url(&format!("{SITE_ORIGIN}{raw}"))
    } else {
        https_url(raw)
    }
}

/// Icône : HTTPS (ou chemin relatif au site) et domaine autorisé, comme les téléchargements.
fn icon_url(raw: &str) -> Result<String, String> {
    let url = page_url(raw)?;
    crate::security::domains::check_initial_str(&url).map_err(|e| e.to_string())?;
    Ok(url)
}

fn check_sha256(raw: &str) -> Result<String, String> {
    let v = raw.trim().to_ascii_lowercase();
    if v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(v)
    } else {
        Err("empreinte SHA-256 mal formée".into())
    }
}

/// Lien facultatif : retiré (et journalisé) s'il est invalide.
fn optional_link(id: &str, field: &str, raw: Option<String>, check: fn(&str) -> Result<String, String>) -> Option<String> {
    let raw = raw?;
    if raw.trim().is_empty() {
        return None;
    }
    match check(&raw) {
        Ok(v) => Some(v),
        Err(e) => {
            log::warn!("Outil « {id} » : lien « {field} » retiré ({e})");
            None
        }
    }
}

fn build_download(id: &str, url: Option<String>, size: Option<u64>, sha: Option<String>) -> Option<Download> {
    let url = url?;
    let result = (|| {
        let url = https_url(&url)?;
        // Lien qu'aucun téléchargement n'accepterait : l'outil s'affiche sans bouton
        // d'installation (la fiche reste accessible) plutôt qu'avec un bouton en échec.
        crate::security::domains::check_initial_str(&url).map_err(|e| e.to_string())?;
        let sha256 = match sha.as_deref().map(str::trim) {
            None | Some("") => None,
            Some(s) => Some(check_sha256(s)?),
        };
        if size == Some(0) {
            return Err("taille annoncée nulle".to_string());
        }
        Ok(Download { url, size_bytes: size, sha256 })
    })();
    match result {
        Ok(d) => Some(d),
        Err(e) => {
            log::warn!("Outil « {id} » : téléchargement retiré ({e})");
            None
        }
    }
}

fn web_url_for(id: &str, kinds: &[String], raw: Option<String>, fallback: Option<&str>) -> Option<String> {
    if !kinds.iter().any(|k| k == "web") {
        return None;
    }
    let from_catalog = optional_link(id, "webUrl", raw, https_url);
    from_catalog.or_else(|| fallback.map(str::to_string))
}

/// Installable depuis le launcher : un lien accepté **et** une empreinte SHA-256 (sans elle,
/// l'installation est toujours refusée : pas de bouton qui échouerait à coup sûr).
fn installable(download: &Option<Download>) -> bool {
    download.as_ref().is_some_and(|d| d.sha256.is_some())
}

/// Version overlay : champ `overlayUrl` du catalogue, sinon déduite de la version web (voir
/// `web_urls::overlay_for`). Le champ est accepté s'il est sur le même hôte que la version
/// web (même règle qu'elle), sinon seulement sur un domaine autorisé.
fn overlay_url_for(id: &str, raw: Option<String>, web_url: Option<&str>) -> Option<String> {
    let web_host = web_url.and_then(|w| Url::parse(w).ok()).and_then(|u| u.host_str().map(str::to_ascii_lowercase));
    let checked = raw.filter(|r| !r.trim().is_empty()).and_then(|r| {
        let same_host = https_url(&r)
            .ok()
            .filter(|u| Url::parse(u).ok().and_then(|p| p.host_str().map(str::to_ascii_lowercase)) == web_host && web_host.is_some());
        same_host.or_else(|| optional_link(id, "overlayUrl", Some(r), icon_url))
    });
    checked.or_else(|| web_url.and_then(|w| web_urls::overlay_for(id, w)))
}

impl RawOfficialApp {
    fn into_tool(self) -> Result<Tool, String> {
        check_id(&self.id)?;
        let id = self.id;
        let version = self.version.as_deref().map(check_version).transpose()?;
        let raw = if FOR_MAC { self.download_mac } else { self.download };
        let download = raw.and_then(|d| build_download(&id, Some(d.url), d.size_bytes, d.sha256));
        let available = installable(&download) && self.status != Status::InDevelopment;
        let web_url = web_url_for(&id, &self.kinds, self.web_url, web_urls::official_fallback(&id));
        let overlay_url = web_url.as_deref().and_then(|w| overlay_url_for(&id, self.overlay_url, Some(w)));
        Ok(Tool {
            name: check_text("name", &self.name)?,
            author: check_text("author", &self.author)?,
            category: check_text("category", &self.category)?,
            web_url,
            overlay_url,
            icon_url: optional_link(&id, "iconUrl", self.icon_url, icon_url),
            page_url: optional_link(&id, "pageUrl", self.page_url, page_url),
            source_url: optional_link(&id, "sourceUrl", self.source_url, https_url),
            virustotal_url: None,
            updated_at: None,
            source: Source::Official,
            kinds: self.kinds,
            platforms: self.platforms,
            status: self.status,
            version,
            order: self.order,
            tagline: self.tagline,
            description: self.description,
            download,
            available,
            id,
        })
    }
}

impl RawCommunityPlugin {
    fn into_tool(self, index: usize) -> Result<Tool, String> {
        check_id(&self.slug)?;
        let id = self.slug;
        let version = Some(check_version(&self.version)?);
        // Pas de taille annoncée côté communauté (plafond appliqué au téléchargement).
        let download = if FOR_MAC {
            build_download(&id, self.download_url_mac, None, self.sha256_mac)
        } else {
            build_download(&id, self.download_url, None, self.sha256)
        };
        let web_url = web_url_for(&id, &self.kinds, self.web_url, None);
        let overlay_url = web_url.as_deref().and_then(|w| overlay_url_for(&id, self.overlay_url, Some(w)));
        Ok(Tool {
            name: check_text("name", &self.name)?,
            author: check_text("author", &self.author)?,
            category: check_text("category", &self.category)?,
            web_url,
            overlay_url,
            icon_url: optional_link(&id, "iconUrl", self.icon_url, icon_url),
            page_url: optional_link(&id, "pageUrl", self.page_url, page_url),
            source_url: optional_link(&id, "sourceUrl", self.source_url, https_url),
            virustotal_url: optional_link(&id, "virustotalUrl", self.virustotal_url, https_url),
            updated_at: self.updated_at,
            source: Source::Community,
            kinds: self.kinds,
            platforms: self.platforms,
            // Le catalogue communautaire n'a pas de statut.
            status: Status::Stable,
            version,
            order: COMMUNITY_ORDER_BASE + index as i64,
            tagline: None,
            description: None,
            available: installable(&download),
            download,
            id,
        })
    }
}

// ---------- Lecture ----------

fn list<'a>(root: &'a Value, key: &str) -> Result<&'a Vec<Value>, ParseError> {
    let version = root
        .get("schemaVersion")
        .and_then(Value::as_u64)
        .ok_or_else(|| ParseError::Invalid("schemaVersion absent".into()))?;
    if version > KNOWN_SCHEMA_VERSION {
        return Err(ParseError::NewerSchema { found: version });
    }
    if version < KNOWN_SCHEMA_VERSION {
        return Err(ParseError::Invalid(format!("schemaVersion {version} non pris en charge")));
    }
    root.get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| ParseError::Invalid(format!("liste « {key} » absente")))
}

fn collect<F>(label: &str, items: &[Value], id_key: &str, mut convert: F) -> Parsed
where
    F: FnMut(usize, Value) -> Result<Tool, String>,
{
    let mut tools = Vec::new();
    let mut ignored = 0;
    let mut seen = HashSet::new();
    for (i, item) in items.iter().enumerate() {
        let label_id = item.get(id_key).and_then(Value::as_str).unwrap_or("?").to_string();
        match convert(i, item.clone()) {
            Ok(tool) if seen.insert(tool.id.clone()) => tools.push(tool),
            Ok(tool) => {
                ignored += 1;
                log::warn!("{label} : entrée n°{i} ignorée (identifiant « {} » en double)", tool.id);
            }
            Err(e) => {
                ignored += 1;
                log::warn!("{label} : entrée n°{i} (« {label_id} ») ignorée : {e}");
            }
        }
    }
    Parsed { tools, ignored }
}

pub fn parse_official(bytes: &[u8]) -> Result<Parsed, ParseError> {
    let root: Value = serde_json::from_slice(bytes).map_err(|e| ParseError::Json(e.to_string()))?;
    let items = list(&root, "apps")?;
    Ok(collect("Catalogue officiel", items, "id", |_, v| {
        serde_json::from_value::<RawOfficialApp>(v)
            .map_err(|e| e.to_string())
            .and_then(RawOfficialApp::into_tool)
    }))
}

pub fn parse_community(bytes: &[u8]) -> Result<Parsed, ParseError> {
    let root: Value = serde_json::from_slice(bytes).map_err(|e| ParseError::Json(e.to_string()))?;
    let items = list(&root, "communityPlugins")?;
    Ok(collect("Catalogue communautaire", items, "slug", |i, v| {
        serde_json::from_value::<RawCommunityPlugin>(v)
            .map_err(|e| e.to_string())
            .and_then(|raw| raw.into_tool(i))
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Sur Mac, les champs de téléchargement des exemples deviennent leurs versions Mac
    /// (`download` → `downloadMac`…) : les mêmes tests vérifient les deux launchers.
    fn for_platform(mut root: Value) -> Value {
        if !FOR_MAC {
            return root;
        }
        for list in ["apps", "communityPlugins"] {
            if let Some(items) = root.get_mut(list).and_then(Value::as_array_mut) {
                for item in items.iter_mut().filter_map(Value::as_object_mut) {
                    for (from, to) in [("download", "downloadMac"), ("downloadUrl", "downloadUrlMac"), ("sha256", "sha256Mac")] {
                        if let Some(v) = item.remove(from) {
                            item.insert(to.to_string(), v);
                        }
                    }
                }
            }
        }
        root
    }

    fn to_vec_p(v: Value) -> serde_json::Result<Vec<u8>> {
        serde_json::to_vec(&for_platform(v))
    }

    fn official(apps: Value) -> Vec<u8> {
        to_vec_p(json!({"schemaVersion": 2, "apps": apps})).unwrap()
    }

    fn app(id: &str) -> Value {
        json!({
            "id": id, "name": "Outil", "author": "Bworkshop", "kinds": ["application", "web"],
            "platforms": ["windows"], "status": "beta", "version": "0.1.0", "category": "combat",
            "order": 1, "tagline": {"fr": "Salut", "en": "Hi"},
            "download": {"url": "https://bworkshop.app/a.exe", "sizeBytes": 10, "sha256": "AB".repeat(32)},
            "pageUrl": "/apps/x/", "champInconnu": [1, 2]
        })
    }

    #[test]
    fn entree_valide() {
        let p = parse_official(&official(json!([app("assistant-forge")]))).unwrap();
        assert_eq!(p.ignored, 0);
        let t = &p.tools[0];
        assert_eq!(t.page_url.as_deref(), Some("https://bworkshop.app/apps/x/"));
        assert_eq!(t.download.as_ref().unwrap().sha256.as_deref(), Some("ab".repeat(32).as_str()));
        assert_eq!(t.web_url.as_deref(), Some("https://bworkshop.app/forge-app/"));
        assert!(t.available);
    }

    #[test]
    fn entree_invalide_ignoree_sans_bloquer_les_autres() {
        let mut sans_nom = app("b");
        sans_nom.as_object_mut().unwrap().remove("name");
        let mut mauvais_statut = app("c");
        mauvais_statut["status"] = json!("archive");
        let mut mauvaise_version = app("d");
        mauvaise_version["version"] = json!("v1");
        let mut mauvais_id = app("e");
        mauvais_id["id"] = json!("Pas Bon");
        let p = parse_official(&official(json!([
            app("a"), sans_nom, mauvais_statut, mauvaise_version, mauvais_id, app("a"), "texte"
        ])))
        .unwrap();
        assert_eq!(p.tools.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), vec!["a"]);
        assert_eq!(p.ignored, 6);
    }

    #[test]
    fn telechargement_douteux_retire() {
        let mut http = app("a");
        http["download"]["url"] = json!("http://bworkshop.app/a.exe");
        let mut sha = app("b");
        sha["download"]["sha256"] = json!("1234");
        let p = parse_official(&official(json!([http, sha]))).unwrap();
        assert!(p.tools.iter().all(|t| t.download.is_none() && !t.available));
    }

    #[test]
    fn en_developpement_ou_sans_download_bientot() {
        let mut dev = app("a");
        dev["status"] = json!("en-developpement");
        let mut sans = app("b");
        sans.as_object_mut().unwrap().remove("download");
        let p = parse_official(&official(json!([dev, sans]))).unwrap();
        assert!(p.tools.iter().all(|t| !t.available));
    }

    #[test]
    fn web_url_du_catalogue_prioritaire_sur_la_table() {
        let mut a = app("assistant-forge");
        a["webUrl"] = json!("https://bworkshop.app/nouvelle-forge/");
        let p = parse_official(&official(json!([a]))).unwrap();
        assert_eq!(p.tools[0].web_url.as_deref(), Some("https://bworkshop.app/nouvelle-forge/"));
    }

    #[test]
    fn pas_de_widget_sans_version_web() {
        let mut a = app("assistant-forge");
        a["kinds"] = json!(["application"]);
        let p = parse_official(&official(json!([a]))).unwrap();
        assert_eq!(p.tools[0].web_url, None);
    }

    #[test]
    fn icones() {
        let mut relative = app("a");
        relative["iconUrl"] = json!("/assets/icons/forge.webp");
        let mut ailleurs = app("b");
        ailleurs["iconUrl"] = json!("https://exemple.com/icone.png");
        let mut http = app("c");
        http["iconUrl"] = json!("http://bworkshop.app/icone.png");
        let p = parse_official(&official(json!([relative, ailleurs, http, app("d")]))).unwrap();
        let icons: Vec<_> = p.tools.iter().map(|t| t.icon_url.as_deref()).collect();
        assert_eq!(icons, vec![Some("https://bworkshop.app/assets/icons/forge.webp"), None, None, None]);
        assert_eq!(p.ignored, 0, "une icône refusée ne retire pas l'outil");
    }

    #[test]
    fn schema_version() {
        let newer = to_vec_p(json!({"schemaVersion": 3, "apps": []})).unwrap();
        assert_eq!(parse_official(&newer), Err(ParseError::NewerSchema { found: 3 }));
        let older = to_vec_p(json!({"schemaVersion": 1, "apps": []})).unwrap();
        assert!(matches!(parse_official(&older), Err(ParseError::Invalid(_))));
        let absent = to_vec_p(json!({"apps": []})).unwrap();
        assert!(matches!(parse_official(&absent), Err(ParseError::Invalid(_))));
        assert!(matches!(parse_official(b"<html>"), Err(ParseError::Json(_))));
    }

    #[test]
    fn communaute() {
        let body = to_vec_p(json!({"schemaVersion": 2, "communityPlugins": [{
            "slug": "grimoire-boss", "name": "Grimoire Boss", "version": "0.1.0", "category": "combat",
            "kinds": ["overlay", "web"], "platforms": ["windows", "web"], "author": "Rubicon",
            "downloadUrl": "https://github.com/rubiconte/GrimoireBoss/releases/download/v/a.exe",
            "webUrl": "https://bworkshop.app/tacle-retrait-app/", "sha256": "0".repeat(64),
            "pageUrl": "/plugins/grimoire-boss/", "virustotalUrl": "https://www.virustotal.com/gui/url/x"
        }]}))
        .unwrap();
        let p = parse_community(&body).unwrap();
        let t = &p.tools[0];
        assert_eq!(t.source, Source::Community);
        assert_eq!(t.web_url.as_deref(), Some("https://bworkshop.app/tacle-retrait-app/"));
        assert_eq!(t.overlay_url.as_deref(), Some("https://bworkshop.app/tacle-retrait-app/?widget=1"));
        assert_eq!(t.page_url.as_deref(), Some("https://bworkshop.app/plugins/grimoire-boss/"));
        assert_eq!(t.download.as_ref().unwrap().size_bytes, None);
        assert!(t.order >= COMMUNITY_ORDER_BASE);
    }

    #[test]
    fn communaute_autre_auteur_github(){
        let plugin = |url: &str| to_vec_p(json!({"schemaVersion": 2, "communityPlugins": [{
            "slug": "rorganizer", "name": "ROrganizer", "version": "1.6.0", "category": "utilitaire",
            "kinds": ["application"], "platforms": ["windows"], "author": "Loulouw",
            "downloadUrl": url, "sha256": "b".repeat(64), "pageUrl": "/plugins/rorganizer/"
        }]})).unwrap();
        // Fichier de release : installable.
        let p = parse_community(&plugin("https://github.com/Loulouw/ROrganizer/releases/download/v1.6.0/ROrganizer-1.6.0-x86_64-windows.exe")).unwrap();
        assert!(p.tools[0].available);
        // Page de release ou site inconnu : pas de bouton d'installation, la fiche reste.
        for url in ["https://github.com/Loulouw/ROrganizer/releases/tag/v1.6.0", "https://evil.example/a.exe"] {
            let p = parse_community(&plugin(url)).unwrap();
            assert!(p.tools[0].download.is_none() && !p.tools[0].available, "{url}");
            assert!(p.tools[0].page_url.is_some());
        }
    }

    #[test]
    fn sans_empreinte_pas_de_bouton_installer() {
        let p = parse_community(&to_vec_p(json!({"schemaVersion": 2, "communityPlugins": [{
            "slug": "sans-sha", "name": "Sans SHA", "version": "1.0.0", "category": "utilitaire",
            "kinds": ["application"], "platforms": ["windows"], "author": "X",
            "downloadUrl": "https://github.com/SiclUve/x/releases/download/v1/x.exe", "pageUrl": "/plugins/sans-sha/"
        }]})).unwrap())
        .unwrap();
        // Le lien est gardé (fiche), mais l'installation serait refusée : pas de bouton.
        assert!(p.tools[0].download.is_some());
        assert!(!p.tools[0].available);
    }

    #[test]
    fn version_mac_choisie_selon_la_plateforme() {
        let mut a = app("simulateur-harebourg");
        a["downloadMac"] = json!({"url": "https://bworkshop.app/a.dmg", "sizeBytes": 20, "sha256": "cd".repeat(32)});
        let body = serde_json::to_vec(&json!({"schemaVersion": 2, "apps": [a, app("bworklingo")]})).unwrap();
        let p = parse_official(&body).unwrap();
        let (with_mac, without) = (&p.tools[0], &p.tools[1]);
        if cfg!(target_os = "macos") {
            assert_eq!(with_mac.download.as_ref().unwrap().url, "https://bworkshop.app/a.dmg");
            assert!(with_mac.available);
            // Sans version Mac : pas de bouton d'installation sur Mac (la version web reste).
            assert!(without.download.is_none() && !without.available);
        } else {
            assert_eq!(with_mac.download.as_ref().unwrap().url, "https://bworkshop.app/a.exe");
            assert!(without.available);
        }
        let body = serde_json::to_vec(&json!({"schemaVersion": 2, "communityPlugins": [{
            "slug": "grimoire", "name": "Grimoire", "version": "0.2.0", "category": "combat",
            "author": "Rubicon", "downloadUrl": "https://bworkshop.app/g.exe", "sha256": "0".repeat(64),
            "downloadUrlMac": "https://bworkshop.app/g.dmg", "sha256Mac": "1".repeat(64)
        }]}))
        .unwrap();
        let t = &parse_community(&body).unwrap().tools[0];
        let expected = if cfg!(target_os = "macos") { "https://bworkshop.app/g.dmg" } else { "https://bworkshop.app/g.exe" };
        assert_eq!(t.download.as_ref().unwrap().url, expected);
    }
}
